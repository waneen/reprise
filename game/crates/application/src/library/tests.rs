//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use crate::ports::DirEntry;
use domain::chart::*;
use domain::random::LegacyMt;
use rstest::rstest;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// パス → (更新日時, 中身)。フォルダは中身なし
type Files = BTreeMap<PathBuf, (i64, Option<Vec<u8>>)>;
#[derive(Default)]
struct FakeFs(Mutex<Files>);

impl FakeFs {
    fn with(entries: &[(&str, i64, Option<&str>)]) -> Self {
        let fs = FakeFs::default();
        for (p, t, b) in entries {
            fs.set(p, *t, *b);
        }
        fs
    }
    fn set(&self, path: &str, date: i64, body: Option<&str>) {
        self.0
            .lock()
            .unwrap()
            .insert(path.into(), (date, body.map(|b| b.as_bytes().to_vec())));
    }
    fn remove(&self, path: &str) {
        self.0.lock().unwrap().retain(|p, _| !p.starts_with(path));
    }
}

impl FileSystem for FakeFs {
    fn list(&self, dir: &Path) -> Vec<DirEntry> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p.parent() == Some(dir))
            .map(|(p, (_, b))| DirEntry {
                name: p.file_name().unwrap().to_string_lossy().into_owned(),
                path: p.clone(),
                is_file: b.is_some(),
            })
            .collect()
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>, PortError> {
        match self.0.lock().unwrap().get(path) {
            Some((_, Some(b))) => Ok(b.clone()),
            _ => Err(PortError::Io("none".into())),
        }
    }
    fn is_file(&self, path: &Path) -> bool {
        matches!(self.0.lock().unwrap().get(path), Some((_, Some(_))))
    }
    fn modified(&self, path: &Path) -> Option<i64> {
        self.0.lock().unwrap().get(path).map(|(t, _)| *t)
    }
}

/// 中身 "題名;#DIFFICULTY" の .bms（中身が "x" なら読めない）
struct TextLoader;
impl ChartLoader for TextLoader {
    fn can_load(&self, p: &Path) -> bool {
        p.extension().is_some_and(|e| e == "bms")
    }
    fn load(&self, _p: &Path, b: &[u8], _rng: &mut LegacyMt) -> Result<Chart, PortError> {
        let s = String::from_utf8_lossy(b).into_owned();
        if s == "x" {
            return Err(PortError::Parse("x".into()));
        }
        let mut it = s.split(';');
        let title = it.next().unwrap_or_default().to_string();
        let difficulty = it.next().and_then(|d| d.parse().ok());
        Ok(Chart {
            hash: ChartHash {
                md5: [title.len() as u8; 16],
                sha256: None,
            },
            meta: ChartMeta {
                title,
                difficulty,
                initial_bpm: Some(150.5),
                judge_rank: None,
                subartist: "sa".into(),
                banner: "b.png".into(),
                ..Default::default()
            },
            key_mode: KeyMode::K7,
            notes: vec![Note {
                beat: Beat(1.0),
                lane: Lane(1),
                kind: NoteKind::Long {
                    end: Beat(2.0),
                    end_sound: None,
                },
                sound: None,
            }],
            bgm: vec![],
            bga: vec![],
            timing: vec![
                TimingEvent::Bpm {
                    beat: Beat(4.0),
                    bpm: 90.9,
                },
                TimingEvent::Bpm {
                    beat: Beat(5.0),
                    bpm: -3.0,
                },
            ],
            sounds: vec![],
            images: vec![],
        })
    }
}

/// 行をパスで持つ DB
#[derive(Default)]
struct MemDb {
    folders: BTreeMap<PathBuf, (FolderEntry, i64)>,
    songs: BTreeMap<PathBuf, (SongEntry, i64)>,
}

