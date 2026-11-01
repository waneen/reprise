use crate::ports::*;
use domain::score::{SaveMode, ScoreRecord};
use domain::select::{Library, SongInfo};
use domain::{chart::Chart, random::LegacyMt};
use std::path::{Path, PathBuf};

/// 譜面を読み込み、同じ乱数列を続けてレーン RANDOM に使えるよう rng も返す（LR2 互換）
pub fn load_chart(
    loaders: &[Box<dyn ChartLoader>],
    path: &Path,
    bytes: &[u8],
    seed: u32,
) -> Result<(Chart, LegacyMt), PortError> {
    let l = loaders
        .iter()
        .find(|l| l.can_load(path))
        .ok_or(PortError::Unsupported)?;
    let mut rng = LegacyMt::new(seed);
    let chart = l.load(path, bytes, &mut rng)?;
    Ok((chart, rng))
}

/// ② 取り込み：ドメインのマージ規則を適用して保存
pub fn import_scores(
    importer: &dyn ScoreImporter,
    src: &Path,
    repo: &mut dyn ScoreRepository,
) -> Result<usize, PortError> {
    let recs = importer.import(src)?;
    let n = recs.len();
    for r in recs {
        match repo.find(&r.chart, r.ruleset)? {
            Some(mut cur) => {
                cur.merge_best(&r);
                repo.save(cur)?;
            }
            None => repo.save(r)?,
        }
    }
    Ok(n)
}

/// プレイの保存の結果: 保存前の記録（無ければ None）と保存後の記録
#[derive(Debug, Clone, PartialEq)]
pub struct SavedScore {
    pub before: Option<ScoreRecord>,
    pub after: ScoreRecord,
}

/// プレイの保存（APP-UC-06）。`mode` が None か、PG・GR・GD が 1 つも無ければ（LR2 `Main.cpp`）保存せず None。
/// 既存の記録（無ければ空の記録）に LR2 の更新（DOM-SCR-06, 07）を当てて保存する
pub fn save_play(
    repo: &mut dyn ScoreRepository,
    play: &ScoreRecord,
    mode: Option<SaveMode>,
) -> Result<Option<SavedScore>, PortError> {
    let Some(mode) = mode else {
        return Ok(None);
    };
    if play.pgreat + play.great + play.good == 0 {
        return Ok(None);
    }
    let before = repo.find(&play.chart, play.ruleset)?;
    let mut after = before
        .clone()
        .unwrap_or_else(|| ScoreRecord::empty(play.chart, play.ruleset));
    if after.chart.sha256.is_none() {
        after.chart.sha256 = play.chart.sha256;
    }
    after.update_lr2(play, mode);
    repo.save(after.clone())?;
    Ok(Some(SavedScore { before, after }))
}

