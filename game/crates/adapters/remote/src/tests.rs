//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;
use std::cell::Cell;

/// (メソッド, 行き先) → (状態, 中身)
type Answers = HashMap<(String, String), (u16, Vec<u8>)>;
/// 来た (メソッド, 行き先, 中身)
type Asked = Vec<(String, String, Option<Vec<u8>>)>;

/// 決まった答えを返し、来た要求を覚える
#[derive(Default)]
struct Fake {
    answers: Mutex<Answers>,
    asked: Arc<Mutex<Asked>>,
}

impl Fake {
    fn answer(&self, method: &str, target: &str, status: u16, body: &str) {
        self.answers.lock().unwrap().insert(
            (method.into(), target.into()),
            (status, body.as_bytes().to_vec()),
        );
    }
}

/// `Fake` を共有する（Remote に渡したあとも答えを替えられるように）
struct Shared(Arc<Fake>);

impl Transport for Shared {
    fn request(&self, method: &str, target: &str, body: Option<&[u8]>) -> (u16, Vec<u8>) {
        self.0
            .asked
            .lock()
            .unwrap()
            .push((method.into(), target.into(), body.map(|b| b.to_vec())));
        self.0
            .answers
            .lock()
            .unwrap()
            .get(&(method.to_string(), target.to_string()))
            .cloned()
            .unwrap_or((404, vec![]))
    }
}

fn remote() -> (Arc<Fake>, Arc<Remote>) {
    let f = Arc::new(Fake::default());
    (f.clone(), Remote::new(Shared(f)))
}

fn asked(f: &Fake) -> Vec<String> {
    f.asked
        .lock()
        .unwrap()
        .iter()
        .map(|(m, t, _)| format!("{m} {t}"))
        .collect()
}

fn song(path: &str, md5: u8, sha: Option<u8>) -> SongInfo {
    SongInfo {
        path: path.into(),
        hash: ChartHash {
            md5: [md5; 16],
            sha256: sha.map(|s| [s; 32]),
        },
        title: format!("t{md5}"),
        subtitle: "sub".into(),
        artist: "a".into(),
        genre: "g".into(),
        level: 3,
        difficulty: 2,
        key_mode: KeyMode::K14,
        notes: 99,
    }
}

fn sample() -> Library {
    Library::new(
        vec!["/r".into(), "/hide".into()],
        vec!["/r/a".into(), "/hide/x".into()],
        vec![
            song("/r/a/1.bms", 1, Some(9)),
            song("/r/a/2.bms", 2, None),
            song("/hide/x/3.bms", 3, None),
        ],
    )
    .with_custom(vec![CustomFolder {
        path: "/r/f.lr2folder".into(),
        title: "F".into(),
        category: "c".into(),
        info_a: "ia".into(),
        info_b: "ib".into(),
        command: "level = 1".into(),
        max: 1,
        kind: 2,
    }])
    .with_tables(vec![DifficultyTable {
        url: "http://t".into(),
        name: "T".into(),
        levels: vec![TableLevel {
            name: "★1".into(),
            charts: vec![TableChart {
                md5: "01".repeat(16),
                sha256: String::new(),
                title: "x".into(),
                artist: "y".into(),
            }],
        }],
    }])
}

/// `/hide` の下は替えられない、ほかは `v` を付ける
fn map(p: &Path) -> Option<String> {
    let s = p.to_string_lossy();
    (!s.starts_with("/hide")).then(|| format!("/v{s}"))
}

