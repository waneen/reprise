//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

fn names(v: &[DirEntry]) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = v.iter().map(|e| (e.name.clone(), e.is_file)).collect();
    v.sort();
    v
}

/// FS-STD-01: ディスクの一覧・読み出し
#[rstest]
#[case::dir("", vec![("a.txt".into(), true), ("sub".into(), false)])]
#[case::not_dir("a.txt", vec![])]
#[case::missing("none", vec![])]
fn fs_std_01_disk(#[case] rel: &str, #[case] want: Vec<(String, bool)>) {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("a.txt"), b"x").unwrap();
    std::fs::create_dir(t.path().join("sub")).unwrap();
    let dir = t.path().join(rel);
    assert_eq!(names(&StdFs.list(&dir)), want);
    assert_eq!(StdFs.read(&t.path().join("a.txt")).unwrap(), b"x");
    assert!(StdFs.is_file(&t.path().join("a.txt")));
    assert!(matches!(
        StdFs.read(&t.path().join("none")),
        Err(PortError::Io(_))
    ));
}

/// FS-STD-02: 更新日時
#[rstest]
#[case::file("a.txt", true)]
#[case::dir("sub", true)]
#[case::missing("none", false)]
fn fs_std_02_modified(#[case] rel: &str, #[case] exists: bool) {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("a.txt"), b"x").unwrap();
    std::fs::create_dir(t.path().join("sub")).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let got = StdFs.modified(&t.path().join(rel));
    assert_eq!(got.is_some(), exists);
    if let Some(m) = got {
        assert!((now - m).abs() < 60, "{m} {now}");
    }
}

/// FS-MEM-01: パスの正規化と読み出し
#[rstest]
#[case::same("/song/a.bms", true)]
#[case::dot("/song/./a.bms", true)]
#[case::parent("/song/sub/../a.bms", true)]
#[case::backslash("\\song\\a.bms", true)]
#[case::relative("song/a.bms", true)]
#[case::other("/song/b.bms", false)]
#[case::dir("/song", false)]
fn fs_mem_01_paths(#[case] path: &str, #[case] found: bool) {
    let mut fs = MemFs::new();
    fs.insert("/song/a.bms", b"data".to_vec());
    assert_eq!(fs.is_file(Path::new(path)), found);
    match fs.read(Path::new(path)) {
        Ok(b) => assert!(found && b == b"data"),
        Err(e) => assert!(!found && matches!(e, PortError::Io(_))),
    }
}

/// FS-MEM-02: 直下のファイルと、置いたファイルから分かるフォルダ
#[rstest]
#[case::root("/", vec![("a".into(), false), ("top.txt".into(), true)])]
#[case::sub("/a", vec![("b".into(), false), ("x.wav".into(), true), ("y.wav".into(), true)])]
#[case::deep("/a/b", vec![("z.ogg".into(), true)])]
#[case::file("/a/x.wav", vec![])]
#[case::missing("/none", vec![])]
fn fs_mem_02_list(#[case] dir: &str, #[case] want: Vec<(String, bool)>) {
    let mut fs = MemFs::new();
    for p in ["/top.txt", "/a/y.wav", "/a/x.wav", "/a/b/z.ogg"] {
        fs.insert(p, b"x".to_vec());
    }
    let got = fs.list(Path::new(dir));
    assert_eq!(names(&got), want);
    assert!(got.iter().all(|e| e.path == Path::new(dir).join(&e.name)));
}

/// FS-MEM-03: 更新日時
#[rstest]
#[case::file("/a/x.wav", Some(0))]
#[case::implied_dir("/a/b", Some(0))]
#[case::prefix_is_not_dir("/a/b/z", None)]
#[case::missing("/c", None)]
fn fs_mem_03_modified(#[case] path: &str, #[case] want: Option<i64>) {
    let mut fs = MemFs::new();
    for p in ["/a/x.wav", "/a/b/z.ogg"] {
        fs.insert(p, b"x".to_vec());
    }
    assert_eq!(fs.modified(Path::new(path)), want);
}