/// 選曲の曲一覧を作る（APP-UC-07）。登録したフォルダの中をすべて辿り、読める譜面のメタ情報を集める。
/// メタ情報は乱数の種 0 で読む（`#RANDOM` の中の定義は種で変わりうる）
pub fn scan_library(
    fs: &dyn FileSystem,
    loaders: &[Box<dyn ChartLoader>],
    roots: &[PathBuf],
) -> Library {
    let mut folders = vec![];
    let mut songs = vec![];
    let mut dirs: Vec<PathBuf> = roots.to_vec();
    while let Some(dir) = dirs.pop() {
        for e in fs.list(&dir) {
            if !e.is_file {
                folders.push(e.path.clone());
                dirs.push(e.path);
                continue;
            }
            if !loaders.iter().any(|l| l.can_load(&e.path)) {
                continue;
            }
            let Ok(bytes) = fs.read(&e.path) else {
                continue;
            };
            if let Ok((c, _)) = load_chart(loaders, &e.path, &bytes, 0) {
                let timing = domain::play::TimingMap::from_chart(&c);
                let notes =
                    domain::play::Lr2Play::new(&c, &timing, domain::play::GaugeType::Groove)
                        .map_or(0, |p| p.total_notes());
                songs.push(SongInfo {
                    path: e.path,
                    hash: c.hash,
                    title: c.meta.title,
                    subtitle: c.meta.subtitle,
                    artist: c.meta.artist,
                    genre: c.meta.genre,
                    level: c.meta.level.unwrap_or(0),
                    difficulty: c.meta.difficulty.unwrap_or(-1),
                    key_mode: c.key_mode,
                    notes,
                });
            }
        }
    }
    Library::new(roots.to_vec(), folders, songs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::chart::*;
    use domain::play::RulesetId;
    use domain::score::*;
    use rstest::rstest;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // ---- 偽物 ----
    struct FakeLoader {
        ext: &'static str,
        title: &'static str,
        draws: u32,
    }
    impl ChartLoader for FakeLoader {
        fn can_load(&self, p: &Path) -> bool {
            p.extension().is_some_and(|e| e == self.ext)
        }
        fn load(&self, _p: &Path, _b: &[u8], rng: &mut LegacyMt) -> Result<Chart, PortError> {
            for _ in 0..self.draws {
                rng.next_u32();
            }
            Ok(chart(self.title))
        }
    }
    fn chart(title: &str) -> Chart {
        Chart {
            hash: hash(0),
            meta: ChartMeta {
                title: title.into(),
                ..Default::default()
            },
            key_mode: KeyMode::K7,
            notes: vec![],
            bgm: vec![],
            bga: vec![],
            timing: vec![],
            sounds: vec![],
            images: vec![],
        }
    }
    fn hash(n: u8) -> ChartHash {
        ChartHash {
            md5: [n; 16],
            sha256: None,
        }
    }
    fn record(n: u8, ruleset: RulesetId, ex: u32, plays: u32) -> ScoreRecord {
        ScoreRecord {
            chart: hash(n),
            ruleset,
            lamp: ClearLamp::Easy,
            ex_score: ex,
            pgreat: 0,
            great: 0,
            good: 0,
            bad: 0,
            poor: 0,
            max_combo: 0,
            min_bp: None,
            play_count: plays,
            clear_count: 0,
            origin: ScoreOrigin::ImportedFrom("x".into()),
        }
    }
    struct FakeImporter(Vec<ScoreRecord>);
    impl ScoreImporter for FakeImporter {
        fn source_name(&self) -> &'static str {
            "fake"
        }
        fn import(&self, _s: &Path) -> Result<Vec<ScoreRecord>, PortError> {
            Ok(self.0.clone())
        }
    }
    #[derive(Default)]
    struct MemRepo(HashMap<(ChartHash, RulesetId), ScoreRecord>);
    impl ScoreRepository for MemRepo {
        fn find(&self, c: &ChartHash, r: RulesetId) -> Result<Option<ScoreRecord>, PortError> {
            Ok(self.0.get(&(*c, r)).cloned())
        }
        fn save(&mut self, rec: ScoreRecord) -> Result<(), PortError> {
            self.0.insert((rec.chart, rec.ruleset), rec);
            Ok(())
        }
    }
    fn loaders() -> Vec<Box<dyn ChartLoader>> {
        vec![
            Box::new(FakeLoader {
                ext: "bms",
                title: "first",
                draws: 0,
            }),
            Box::new(FakeLoader {
                ext: "bms",
                title: "second",
                draws: 0,
            }),
            Box::new(FakeLoader {
                ext: "bmson",
                title: "json",
                draws: 0,
            }),
        ]
    }

    /// APP-UC-01: 最初に扱えるローダ
    #[rstest]
    #[case::first_match_wins("a.bms", Some("first"))]
    #[case::other_loader("a.bmson", Some("json"))]
    #[case::unsupported("a.txt", None)]
    fn app_uc_01_first_capable_loader(#[case] path: &str, #[case] title: Option<&str>) {
        let got = load_chart(&loaders(), &PathBuf::from(path), b"", 1);
        match title {
            Some(t) => assert_eq!(got.unwrap().0.meta.title, t),
            None => assert!(matches!(got, Err(PortError::Unsupported))),
        }
    }

    /// APP-UC-02: 返る乱数はローダの消費の続き
    #[rstest]
    #[case::no_draw(0)]
    #[case::two_draws(2)]
    #[case::crosses_regeneration(700)]
    fn app_uc_02_rng_continues_after_loader(#[case] draws: u32) {
        let ls: Vec<Box<dyn ChartLoader>> = vec![Box::new(FakeLoader {
            ext: "bms",
            title: "t",
            draws,
        })];
        let (_, mut rng) = load_chart(&ls, Path::new("a.bms"), b"", 391).unwrap();
        let mut fresh = LegacyMt::new(391);
        for _ in 0..draws {
            fresh.next_u32();
        }
        assert_eq!(rng.next_u32(), fresh.next_u32());
    }

    /// APP-UC-03: 既存が無ければそのまま保存
    #[rstest]
    #[case::lr2(RulesetId::Lr2)]
    #[case::beatoraja(RulesetId::Beatoraja)]
    fn app_uc_03_new_record_saved_as_is(#[case] ruleset: RulesetId) {
        let rec = record(1, ruleset, 500, 3);
        let mut repo = MemRepo::default();
        import_scores(&FakeImporter(vec![rec.clone()]), Path::new("x"), &mut repo).unwrap();
        assert_eq!(repo.find(&hash(1), ruleset).unwrap(), Some(rec));
    }

    /// APP-UC-04: 同じ譜面・ルールはマージ、ルール違いは別記録
    #[rstest]
    #[case::same_ruleset_merged(RulesetId::Lr2, 900, 8)]
    #[case::different_ruleset_separate(RulesetId::Beatoraja, 500, 3)]
    fn app_uc_04_existing_record_merged(
        #[case] imported: RulesetId,
        #[case] ex: u32,
        #[case] plays: u32,
    ) {
        let mut repo = MemRepo::default();
        repo.save(record(1, RulesetId::Lr2, 500, 3)).unwrap();
        import_scores(
            &FakeImporter(vec![record(1, imported, 900, 5)]),
            Path::new("x"),
            &mut repo,
        )
        .unwrap();
        let got = repo.find(&hash(1), RulesetId::Lr2).unwrap().unwrap();
        assert_eq!((got.ex_score, got.play_count), (ex, plays));
    }

    /// APP-UC-05: 件数
    #[rstest]
    #[case::none(0)]
    #[case::three(3)]
    fn app_uc_05_returns_count(#[case] n: u8) {
        let recs = (0..n).map(|i| record(i, RulesetId::Lr2, 1, 1)).collect();
        let got =
            import_scores(&FakeImporter(recs), Path::new("x"), &mut MemRepo::default()).unwrap();
        assert_eq!(got, n as usize);
    }

    fn played(lamp: ClearLamp, ex: u32) -> ScoreRecord {
        ScoreRecord {
            chart: ChartHash {
                md5: [1; 16],
                sha256: Some([9; 32]),
            },
            lamp,
            pgreat: ex / 2,
            min_bp: Some(3),
            play_count: 1,
            origin: ScoreOrigin::Native,
            ..record(1, RulesetId::Lr2, ex, 1)
        }
    }

    /// 保存前の EX と、保存後の (ランプ, EX, プレイ回数, SHA-256 がある)
    type Saved = (Option<u32>, (ClearLamp, u32, u32, bool));

    /// APP-UC-06: プレイの保存。保存しないか PG・GR・GD が無ければ何もしない。既存（無ければ空の記録）に LR2 の更新を当てて保存し、
    /// 前後の記録を返す。SHA-256 が分かっていなければプレイのものを使う
    #[rstest]
    #[case::first(None, Some(SaveMode::Full), Some((None, (ClearLamp::Normal, 100, 1, true))))]
    #[case::update(Some(record(1, RulesetId::Lr2, 500, 3)), Some(SaveMode::Full), Some((Some(500), (ClearLamp::Normal, 500, 4, true))))]
    #[case::lamp_only(None, Some(SaveMode::LampOnly), Some((None, (ClearLamp::Easy, 0, 1, true))))]
    #[case::not_saved(Some(record(1, RulesetId::Lr2, 500, 3)), None, None)]
    #[case::no_good_or_better(Some(record(1, RulesetId::Lr2, 500, 3)), Some(SaveMode::Full), None)]
    fn app_uc_06_save_play(
        #[case] best: Option<ScoreRecord>,
        #[case] mode: Option<SaveMode>,
        #[case] want: Option<Saved>,
    ) {
        let mut repo = MemRepo::default();
        if let Some(b) = &best {
            repo.save(b.clone()).unwrap();
        }
        let mut play = played(ClearLamp::Normal, 100);
        if want.is_none() && mode.is_some() {
            // BD・POOR だけのプレイ
            (play.pgreat, play.great, play.good, play.bad) = (0, 0, 0, 2);
        }
        let got = save_play(&mut repo, &play, mode).unwrap();
        let summary = got.as_ref().map(|s| {
            (
                s.before.as_ref().map(|b| b.ex_score),
                (
                    s.after.lamp,
                    s.after.ex_score,
                    s.after.play_count,
                    s.after.chart.sha256.is_some(),
                ),
            )
        });
        assert_eq!(summary, want);
        let stored = repo.find(&hash(1), RulesetId::Lr2).unwrap();
        assert_eq!(stored, got.map(|s| s.after).or(best));
    }

    /// 中身を題名、1 バイト目を MD5 にするローダ（中身が "x" なら読めない）
    struct BytesLoader;
    impl ChartLoader for BytesLoader {
        fn can_load(&self, p: &Path) -> bool {
            p.extension().is_some_and(|e| e == "bms")
        }
        fn load(&self, _p: &Path, b: &[u8], _rng: &mut LegacyMt) -> Result<Chart, PortError> {
            if b == b"x" {
                return Err(PortError::Parse("x".into()));
            }
            let mut c = chart(&String::from_utf8_lossy(b));
            c.hash = hash(b[0]);
            c.meta.level = Some(3);
            c.meta.difficulty = Some(4);
            Ok(c)
        }
    }
    /// パス → 中身 のファイル（フォルダは置いたファイルから分かる）
    struct MapFs(Vec<(&'static str, &'static [u8])>);
    impl FileSystem for MapFs {
        fn list(&self, dir: &Path) -> Vec<DirEntry> {
            let mut out: Vec<DirEntry> = vec![];
            for (p, _) in &self.0 {
                let Ok(rest) = Path::new(p).strip_prefix(dir) else {
                    continue;
                };
                let mut c = rest.components();
                let Some(first) = c.next() else { continue };
                let path = dir.join(first);
                if out.iter().all(|e| e.path != path) {
                    out.push(DirEntry {
                        name: first.as_os_str().to_string_lossy().into_owned(),
                        path,
                        is_file: c.next().is_none(),
                    });
                }
            }
            out
        }
        fn read(&self, path: &Path) -> Result<Vec<u8>, PortError> {
            self.0
                .iter()
                .find(|(p, _)| Path::new(p) == path)
                .map(|(_, b)| b.to_vec())
                .ok_or_else(|| PortError::Io("none".into()))
        }
        fn is_file(&self, path: &Path) -> bool {
            self.0.iter().any(|(p, _)| Path::new(p) == path)
        }
    }

    /// APP-UC-07: 曲一覧の走査（開いた一覧で確かめる）
    #[rstest]
    #[case::songs("/r", vec!["/r/a/1.bms", "/r/b/2.bms"])]
    #[case::nested_folders("/r2", vec!["[p]"])]
    #[case::deep_songs("/r2/p", vec!["/r2/p/s/4.bms"])]
    fn app_uc_07_scan_library(#[case] open: &str, #[case] want: Vec<&str>) {
        let fs = MapFs(vec![
            ("/r/a/1.bms", b"A"),
            ("/r/b/2.bms", b"B"),
            ("/r/b/readme.txt", b"C"),
            ("/r/b/broken.bms", b"x"),
            ("/r2/p/s/4.bms", b"D"),
        ]);
        let ls: Vec<Box<dyn ChartLoader>> = vec![Box::new(BytesLoader)];
        let lib = scan_library(&fs, &ls, &[PathBuf::from("/r"), PathBuf::from("/r2")]);
        let mut all = domain::select::ListFilter {
            key: 0,
            ..Default::default()
        };
        let bars = lib
            .open(Path::new(open), &mut all, &domain::select::NoLookup)
            .unwrap()
            .bars;
        let got: Vec<String> = bars
            .iter()
            .map(|b| match b {
                domain::select::Bar::Folder { title, .. } => format!("[{title}]"),
                domain::select::Bar::Song(i) => {
                    let s = &lib.songs()[*i];
                    assert_eq!(
                        (s.level, s.difficulty, s.hash.md5[0]),
                        (3, 4, s.title.as_bytes()[0])
                    );
                    // Windows では join した部分が \ 区切りになる
                    s.path.to_string_lossy().replace('\\', "/")
                }
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(got, want);
    }
}
