//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
//! サーバーをこの機械の空いたポートで本当に動かし、ブラウザ版と同じ `remote` のポートの実装でつないで確かめる。
use super::*;
use application::ports::DirEntry;
use domain::chart::ChartHash;
use domain::play::RulesetId;
use domain::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use remote::{Remote, RemoteFs, RemoteScores, RemoteSongQuery, Transport};
use rstest::rstest;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tempfile::TempDir;

/// ureq で送る（ブラウザ版のページの JavaScript の代わり）
struct Http {
    base: String,
    token: Option<String>,
    agent: ureq::Agent,
}

impl Http {
    fn new(addr: SocketAddr, token: Option<&str>) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        Self {
            base: format!("http://{addr}"),
            token: token.map(String::from),
            agent,
        }
    }
}

impl Transport for Http {
    fn request(&self, method: &str, target: &str, body: Option<&[u8]>) -> (u16, Vec<u8>) {
        let url = format!("{}{}", self.base, target);
        let auth = self.token.as_ref().map(|t| format!("Bearer {t}"));
        let res = if method == "GET" {
            let mut r = self.agent.get(&url);
            if let Some(a) = &auth {
                r = r.header("Authorization", a);
            }
            r.call()
        } else {
            let mut r = self.agent.post(&url);
            if let Some(a) = &auth {
                r = r.header("Authorization", a);
            }
            r.send(body.unwrap_or_default())
        };
        match res {
            Ok(mut resp) => (
                resp.status().as_u16(),
                resp.body_mut()
                    .with_config()
                    .limit(1 << 30)
                    .read_to_vec()
                    .unwrap_or_default(),
            ),
            Err(_) => (0, vec![]),
        }
    }
}

