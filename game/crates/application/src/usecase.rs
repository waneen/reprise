use crate::ports::*;
use domain::{chart::Chart, random::LegacyMt};
use std::path::Path;

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
        match repo.find(&r.chart, r.ruleset) {
            Some(mut cur) => {
                cur.merge_best(&r);
                repo.save(cur);
            }
            None => repo.save(r),
        }
    }
    Ok(n)
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
        fn find(&self, c: &ChartHash, r: RulesetId) -> Option<ScoreRecord> {
            self.0.get(&(*c, r)).cloned()
        }
        fn save(&mut self, rec: ScoreRecord) {
            self.0.insert((rec.chart, rec.ruleset), rec);
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
        assert_eq!(repo.find(&hash(1), ruleset), Some(rec));
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
        repo.save(record(1, RulesetId::Lr2, 500, 3));
        import_scores(
            &FakeImporter(vec![record(1, imported, 900, 5)]),
            Path::new("x"),
            &mut repo,
        )
        .unwrap();
        let got = repo.find(&hash(1), RulesetId::Lr2).unwrap();
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
}