impl SongDb for MemDb {
    fn folders(&self) -> Result<Vec<FolderEntry>, PortError> {
        Ok(self.folders.values().map(|(f, _)| f.clone()).collect())
    }
    fn songs(&self) -> Result<Vec<SongEntry>, PortError> {
        Ok(self.songs.values().map(|(s, _)| s.clone()).collect())
    }
    fn song_date(&self, path: &Path) -> Result<Option<i64>, PortError> {
        Ok(self.songs.get(path).map(|(s, _)| s.date))
    }
    fn insert_folder(&mut self, f: &FolderEntry, now: i64) -> Result<bool, PortError> {
        if self.folders.contains_key(&f.path) {
            return Ok(false);
        }
        self.folders.insert(f.path.clone(), (f.clone(), now));
        Ok(true)
    }
    fn touch_folder(&mut self, path: &Path, date: i64, now: i64) -> Result<(), PortError> {
        if let Some((f, t)) = self.folders.get_mut(path) {
            f.date = date;
            *t = now;
        }
        Ok(())
    }
    fn delete_folder(&mut self, path: &Path) -> Result<(), PortError> {
        self.folders.remove(path);
        Ok(())
    }
    fn put_song(&mut self, s: &SongEntry, now: i64) -> Result<(), PortError> {
        self.songs.insert(s.info.path.clone(), (s.clone(), now));
        Ok(())
    }
    fn delete_song(&mut self, path: &Path) -> Result<(), PortError> {
        self.songs.remove(path);
        Ok(())
    }
    fn set_difficulty(&mut self, path: &Path, d: i32) -> Result<(), PortError> {
        if let Some((s, _)) = self.songs.get_mut(path) {
            s.info.difficulty = d;
        }
        Ok(())
    }
    fn clear(&mut self) -> Result<(), PortError> {
        *self = Self::default();
        Ok(())
    }
}