/// RMT-01: 曲一覧の形（往復した曲一覧と、落とした数）
#[rstest]
#[case::mapped(sample(), 2)]
#[case::empty(Library::default(), 0)]
fn rmt_01_library(#[case] lib: Library, #[case] songs: usize) {
    let d = library_to_dto(&lib, &map);
    assert_eq!(d.songs.len(), songs);
    if songs > 0 {
        assert_eq!(d.roots, vec!["/v/r"]);
        assert_eq!(d.folders, vec!["/v/r/a"]);
        assert_eq!(d.songs[0].md5, "01".repeat(16));
        assert_eq!(d.songs[0].sha256, "09".repeat(32));
        assert_eq!(d.songs[1].sha256, "");
        assert_eq!(d.songs[0].keys, 14);
    }
    // 文字にして読み戻す
    let text = serde_json::to_string(&d).unwrap();
    let back = library_from_dto(serde_json::from_str(&text).unwrap());
    let want = library_from_dto(d.clone());
    assert_eq!(back, want);
    assert_eq!(back.songs().len(), songs);
    assert_eq!(back.custom().len(), lib.custom().len());
    assert_eq!(back.tables(), lib.tables());
    for (b, s) in back.songs().iter().zip(lib.songs()) {
        assert_eq!(
            (
                b.path.to_string_lossy().into_owned(),
                &b.hash,
                &b.title,
                b.key_mode
            ),
            (
                format!("/v{}", s.path.display()),
                &s.hash,
                &s.title,
                s.key_mode
            )
        );
    }
    // 読めないハッシュ・鍵盤数の譜面は落とす
    let mut bad = d;
    bad.songs.push(SongDto {
        md5: "zz".into(),
        keys: 7,
        ..Default::default()
    });
    bad.songs.push(SongDto {
        md5: "00".repeat(16),
        keys: 6,
        ..Default::default()
    });
    assert_eq!(library_from_dto(bad).songs().len(), songs);
}

fn record(
    lamp: ClearLamp,
    ruleset: RulesetId,
    origin: ScoreOrigin,
    bp: Option<u32>,
) -> ScoreRecord {
    ScoreRecord {
        chart: ChartHash {
            md5: [7; 16],
            sha256: Some([8; 32]),
        },
        ruleset,
        lamp,
        ex_score: 100,
        pgreat: 40,
        great: 20,
        good: 3,
        bad: 2,
        poor: 1,
        max_combo: 50,
        min_bp: bp,
        play_count: 5,
        clear_count: 4,
        origin,
    }
}

/// RMT-02: 記録の形（ランプの番号）
#[rstest]
#[case::no_play(
    record(ClearLamp::NoPlay, RulesetId::Lr2, ScoreOrigin::Native, None),
    0,
    "lr2"
)]
#[case::hard(
    record(ClearLamp::Hard, RulesetId::Lr2, ScoreOrigin::Native, Some(3)),
    6,
    "lr2"
)]
#[case::max_imported(record(ClearLamp::Max, RulesetId::Beatoraja, ScoreOrigin::ImportedFrom("lr2".into()), Some(0)), 10, "beatoraja")]
fn rmt_02_score(#[case] r: ScoreRecord, #[case] lamp: u8, #[case] ruleset: &str) {
    let d = score_to_dto(&r);
    assert_eq!((d.lamp, d.ruleset.as_str()), (lamp, ruleset));
    let text = serde_json::to_string(&d).unwrap();
    let back = score_from_dto(&serde_json::from_str(&text).unwrap());
    assert_eq!(back, Some(r));
    assert_eq!(
        score_from_dto(&ScoreDto {
            lamp: 11,
            ..d.clone()
        }),
        None
    );
    assert_eq!(
        score_from_dto(&ScoreDto {
            ruleset: "x".into(),
            ..d
        }),
        None
    );
}

/// RMT-03: クエリの値の符号化
#[rstest]
#[case::plain("/songs/000/a-b_c.~", "/songs/000/a-b_c.~")]
#[case::space_and_amp("a b&c=d", "a%20b%26c%3Dd")]
#[case::japanese("曲", "%E6%9B%B2")]
#[case::percent("%", "%25")]
fn rmt_03_encode(#[case] s: &str, #[case] want: &str) {
    assert_eq!(encode(s), want);
}