fn bms(title: &str, level: i32) -> String {
    format!("#TITLE {title}\n#ARTIST A\n#BPM 120\n#PLAYLEVEL {level}\n#00111:01\n")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// 譜面のフォルダ `bms`（`a/1.bms`・`a/2.bms`）、LR2 本体 `lr2`、ページ `web` を置いたフォルダ
fn place() -> TempDir {
    let t = tempfile::tempdir().unwrap();
    write(&t.path().join("bms/a/1.bms"), &bms("Alpha", 3));
    write(&t.path().join("bms/a/2.bms"), &bms("Beta", 5));
    write(&t.path().join("bms/a/s.wav"), "RIFF");
    write(&t.path().join("lr2/LR2files/Config/config.xml"), "<x/>");
    write(&t.path().join("web/index.html"), "<html>reprise</html>");
    t
}

fn config(t: &TempDir) -> Config {
    Config {
        data: t.path().join("data"),
        songs: vec![t.path().join("bms")],
        skin_root: Some(t.path().join("lr2")),
        tables: vec![],
        bind: "127.0.0.1:0".into(),
        token: Some("tok".into()),
        web: Some(t.path().join("web")),
    }
}

/// 表を数える偽物（`fetch` するたびにレベルが 1 つ増える）
#[derive(Default)]
struct FakeTables {
    saved: Mutex<HashMap<String, DifficultyTable>>,
    fetched: AtomicUsize,
}

impl TableSource for FakeTables {
    fn cached(&self, url: &str) -> Option<DifficultyTable> {
        lock(&self.saved).get(url).cloned()
    }
    fn fetch(&self, url: &str) -> Result<DifficultyTable, PortError> {
        let n = self.fetched.fetch_add(1, Ordering::SeqCst) + 1;
        let t = DifficultyTable {
            url: url.into(),
            name: "T".into(),
            levels: (0..n)
                .map(|i| domain::select::TableLevel {
                    name: format!("★{i}"),
                    charts: vec![],
                })
                .collect(),
        };
        lock(&self.saved).insert(url.into(), t.clone());
        Ok(t)
    }
}

fn serve(c: Config) -> (Running, Arc<Remote>) {
    let r = start_with(c, Arc::new(FakeTables::default())).unwrap();
    let remote = Remote::new(Http::new(r.addr, Some("tok")));
    (r, remote)
}

/// 曲一覧が `ok` になるまで待つ（10 秒まで）
fn wait_library(remote: &Remote, ok: impl Fn(&Library) -> bool) -> (u64, Library) {
    let t = Instant::now();
    loop {
        let (v, l) = remote.library().unwrap();
        if ok(&l) || t.elapsed() > Duration::from_secs(10) {
            return (v, l);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn titles(l: &Library) -> Vec<String> {
    let mut v: Vec<String> = l.songs().iter().map(|s| s.title.clone()).collect();
    v.sort();
    v
}

/// SRV-01: 仮想のパス（登録したフォルダ, 実のパス → 仮想のパス）
#[rstest]
#[case::root(&["/x/bms"], "/x/bms", Some("/songs/000/bms"))]
#[case::inside(&["/x/bms"], "/x/bms/a/1.bms", Some("/songs/000/bms/a/1.bms"))]
#[case::sorted_upper(&["/x/b", "/x/A"], "/x/b/1.bms", Some("/songs/001/b/1.bms"))]
#[case::deepest(&["/x", "/x/bms"], "/x/bms/1.bms", Some("/songs/001/bms/1.bms"))]
#[case::lr2folder(&["/x/new.lr2folder"], "/x/new.lr2folder", Some("/songs/000/new.lr2folder"))]
#[case::skin(&["/x/bms"], "/lr2/LR2files/Theme/a.png", Some("/skin/LR2files/Theme/a.png"))]
#[case::outside(&["/x/bms"], "/etc/passwd", None)]
fn srv_01_virtual(#[case] songs: &[&str], #[case] real: &str, #[case] want: Option<&str>) {
    let songs: Vec<PathBuf> = songs.iter().map(PathBuf::from).collect();
    let roots = Roots::new(&songs, Some(PathBuf::from("/lr2")));
    let real = PathBuf::from(real);
    let v = roots.to_virtual(&real);
    assert_eq!(v.as_deref(), want);
    if let Some(v) = v {
        assert_eq!(roots.to_real(&v), Some(real));
    }
}

/// SRV-02: 受けない仮想のパス
#[rstest]
#[case::parent("/songs/000/bms/../../etc/passwd")]
#[case::dot("/songs/000/bms/./a")]
#[case::empty("/songs/000/bms//a")]
#[case::unknown_root("/etc/passwd")]
#[case::unknown_index("/songs/009/bms/a")]
#[case::wrong_name("/songs/000/other/a")]
#[case::relative("songs/000/bms")]
#[case::skin_parent("/skin/../x")]
fn srv_02_reject(#[case] v: &str) {
    let roots = Roots::new(&[PathBuf::from("/x/bms")], Some(PathBuf::from("/lr2")));
    assert_eq!(roots.to_real(v), None);
}

/// SRV-03: 合言葉（送る合言葉, 行き先 → 状態）
#[rstest]
#[case::none(None, "/api/version", 401)]
#[case::wrong(Some("nope"), "/api/version", 401)]
#[case::prefix(Some("to"), "/api/library", 401)]
#[case::right(Some("tok"), "/api/version", 200)]
#[case::page_needs_none(None, "/", 200)]
fn srv_03_auth(#[case] token: Option<&str>, #[case] target: &str, #[case] want: u16) {
    let t = place();
    let (r, _) = serve(config(&t));
    let (status, _) = Http::new(r.addr, token).request("GET", target, None);
    assert_eq!(status, want);
}

/// SRV-04: 曲一覧と版（足したファイルで版が上がる）
#[rstest]
#[case::add_chart("bms/a/3.bms", vec!["Alpha", "Beta", "Gamma"])]
fn srv_04_library(#[case] add: &str, #[case] then: Vec<&str>) {
    let t = place();
    let (_r, remote) = serve(config(&t));
    let (v1, lib) = wait_library(&remote, |l| l.songs().len() == 2);
    assert_eq!(titles(&lib), vec!["Alpha", "Beta"]);
    assert!(v1 >= 1);
    assert_eq!(remote.version().unwrap(), v1);
    let mut paths: Vec<String> = lib
        .songs()
        .iter()
        .map(|s| s.path.to_string_lossy().into_owned())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec!["/songs/000/bms/a/1.bms", "/songs/000/bms/a/2.bms"]
    );
    assert_eq!(lib.roots(), &[PathBuf::from("/songs/000/bms")]);
    // 見張りで替わる
    std::thread::sleep(Duration::from_millis(500));
    write(&t.path().join(add), &bms("Gamma", 1));
    let (v2, lib) = wait_library(&remote, |l| l.songs().len() == 3);
    assert_eq!(titles(&lib), then);
    assert!(v2 > v1);
}

/// SRV-05: フォルダの中身とファイル（仮想のパス → 中身の名前・読めた中身）
#[rstest]
#[case::top("/", Some(vec!["songs", "skin"]), None)]
#[case::songs("/songs", Some(vec!["000"]), None)]
#[case::index("/songs/000", Some(vec!["bms"]), None)]
#[case::folder("/songs/000/bms/a", Some(vec!["1.bms", "2.bms", "s.wav"]), None)]
#[case::file("/songs/000/bms/a/s.wav", None, Some(Some("RIFF")))]
#[case::skin_file("/skin/LR2files/Config/config.xml", None, Some(Some("<x/>")))]
#[case::missing("/songs/000/bms/a/none.wav", None, Some(None))]
#[case::outside("/songs/000/bms/../../web/index.html", None, Some(None))]
fn srv_05_files(
    #[case] path: &str,
    #[case] names: Option<Vec<&str>>,
    #[case] read: Option<Option<&str>>,
) {
    let t = place();
    let (_r, remote) = serve(config(&t));
    let fs = RemoteFs(remote);
    if let Some(want) = names {
        let mut got: Vec<DirEntry> = fs.list(Path::new(path));
        got.sort_by(|a, b| a.name.cmp(&b.name));
        let mut want = want.clone();
        want.sort();
        assert_eq!(
            got.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            want
        );
        for e in &got {
            assert!(e.path.starts_with(path), "{:?}", e.path);
        }
    }
    if let Some(want) = read {
        let got = fs.read(Path::new(path)).ok();
        assert_eq!(got.as_deref(), want.map(str::as_bytes));
        assert_eq!(fs.is_file(Path::new(path)), want.is_some());
    }
}

fn record(ex: u32) -> ScoreRecord {
    ScoreRecord {
        chart: ChartHash {
            md5: [3; 16],
            sha256: Some([4; 32]),
        },
        ruleset: RulesetId::Lr2,
        lamp: ClearLamp::Hard,
        ex_score: ex,
        pgreat: ex / 2,
        great: 0,
        good: 0,
        bad: 0,
        poor: 1,
        max_combo: 9,
        min_bp: Some(1),
        play_count: 2,
        clear_count: 1,
        origin: ScoreOrigin::Native,
    }
}

/// SRV-06: 記録（保存した EX スコアの並び → 最後に読める記録）
#[rstest]
#[case::none(vec![], None)]
#[case::saved(vec![10], Some(10))]
#[case::replaced(vec![10, 4], Some(4))]
fn srv_06_score(#[case] saves: Vec<u32>, #[case] want: Option<u32>) {
    let t = place();
    let (r, remote) = serve(config(&t));
    let mut s = RemoteScores::new(remote.clone());
    for ex in saves {
        s.save(record(ex)).unwrap();
    }
    // 新しくつないだものはサーバーの記録をまとめてもらう
    let s = RemoteScores::new(remote);
    let got = s.find(&record(0).chart, RulesetId::Lr2).unwrap();
    assert_eq!(got.as_ref().map(|r| r.ex_score), want);
    if let Some(g) = got {
        assert_eq!(g, record(g.ex_score));
    }
    // 別のルールは無い
    assert_eq!(
        s.find(&record(0).chart, RulesetId::Beatoraja).unwrap(),
        None
    );
    // 1 つだけ聞く
    let http = Http::new(r.addr, Some("tok"));
    let target = format!("/api/score?md5={}&ruleset=lr2", "03".repeat(16));
    let (status, body) = http.request("GET", &target, None);
    let one: Option<ScoreDto> = serde_json::from_slice(&body).unwrap();
    assert_eq!((status, one.map(|d| d.ex_score)), (200, want));
    // 読めないものは 400
    assert_eq!(
        http.request("GET", "/api/score?md5=zz&ruleset=lr2", None).0,
        400
    );
    assert_eq!(http.request("POST", "/api/score", Some(b"{}")).0, 400);
}

/// SRV-07: SQL（SQL → 仮想のパスか誤り）
#[rstest]
#[case::level("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE level = 5", Ok(vec!["/songs/000/bms/a/2.bms"]))]
#[case::lamp("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE clear IS NULL ORDER BY title", Ok(vec!["/songs/000/bms/a/1.bms", "/songs/000/bms/a/2.bms"]))]
#[case::delete("DELETE FROM song", Err(()))]
#[case::broken("SELECT * FROM song WHERE (", Err(()))]
fn srv_07_query(#[case] sql: &str, #[case] want: Result<Vec<&str>, ()>) {
    let t = place();
    let (_r, remote) = serve(config(&t));
    wait_library(&remote, |l| l.songs().len() == 2);
    let got = RemoteSongQuery(remote.clone())
        .song_paths(sql)
        .map_err(|_| ());
    let want = want.map(|v| v.iter().map(PathBuf::from).collect::<Vec<_>>());
    assert_eq!(got, want);
    // 行は消えていない
    let (_, lib) = remote.library().unwrap();
    assert_eq!(lib.songs().len(), 2);
}

/// SRV-08: 合言葉の決め方（引数, 置いてあるファイルの中身 → 合言葉。None は作る）
#[rstest]
#[case::given(Some("abc"), Some("zzz"), Some("abc"))]
#[case::file(None, Some(" fromfile \n"), Some("fromfile"))]
#[case::created(None, None, None)]
#[case::empty_file(None, Some("  "), None)]
fn srv_08_token(
    #[case] given: Option<&str>,
    #[case] file: Option<&str>,
    #[case] want: Option<&str>,
) {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("data");
    if let Some(f) = file {
        write(&data.join("server-token"), f);
    }
    let got = token(&data, given.map(String::from)).unwrap();
    match want {
        Some(w) => assert_eq!(got, w),
        None => {
            assert_eq!(got.len(), 32);
            assert!(got.bytes().all(|b| b.is_ascii_hexdigit()));
            // 作ったものは次も同じ
            assert_eq!(token(&data, None).unwrap(), got);
        }
    }
}

/// SRV-09: ページ（行き先 → 状態と中身）
#[rstest]
#[case::index("/", 200, Some("<html>reprise</html>"))]
#[case::file("/index.html", 200, Some("<html>reprise</html>"))]
#[case::missing("/none.js", 404, None)]
#[case::api_is_not_page("/api/none", 404, None)]
fn srv_09_web(#[case] target: &str, #[case] status: u16, #[case] body: Option<&str>) {
    let t = place();
    let (r, _) = serve(config(&t));
    let (s, b) = Http::new(r.addr, Some("tok")).request("GET", target, None);
    assert_eq!(s, status);
    if let Some(body) = body {
        assert_eq!(String::from_utf8_lossy(&b), body);
    }
    // ページを置かなければ何も配らない
    let (r, _) = serve(Config {
        web: None,
        ..config(&t)
    });
    assert_eq!(Http::new(r.addr, None).request("GET", "/", None).0, 404);
}

/// SRV-10: 難易度表の取り直し（取り直した回数 → レベルの数）
#[rstest]
#[case::once(1, 2)]
#[case::twice(2, 3)]
fn srv_10_tables(#[case] updates: usize, #[case] levels: usize) {
    let t = place();
    let src = Arc::new(FakeTables::default());
    let r = start_with(
        Config {
            tables: vec!["http://t.invalid/".into()],
            ..config(&t)
        },
        src.clone(),
    )
    .unwrap();
    let remote = Remote::new(Http::new(r.addr, Some("tok")));
    // 起動時は手元に無い表だけ取ってくる
    let (_, lib) = wait_library(&remote, |l| !l.tables().is_empty());
    assert_eq!(lib.tables()[0].levels.len(), 1);
    let http = Http::new(r.addr, Some("tok"));
    for _ in 0..updates {
        let before = remote.version().unwrap();
        let (s, b) = http.request("POST", "/api/tables/update", None);
        assert_eq!(s, 200);
        let v: VersionDto = serde_json::from_slice(&b).unwrap();
        assert!(v.version > before);
    }
    let (_, lib) = remote.library().unwrap();
    assert_eq!(lib.tables()[0].levels.len(), levels);
    assert_eq!(src.fetched.load(Ordering::SeqCst), updates + 1);
}

/// SRV-11: 起動引数（引数 → (songs, skin_root, tables, bind, token)）
#[rstest]
#[case::from_config(&[], (vec!["/c/bms"], Some("/c/lr2"), vec!["http://c/"], BIND, None))]
#[case::override_all(&["--songs", "/a", "--songs", "/b", "--skin-root", "/l", "--table", "http://x/", "--bind", "0.0.0.0:1", "--token", "t"], (vec!["/a", "/b"], Some("/l"), vec!["http://x/"], "0.0.0.0:1", Some("t")))]
fn srv_11_args(
    #[case] args: &[&str],
    #[case] want: (Vec<&str>, Option<&str>, Vec<&str>, &str, Option<&str>),
) {
    let t = tempfile::tempdir().unwrap();
    let data = t.path().join("d");
    settings_json::JsonSettingsFile {
        path: data.join("config.json"),
    }
    .save(&application::settings::Settings {
        songs: vec!["/c/bms".into()],
        skin_root: Some("/c/lr2".into()),
        tables: vec!["http://c/".into()],
        ..Default::default()
    })
    .unwrap();
    let mut all = vec!["--data".to_string(), data.to_string_lossy().into_owned()];
    all.extend(args.iter().map(|a| a.to_string()));
    let c = parse_args(&all).unwrap();
    let got = (
        c.songs.clone(),
        c.skin_root.clone(),
        c.tables.clone(),
        c.bind.clone(),
        c.token.clone(),
    );
    let (s, k, tb, b, tk) = want;
    assert_eq!(
        got,
        (
            s.iter().map(PathBuf::from).collect(),
            k.map(PathBuf::from),
            tb.iter().map(|x| x.to_string()).collect(),
            b.to_string(),
            tk.map(String::from)
        )
    );
    assert_eq!(c.data, data);
    // 知らない引数・値の無い引数は誤り
    assert!(parse_args(&["--nope".to_string()]).is_err());
    assert!(parse_args(&["--songs".to_string()]).is_err());
}