fn utf8(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn loaders() -> Vec<Box<dyn ChartLoader>> {
    vec![Box::new(TextLoader)]
}

fn roots(r: &[&str]) -> Vec<PathBuf> {
    r.iter().map(PathBuf::from).collect()
}

/// DB の中身（フォルダは "[パス]" + 登録なら "*"、譜面は "パス=題名"）
fn rows(db: &MemDb) -> Vec<String> {
    db.folders
        .values()
        .map(|(f, _)| format!("[{}]{}", f.path.display(), if f.root { "*" } else { "" }))
        .chain(
            db.songs
                .values()
                .map(|(s, _)| format!("{}={}", s.info.path.display(), s.info.title)),
        )
        .collect()
}

fn run(fs: &FakeFs, db: &mut MemDb, r: &[&str], now: i64) -> bool {
    let ls = loaders();
    let scan = Scan {
        fs,
        loaders: &ls,
        now,
        decode: utf8,
    };
    update(&scan, db, &roots(r)).unwrap()
}

fn base_fs() -> FakeFs {
    FakeFs::with(&[
        ("/r", 10, None),
        ("/r/a", 10, None),
        ("/r/a/1.bms", 10, Some("one")),
        ("/r/a/readme.txt", 10, Some("t")),
        ("/r/p", 10, None),
        ("/r/p/s", 10, None),
        ("/r/p/s/2.bms", 10, Some("two")),
        ("/q", 10, None),
        ("/q/3.bms", 10, Some("three")),
    ])
}

/// APP-LIB-01: 登録したフォルダ
#[rstest]
#[case::added_and_searched(&["/r"], &[], vec!["[/r]*", "[/r/a]", "[/r/p]", "[/r/p/s]", "/r/a/1.bms=one", "/r/p/s/2.bms=two"])]
#[case::missing_root_skipped(&["/none"], &[], vec![])]
#[case::unregistered_removed(&["/q"], &["/r"], vec!["[/q]*", "/q/3.bms=three"])]
fn app_lib_01_roots(#[case] now_roots: &[&str], #[case] before: &[&str], #[case] want: Vec<&str>) {
    let fs = base_fs();
    let mut db = MemDb::default();
    run(&fs, &mut db, before, 100);
    run(&fs, &mut db, now_roots, 100);
    assert_eq!(rows(&db), want);
}

/// APP-LIB-02: フォルダの中を探す（追加日時・日時も）
#[rstest]
#[case::unreadable_skipped("/r/a/bad.bms", Some("x"), vec!["/r/a/1.bms=one"])]
#[case::other_ext_skipped("/r/a/a.ogg", Some("o"), vec!["/r/a/1.bms=one"])]
#[case::new_chart_added("/r/a/9.bms", Some("nine"), vec!["/r/a/1.bms=one", "/r/a/9.bms=nine"])]
fn app_lib_02_search(#[case] extra: &str, #[case] body: Option<&str>, #[case] want: Vec<&str>) {
    let fs = FakeFs::with(&[
        ("/r", 10, None),
        ("/r/a", 10, None),
        ("/r/a/1.bms", 7, Some("one")),
    ]);
    fs.set(extra, 8, body);
    let mut db = MemDb::default();
    run(&fs, &mut db, &["/r"], 100);
    let songs: Vec<String> = rows(&db)
        .into_iter()
        .filter(|r| !r.starts_with('['))
        .collect();
    assert_eq!(songs, want);
    let (s, added) = &db.songs[Path::new("/r/a/1.bms")];
    assert_eq!((s.date, *added), (7, 100));
    // DB にあるフォルダには入らない: フォルダの日時が同じなら新しい譜面に気づかない（LR2 と同じ）
    fs.set("/r/a/late.bms", 20, Some("late"));
    run(&fs, &mut db, &["/r"], 200);
    assert!(!db.songs.contains_key(Path::new("/r/a/late.bms")));
}

/// APP-LIB-03: 行を確かめる
#[rstest]
#[case::unchanged(|_fs: &FakeFs| {}, false, vec!["/r/a/1.bms=one", "/r/p/s/2.bms=two"])]
#[case::removed_file(|fs: &FakeFs| fs.remove("/r/a/1.bms"), true, vec!["/r/p/s/2.bms=two"])]
#[case::removed_folder(|fs: &FakeFs| fs.remove("/r/p"), true, vec!["/r/a/1.bms=one"])]
#[case::newer_file_reread(|fs: &FakeFs| fs.set("/r/a/1.bms", 11, Some("ONE")), true, vec!["/r/a/1.bms=ONE", "/r/p/s/2.bms=two"])]
#[case::same_date_kept(|fs: &FakeFs| fs.set("/r/a/1.bms", 10, Some("ONE")), false, vec!["/r/a/1.bms=one", "/r/p/s/2.bms=two"])]
#[case::newer_unreadable_removed(|fs: &FakeFs| fs.set("/r/a/1.bms", 11, Some("x")), true, vec!["/r/p/s/2.bms=two"])]
#[case::newer_folder_searched(|fs: &FakeFs| { fs.set("/r/a", 12, None); fs.set("/r/a/4.bms", 12, Some("four")); }, true, vec!["/r/a/1.bms=one", "/r/a/4.bms=four", "/r/p/s/2.bms=two"])]
fn app_lib_03_check(#[case] change: fn(&FakeFs), #[case] changed: bool, #[case] want: Vec<&str>) {
    let fs = base_fs();
    let mut db = MemDb::default();
    run(&fs, &mut db, &["/r"], 100);
    change(&fs);
    assert_eq!(run(&fs, &mut db, &["/r"], 200), changed);
    let songs: Vec<String> = rows(&db)
        .into_iter()
        .filter(|r| !r.starts_with('['))
        .collect();
    assert_eq!(songs, want);
    if changed && want.iter().any(|w| w.ends_with("=ONE")) {
        // 読み直した譜面は追加日時も今
        assert_eq!(db.songs[Path::new("/r/a/1.bms")].1, 200);
    }
}

/// APP-LIB-04: 譜面の行の中身
#[rstest]
#[case::txt_in_folder("/r/a/1.bms", true)]
#[case::no_txt("/r/p/s/2.bms", false)]
fn app_lib_04_song_entry(#[case] path: &str, #[case] txt: bool) {
    let fs = base_fs();
    let ls = loaders();
    let scan = Scan {
        fs: &fs,
        loaders: &ls,
        now: 0,
        decode: utf8,
    };
    let s = read_song(&scan, Path::new(path), 33).unwrap();
    assert_eq!(
        (
            s.date,
            s.min_bpm,
            s.max_bpm,
            s.judge,
            s.long_note,
            s.bga,
            s.txt,
            s.subartist.as_str(),
            s.banner.as_str(),
            s.info.difficulty,
            s.info.notes
        ),
        (33, 90, 150, 2, true, false, txt, "sa", "b.png", -1, 1)
    );
}

/// APP-LIB-05: 難易度を付けて書く（付けたものは残る）
#[rstest]
#[case::assigned_in_order(&[("/r/a/1.bms", "a"), ("/r/a/2.bms", "bb")], vec![2, 3])]
#[case::defined_kept(&[("/r/a/1.bms", "a;5"), ("/r/a/2.bms", "bb")], vec![5, 5])]
fn app_lib_05_difficulty(#[case] charts: &[(&str, &str)], #[case] want: Vec<i32>) {
    let fs = FakeFs::with(&[("/r", 10, None), ("/r/a", 10, None)]);
    for (p, b) in charts {
        fs.set(p, 10, Some(b));
    }
    let mut db = MemDb::default();
    run(&fs, &mut db, &["/r"], 100);
    let got: Vec<i32> = db.songs.values().map(|(s, _)| s.info.difficulty).collect();
    assert_eq!(got, want);
    // 後から足した譜面だけに付ける（前に付けたものは替えない）
    fs.set("/r/a", 11, None);
    fs.set("/r/a/0.bms", 11, Some("c"));
    run(&fs, &mut db, &["/r"], 200);
    let again: Vec<i32> = db
        .songs
        .values()
        .skip(1)
        .map(|(s, _)| s.info.difficulty)
        .collect();
    assert_eq!(again, want);
}

/// APP-LIB-06: DB からの曲一覧
#[rstest]
#[case::root_list(None, vec!["[q]", "[r]"])]
#[case::open_pack(Some("/r/p"), vec!["/r/p/s/2.bms"])]
fn app_lib_06_load(#[case] open: Option<&str>, #[case] want: Vec<&str>) {
    let fs = base_fs();
    let mut db = MemDb::default();
    run(&fs, &mut db, &["/r", "/q"], 100);
    let lib = load(&db).unwrap();
    let bars = match open {
        None => lib.root_bars(),
        Some(f) => {
            let mut all = domain::select::ListFilter {
                key: 0,
                ..Default::default()
            };
            lib.open(Path::new(f), &mut all, &domain::select::NoLookup)
                .unwrap()
                .bars
        }
    };
    let got: Vec<String> = bars
        .iter()
        .map(|b| match b {
            domain::select::Bar::Folder { title, .. } => format!("[{title}]"),
            domain::select::Bar::Song(i) => lib.songs()[*i].path.display().to_string(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(got, want);
}

/// APP-LIB-07: 見張りの読み直し（日時が同じでも、知らされたフォルダは探し、その下の行は確かめる）
#[rstest]
#[case::new_file(|fs: &FakeFs| fs.set("/r/a/5.bms", 10, Some("five")), &["/r/a/5.bms"], true, vec!["/r/a/1.bms=one", "/r/a/5.bms=five", "/r/p/s/2.bms=two"])]
#[case::new_nested_folder(|fs: &FakeFs| { fs.set("/r/n", 10, None); fs.set("/r/n/m", 10, None); fs.set("/r/n/m/6.bms", 10, Some("six")); }, &["/r/n/m/6.bms"], true, vec!["/r/a/1.bms=one", "/r/n/m/6.bms=six", "/r/p/s/2.bms=two"])]
#[case::removed_folder(|fs: &FakeFs| fs.remove("/r/p/s"), &["/r/p/s"], true, vec!["/r/a/1.bms=one"])]
#[case::edited(|fs: &FakeFs| fs.set("/r/a/1.bms", 11, Some("ONE")), &["/r/a/1.bms"], true, vec!["/r/a/1.bms=ONE", "/r/p/s/2.bms=two"])]
#[case::outside_ignored(|fs: &FakeFs| fs.set("/q/7.bms", 10, Some("seven")), &["/q/7.bms"], false, vec!["/r/a/1.bms=one", "/r/p/s/2.bms=two"])]
fn app_lib_07_apply(
    #[case] change: fn(&FakeFs),
    #[case] paths: &[&str],
    #[case] changed: bool,
    #[case] want: Vec<&str>,
) {
    let fs = base_fs();
    let mut db = MemDb::default();
    run(&fs, &mut db, &["/r"], 100);
    change(&fs);
    let ls = loaders();
    let scan = Scan {
        fs: &fs,
        loaders: &ls,
        now: 200,
        decode: utf8,
    };
    let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    assert_eq!(
        apply(&scan, &mut db, &roots(&["/r"]), &paths).unwrap(),
        changed
    );
    let songs: Vec<String> = rows(&db)
        .into_iter()
        .filter(|r| !r.starts_with('['))
        .collect();
    assert_eq!(songs, want);
}

/// APP-LIB-08: カスタムフォルダ（パス → (題名, command, 種類, 登録した)）
#[rstest]
#[case::found(&["/r"], |_: &FakeFs| {}, vec![("/r/fav.lr2folder", "Fav", "clear = 1", 2, false)])]
#[case::root_registered(&["/r", "/n.lr2folder"], |_: &FakeFs| {}, vec![("/n.lr2folder", "New", "__NEWSONG__", 3, true), ("/r/fav.lr2folder", "Fav", "clear = 1", 2, false)])]
#[case::reread(&["/r", "/n.lr2folder"], |fs: &FakeFs| {
    fs.set("/r/fav.lr2folder", 20, Some("#TITLE Fav2\n#COMMAND clear = 2\n"));
    fs.set("/n.lr2folder", 20, Some("#TITLE New2\n#COMMAND __NEWSONG__\n"));
}, vec![("/n.lr2folder", "New2", "__NEWSONG__", 3, true), ("/r/fav.lr2folder", "Fav2", "clear = 2", 2, false)])]
#[case::same_date_kept(&["/r"], |fs: &FakeFs| fs.set("/r/fav.lr2folder", 10, Some("#TITLE X\n")), vec![("/r/fav.lr2folder", "Fav", "clear = 1", 2, false)])]
#[case::removed(&["/r"], |fs: &FakeFs| fs.remove("/r/fav.lr2folder"), vec![])]
fn app_lib_08_custom(
    #[case] r: &[&str],
    #[case] change: fn(&FakeFs),
    #[case] want: Vec<(&str, &str, &str, i32, bool)>,
) {
    let fs = FakeFs::with(&[
        ("/r", 10, None),
        ("/r/a", 10, None),
        ("/r/a/1.bms", 10, Some("one")),
        (
            "/r/fav.lr2folder",
            10,
            Some("#TITLE Fav\n#COMMAND clear = 1\n"),
        ),
        ("/r/.lr2folder", 10, Some("#TITLE no\n")),
        (
            "/n.lr2folder",
            10,
            Some("#TITLE New\n#COMMAND __NEWSONG__\n"),
        ),
    ]);
    let mut db = MemDb::default();
    run(&fs, &mut db, r, 1);
    change(&fs);
    run(&fs, &mut db, r, 2);
    let got: Vec<(String, String, String, i32, bool)> = db
        .folders
        .values()
        .filter_map(|(f, _)| {
            let c = f.custom.as_ref()?;
            Some((
                f.path.display().to_string(),
                f.title.clone(),
                c.command.clone(),
                c.kind,
                f.root,
            ))
        })
        .collect();
    let want: Vec<(String, String, String, i32, bool)> = want
        .iter()
        .map(|(p, t, c, k, root)| (p.to_string(), t.to_string(), c.to_string(), *k, *root))
        .collect();
    assert_eq!(got, want);
    // DB からの曲一覧ではカスタムフォルダ
    let lib = load(&db).unwrap();
    let mut names: Vec<String> = lib.custom().iter().map(|c| c.title.clone()).collect();
    names.sort();
    let mut titles: Vec<String> = want.iter().map(|w| w.1.clone()).collect();
    titles.sort();
    assert_eq!(names, titles);
}