/// RMT-04: ファイル
#[rstest]
#[case::list_and_read("/s/a.bms", true, Ok(b"abc".to_vec()))]
#[case::not_listed("/s/b.bms", false, Err(()))]
#[case::folder_is_not_file("/s/sub", false, Err(()))]
fn rmt_04_fs(#[case] path: &str, #[case] is_file: bool, #[case] read: Result<Vec<u8>, ()>) {
    let (f, r) = remote();
    f.answer(
        "GET",
        "/api/list?path=/s",
        200,
        r#"[{"name":"a.bms","path":"/s/a.bms","is_file":true},{"name":"sub","path":"/s/sub","is_file":false}]"#,
    );
    f.answer("GET", "/api/file?path=/s/a.bms", 200, "abc");
    let fs = RemoteFs(r.clone());
    assert_eq!(fs.is_file(Path::new(path)), is_file);
    assert_eq!(fs.read(Path::new(path)).map_err(|_| ()), read);
    // 2 度目の中身は覚えたもの
    assert_eq!(fs.list(Path::new("/s")).len(), 2);
    let lists = asked(&f).iter().filter(|a| a.contains("/api/list")).count();
    assert_eq!(lists, 1);
    // 読めないフォルダは空で、覚えない
    assert!(fs.list(Path::new("/none")).is_empty());
    assert!(fs.list(Path::new("/none")).is_empty());
    let none = asked(&f).iter().filter(|a| a.contains("/none")).count();
    assert_eq!(none, 2);
    // 忘れたらまた聞く
    r.forget();
    fs.list(Path::new("/s"));
    let lists = asked(&f)
        .iter()
        .filter(|a| a.contains("/api/list?path=/s"))
        .count();
    assert_eq!(lists, 2);
}

/// RMT-05: 記録（サーバーにある記録の EX → 探した記録の EX。保存の後も）
#[rstest]
#[case::found(Some(5), Some(5))]
#[case::none(None, None)]
fn rmt_05_scores(#[case] on_server: Option<u32>, #[case] want: Option<u32>) {
    let (f, r) = remote();
    let mut rec = record(
        ClearLamp::Easy,
        RulesetId::Lr2,
        ScoreOrigin::Native,
        Some(1),
    );
    let other = ScoreRecord {
        chart: ChartHash {
            md5: [9; 16],
            sha256: None,
        },
        ..rec.clone()
    };
    let mut all = vec![score_to_dto(&other)];
    if let Some(ex) = on_server {
        rec.ex_score = ex;
        all.push(score_to_dto(&rec));
    }
    let mut s = RemoteScores::new(r);
    // もらえなければ誤りで、覚えない
    assert!(s.find(&rec.chart, RulesetId::Lr2).is_err());
    f.answer(
        "GET",
        "/api/scores",
        200,
        &serde_json::to_string(&all).unwrap(),
    );
    f.answer("POST", "/api/score", 204, "");
    let got = s.find(&rec.chart, RulesetId::Lr2).unwrap();
    assert_eq!(got.map(|r| r.ex_score), want);
    assert_eq!(s.find(&rec.chart, RulesetId::Beatoraja).unwrap(), None);
    assert_eq!(
        s.find(&other.chart, RulesetId::Lr2).unwrap(),
        Some(other.clone())
    );
    // 2 度目からは覚えたもの
    let gets = asked(&f).iter().filter(|a| *a == "GET /api/scores").count();
    assert_eq!(gets, 2);
    // 保存したら覚えたものも替わる
    rec.ex_score = 77;
    s.save(rec.clone()).unwrap();
    let sent = f.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!((sent.0.as_str(), sent.1.as_str()), ("POST", "/api/score"));
    let dto: ScoreDto = serde_json::from_slice(&sent.2.unwrap()).unwrap();
    assert_eq!(score_from_dto(&dto), Some(rec.clone()));
    assert_eq!(s.find(&rec.chart, RulesetId::Lr2).unwrap(), Some(rec));
    // 保存できなければ誤り
    f.answer("POST", "/api/score", 500, "");
    assert!(s.save(other).is_err());
}

