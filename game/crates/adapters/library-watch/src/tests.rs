//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use application::ports::PortError;
use domain::chart::*;
use domain::random::LegacyMt;
use rstest::rstest;
use std::path::Path;

/// 中身を題名にする .bms
struct TextLoader;
impl ChartLoader for TextLoader {
    fn can_load(&self, p: &Path) -> bool {
        p.extension().is_some_and(|e| e == "bms")
    }
    fn load(&self, _p: &Path, b: &[u8], _rng: &mut LegacyMt) -> Result<Chart, PortError> {
        Ok(Chart {
            hash: ChartHash {
                md5: [b.first().copied().unwrap_or(0); 16],
                sha256: None,
            },
            meta: ChartMeta {
                title: String::from_utf8_lossy(b).into_owned(),
                ..Default::default()
            },
            key_mode: KeyMode::K7,
            notes: vec![],
            bgm: vec![],
            bga: vec![],
            timing: vec![],
            sounds: vec![],
            images: vec![],
        })
    }
}

fn loaders() -> Vec<Box<dyn ChartLoader>> {
    vec![Box::new(TextLoader)]
}

fn titles(l: &Library) -> Vec<String> {
    let mut v: Vec<String> = l.songs().iter().map(|s| s.title.clone()).collect();
    v.sort();
    v
}

/// 曲一覧が届くまで待つ（`want` になったら返す）
fn wait_for(lib: &LocalLibrary, want: &[&str]) -> Vec<String> {
    let mut last = vec![];
    for _ in 0..400 {
        if let Some(l) = lib.poll() {
            last = titles(&l);
            if last == want {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    last
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("bms");
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::write(root.join("a/1.bms"), "one").unwrap();
    (t, root)
}

fn start(t: &tempfile::TempDir, root: &Path) -> (Library, LocalLibrary) {
    LocalLibrary::start(
        t.path().join("data/song.db"),
        vec![root.to_path_buf()],
        Arc::new(file_system::StdFs),
        loaders,
    )
    .unwrap()
}

/// LWT-01: DB の曲一覧をすぐ返し、確かめた結果を届ける
#[rstest]
#[case::first_run(false, vec![], vec!["one"])]
#[case::second_run(true, vec!["one"], vec!["one", "two"])]
fn lwt_01_start(#[case] again: bool, #[case] first: Vec<&str>, #[case] then: Vec<&str>) {
    let (t, root) = setup();
    if again {
        // 前に起動したときの DB
        let mut db = song_db::SqliteSongDb::open(&t.path().join("data/song.db")).unwrap();
        let ls = loaders();
        let scan = Scan {
            fs: &file_system::StdFs,
            loaders: &ls,
            now: 1,
        };
        library::update(&scan, &mut db, std::slice::from_ref(&root)).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(root.join("b/2.bms"), "two").unwrap();
    }
    let (now, l) = start(&t, &root);
    assert_eq!(titles(&now), first);
    assert_eq!(wait_for(&l, &then), then);
}

/// LWT-02: 見張り
#[rstest]
#[case::added("add", vec!["one", "three"])]
#[case::removed("remove", vec![])]
fn lwt_02_watch(#[case] op: &str, #[case] want: Vec<&str>) {
    let (t, root) = setup();
    let (_, l) = start(&t, &root);
    assert_eq!(wait_for(&l, &["one"]), vec!["one"]);
    // 見張りが始まるのを少し待つ
    std::thread::sleep(Duration::from_millis(500));
    match op {
        "add" => {
            std::fs::create_dir_all(root.join("c")).unwrap();
            std::fs::write(root.join("c/3.bms"), "three").unwrap();
        }
        _ => std::fs::remove_dir_all(root.join("a")).unwrap(),
    }
    assert_eq!(wait_for(&l, &want), want);
}

/// LWT-03: 知らせのまとめ方（知らせの時刻 ms, 確かめる時刻 ms → まとめたパス）
#[rstest]
#[case::quiet_after_last(&[(0, "a"), (100, "b"), (100, "a")], 399, None)]
#[case::quiet_reached(&[(0, "a"), (100, "b"), (100, "a")], 400, Some(vec!["a", "b"]))]
#[case::max_wait(&[(0, "a"), (250, "b"), (500, "c"), (750, "d"), (1000, "e"), (1250, "f"), (1500, "g"), (1750, "h")], 2000, Some(vec!["a", "b", "c", "d", "e", "f", "g", "h"]))]
#[case::nothing(&[], 5000, None)]
fn lwt_03_debounce(
    #[case] events: &[(u64, &str)],
    #[case] at: u64,
    #[case] want: Option<Vec<&str>>,
) {
    let t0 = Instant::now();
    let ms = |m| t0 + Duration::from_millis(m);
    let mut d = Debounce::default();
    for (t, p) in events {
        d.push(ms(*t), [PathBuf::from(p)]);
    }
    let got = d.take(ms(at));
    assert_eq!(
        got,
        want.map(|v| v.into_iter().map(PathBuf::from).collect::<Vec<_>>())
    );
    if got.is_some() {
        assert_eq!(d.take(ms(at + 10_000)), None);
    }
}

/// LWT-04: 作り直し
#[rstest]
#[case::rebuild(vec!["one"])]
fn lwt_04_rebuild(#[case] want: Vec<&str>) {
    let (t, root) = setup();
    let (_, l) = start(&t, &root);
    assert_eq!(wait_for(&l, &["one"]), vec!["one"]);
    l.rebuild();
    assert_eq!(wait_for(&l, &want), want);
}