/// RMT-06: SQL（状態 → パスか誤り）
#[rstest]
#[case::ok(200, r#"{"paths":["/s/a.bms","/s/b.bms"]}"#, Ok(vec!["/s/a.bms", "/s/b.bms"]))]
#[case::bad_sql(400, "near x: syntax error", Err("near x: syntax error"))]
fn rmt_06_query(#[case] status: u16, #[case] body: &str, #[case] want: Result<Vec<&str>, &str>) {
    let (f, r) = remote();
    f.answer("POST", "/api/query", status, body);
    let got = RemoteSongQuery(r).song_paths("SELECT 1");
    match (got, want) {
        (Ok(p), Ok(w)) => assert_eq!(p, w.iter().map(PathBuf::from).collect::<Vec<_>>()),
        (Err(PortError::Parse(e)), Err(w)) => assert_eq!(e, w),
        (g, w) => panic!("{g:?} / {w:?}"),
    }
    let sent = f.asked.lock().unwrap()[0].2.clone().unwrap();
    assert_eq!(
        serde_json::from_slice::<QueryDto>(&sent).unwrap().sql,
        "SELECT 1"
    );
}

thread_local! {
    /// テストごとの時刻（テストはスレッドごとに並んで走る）
    static NOW: Cell<u64> = const { Cell::new(0) };
}
fn now() -> f64 {
    NOW.with(|n| n.get()) as f64
}

/// RMT-07: 曲一覧の届け先（(時刻, サーバーの版) ごとに届くか）
#[rstest]
#[case::waits(vec![(1999, 2)], vec![false])]
#[case::newer(vec![(2000, 2)], vec![true])]
#[case::same(vec![(2000, 1)], vec![false])]
#[case::then_waits_again(vec![(2000, 2), (3000, 3), (4000, 3), (4001, 3)], vec![true, false, true, false])]
fn rmt_07_library(#[case] steps: Vec<(u64, u64)>, #[case] want: Vec<bool>) {
    NOW.with(|n| n.set(0));
    let (f, r) = remote();
    f.answer("GET", "/api/list?path=/s", 200, "[]");
    let lib = RemoteLibrary::new(r.clone(), 1, now);
    let mut got = vec![];
    for (t, v) in steps {
        NOW.with(|n| n.set(t));
        f.answer("GET", "/api/version", 200, &format!(r#"{{"version":{v}}}"#));
        let body = serde_json::to_string(&LibraryResponse {
            version: v,
            library: library_to_dto(&sample(), &map),
        })
        .unwrap();
        f.answer("GET", "/api/library", 200, &body);
        RemoteFs(r.clone()).list(Path::new("/s"));
        let l = lib.poll();
        got.push(l.is_some());
        if let Some(l) = l {
            assert_eq!(l.songs().len(), 2);
            // 覚えた中身を忘れたので、もう一度聞く
            RemoteFs(r.clone()).list(Path::new("/s"));
            assert_eq!(asked(&f).last().unwrap(), "GET /api/list?path=/s");
        }
    }
    assert_eq!(got, want);
}

/// RMT-08: 応答の読み方
#[rstest]
#[case::ok(200, Some("ok"))]
#[case::no_content(204, Some(""))]
#[case::token(401, None)]
#[case::offline(0, None)]
#[case::missing(404, None)]
#[case::bad(400, None)]
#[case::server(500, None)]
fn rmt_08_check(#[case] status: u16, #[case] want: Option<&str>) {
    let body = if status == 204 { "" } else { "ok" };
    let got = check(status, body.as_bytes().to_vec());
    match (&got, want) {
        (Ok(b), Some(w)) => assert_eq!(b, w.as_bytes()),
        (Err(PortError::Io(m)), None) => {
            let key = match status {
                401 => "合言葉",
                0 => "つながらない",
                404 => "無い",
                _ => "誤り",
            };
            assert!(m.contains(key), "{m}");
        }
        (Err(PortError::Parse(m)), None) => assert_eq!((status, m.as_str()), (400, "ok")),
        (g, w) => panic!("{g:?} / {w:?}"),
    }
}

/// RMT-09: 難易度表の取り直し
#[rstest]
#[case::updated(200, r#"{"version":7}"#, Some(7))]
#[case::refused(401, "", None)]
fn rmt_09_update_tables(#[case] status: u16, #[case] body: &str, #[case] want: Option<u64>) {
    let (f, r) = remote();
    f.answer("POST", "/api/tables/update", status, body);
    assert_eq!(r.update_tables().ok(), want);
    assert_eq!(asked(&f), vec!["POST /api/tables/update"]);
}
