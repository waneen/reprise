//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::{fixture, rstest};
use tempfile::TempDir;

#[fixture]
fn dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn sep(s: &str) -> String {
    s.replace('/', std::path::MAIN_SEPARATOR_STR)
}

fn song(path: &str, md5: u8, sha: Option<u8>) -> SongEntry {
    SongEntry {
        info: SongInfo {
            path: sep(path).into(),
            hash: ChartHash {
                md5: [md5; 16],
                sha256: sha.map(|b| [b; 32]),
            },
            title: "t".into(),
            subtitle: "s".into(),
            artist: "a".into(),
            genre: "g".into(),
            level: 12,
            difficulty: 4,
            key_mode: KeyMode::K14,
            notes: 1234,
        },
        date: 99,
        subartist: "sa".into(),
        stage_file: "st.png".into(),
        banner: "b.png".into(),
        back_bmp: "bb.png".into(),
        min_bpm: 90,
        max_bpm: 180,
        judge: 3,
        long_note: true,
        bga: false,
        txt: true,
    }
}

fn folder(path: &str, root: bool) -> FolderEntry {
    FolderEntry {
        path: sep(path).into(),
        title: "名前".into(),
        root,
        date: 5,
        custom: None,
        banner: String::new(),
    }
}

fn tables(db: &SqliteSongDb) -> Vec<String> {
    let mut st = db
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'index') ORDER BY name")
        .unwrap();
    st.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// SNG-01: 開く
#[rstest]
#[case::creates_dirs("a/b/song.db", true)]
#[case::directory_path("", false)]
fn sng_01_open(dir: TempDir, #[case] rel: &str, #[case] ok: bool) {
    let p = dir.path().join(rel);
    let got = SqliteSongDb::open(&p);
    assert_eq!(got.is_ok(), ok);
    match got {
        Ok(db) => {
            let t = tables(&db);
            for name in [
                "expert",
                "folder",
                "grade",
                "hashidx",
                "nonstop",
                "parentidx",
                "reprise_song",
                "song",
            ] {
                assert!(t.iter().any(|x| x == name), "{name}");
            }
            let cols: Vec<String> = db
                .connection()
                .prepare("SELECT * FROM song")
                .unwrap()
                .column_names()
                .map(String::from)
                .collect();
            assert_eq!((cols.len(), cols[28].as_str()), (29, "exlevel"));
            assert!(SqliteSongDb::open(&p).is_ok());
        }
        Err(e) => assert!(matches!(e, PortError::Io(_))),
    }
}

/// SNG-02: フォルダの行
#[rstest]
#[case::root("/bms", true, "e2977170")]
#[case::child("/bms/pack", false, "")]
fn sng_02_folder(dir: TempDir, #[case] path: &str, #[case] root: bool, #[case] parent: &str) {
    let mut db = SqliteSongDb::open(&dir.path().join("song.db")).unwrap();
    let f = folder(path, root);
    assert!(db.insert_folder(&f, 77).unwrap());
    assert!(!db
        .insert_folder(
            &FolderEntry {
                date: 1,
                ..f.clone()
            },
            78
        )
        .unwrap());
    assert_eq!(db.folders().unwrap(), vec![f.clone()]);
    let (p, par, ty, added): (String, String, i64, i64) = db
        .connection()
        .query_row("SELECT path, parent, type, adddate FROM folder", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    let want_parent = if root {
        parent.to_string()
    } else {
        lr2_crc32(&sep("/bms/"))
    };
    assert_eq!(
        (p, par, ty, added),
        (sep(&format!("{path}/")), want_parent, 1, 77)
    );
}

/// SNG-03: 譜面の行
#[rstest]
#[case::with_sha(Some(7))]
#[case::without_sha(None)]
fn sng_03_song(dir: TempDir, #[case] sha: Option<u8>) {
    let mut db = SqliteSongDb::open(&dir.path().join("song.db")).unwrap();
    let s = song("/bms/pack/x/a.bms", 1, sha);
    db.put_song(&s, 50).unwrap();
    assert_eq!(db.songs().unwrap(), vec![s.clone()]);
    assert_eq!(db.song_date(&s.info.path).unwrap(), Some(99));
    let row: (String, String, String, i64, i64, i64, Option<String>, i64) = db
        .connection()
        .query_row(
            "SELECT hash, folder, parent, mode, karinotes, adddate, tag, type + favorite + random + exlevel FROM song",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "01".repeat(16),
            lr2_crc32(&sep("/bms/pack/x/")),
            lr2_crc32(&sep("/bms/pack/")),
            14,
            1234,
            50,
            None,
            0
        )
    );
    // 置き換え
    let mut t = s.clone();
    t.info.title = "new".into();
    t.info.hash.sha256 = None;
    db.put_song(&t, 60).unwrap();
    assert_eq!(db.songs().unwrap(), vec![t]);
}

/// SNG-04: LR2 の CRC32
#[rstest]
#[case::root("ROOT", "e2977170")]
#[case::ascii("/home/a/", "83511c75")]
#[case::shift_jis("D:\\BMS\\曲\\", "d24e2220")]
fn sng_04_crc(#[case] s: &str, #[case] want: &str) {
    assert_eq!(lr2_crc32(s), want);
}

/// SNG-05: 消す・書き換える
#[rstest]
#[case::delete_song("song")]
#[case::delete_folder("folder")]
#[case::touch("touch")]
#[case::difficulty("difficulty")]
#[case::clear("clear")]
fn sng_05_edit(dir: TempDir, #[case] op: &str) {
    let mut db = SqliteSongDb::open(&dir.path().join("song.db")).unwrap();
    let s = song("/bms/x/a.bms", 1, Some(2));
    let f = folder("/bms/x", false);
    db.put_song(&s, 1).unwrap();
    db.put_song(&song("/bms/x/b.bms", 2, None), 1).unwrap();
    db.insert_folder(&f, 1).unwrap();
    db.begin().unwrap();
    match op {
        "song" => db.delete_song(&s.info.path).unwrap(),
        "folder" => db.delete_folder(&f.path).unwrap(),
        "touch" => db.touch_folder(&f.path, 42, 43).unwrap(),
        "difficulty" => db.set_difficulty(&s.info.path, 1).unwrap(),
        _ => db.clear().unwrap(),
    }
    db.commit().unwrap();
    let songs: Vec<(String, i32)> = db
        .songs()
        .unwrap()
        .into_iter()
        .map(|s| {
            (
                s.info
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                s.info.difficulty,
            )
        })
        .collect();
    let folders: Vec<i64> = db.folders().unwrap().into_iter().map(|f| f.date).collect();
    let shas: i64 = db
        .connection()
        .query_row("SELECT count(*) FROM reprise_song", [], |r| r.get(0))
        .unwrap();
    let want: (Vec<(&str, i32)>, Vec<i64>, i64) = match op {
        "song" => (vec![("b.bms", 4)], vec![5], 1),
        "folder" => (vec![("a.bms", 4), ("b.bms", 4)], vec![], 2),
        "touch" => (vec![("a.bms", 4), ("b.bms", 4)], vec![42], 2),
        "difficulty" => (vec![("a.bms", 1), ("b.bms", 4)], vec![5], 2),
        _ => (vec![], vec![], 0),
    };
    let songs: Vec<(&str, i32)> = songs.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    assert_eq!((songs, folders, shas), want);
}

fn custom_row(path: &str, root: bool, kind: i32) -> FolderEntry {
    FolderEntry {
        path: sep(path).into(),
        title: "カスタム".into(),
        root,
        date: 6,
        custom: Some(CustomFolder {
            path: sep(path).into(),
            title: "カスタム".into(),
            category: "c".into(),
            info_a: "ia".into(),
            info_b: "ib".into(),
            command: "level >= 12".into(),
            max: 3,
            kind,
        }),
        banner: "bn.png".into(),
    }
}

/// SNG-06: カスタムフォルダの行
#[rstest]
#[case::inside("/bms/pack/a.lr2folder", false, 2)]
#[case::root("/x/b.lr2folder", true, 3)]
fn sng_06_custom(dir: TempDir, #[case] path: &str, #[case] root: bool, #[case] kind: i32) {
    let mut db = SqliteSongDb::open(&dir.path().join("song.db")).unwrap();
    let f = custom_row(path, root, kind);
    assert!(db.insert_folder(&f, 70).unwrap());
    assert!(!db.insert_folder(&f, 71).unwrap());
    assert_eq!(db.folders().unwrap(), vec![f.clone()]);
    let (p, parent, ty, max, banner): (String, String, i64, i64, String) = db
        .connection()
        .query_row(
            "SELECT path, parent, type, max, banner FROM folder",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    let want_parent = if root {
        lr2_crc32("ROOT")
    } else {
        folder_crc(Path::new(&sep("/bms/pack")))
    };
    assert_eq!(
        (p, parent, ty, max, banner),
        (sep(path), want_parent, kind as i64, 3, "bn.png".into())
    );
    db.delete_folder(&f.path).unwrap();
    assert_eq!(db.folders().unwrap(), vec![]);
}

fn record(
    md5: u8,
    lamp: domain::score::ClearLamp,
    ruleset: domain::play::RulesetId,
) -> domain::score::ScoreRecord {
    domain::score::ScoreRecord {
        chart: ChartHash {
            md5: [md5; 16],
            sha256: None,
        },
        ruleset,
        lamp,
        ex_score: 0,
        pgreat: 500,
        great: 100,
        good: 0,
        bad: 0,
        poor: 0,
        max_combo: 600,
        min_bp: Some(3),
        play_count: 5,
        clear_count: 2,
        origin: domain::score::ScoreOrigin::Native,
    }
}

/// SNG-07: SQL で引く（カスタムフォルダ）
#[rstest]
#[case::where_level("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE level >= 12 ORDER BY song.path", true, Ok(vec!["/b/1.bms", "/b/2.bms"]))]
#[case::lamp("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE clear = 4", true, Ok(vec!["/b/1.bms"]))]
#[case::rate_rank("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE rate = 55 AND rank = 4 AND failcount = 3 AND totalnotes = 1000", true, Ok(vec!["/b/1.bms"]))]
#[case::beatoraja_rule_ignored("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE clear = 5", true, Ok(vec![]))]
#[case::no_play("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE clear IS NULL", true, Ok(vec!["/b/2.bms"]))]
#[case::no_score_db("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE clear IS NULL ORDER BY song.path", false, Ok(vec!["/b/1.bms", "/b/2.bms"]))]
#[case::bad_sql("SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE nope(", true, Err(()))]
fn sng_07_query(
    dir: TempDir,
    #[case] sql: &str,
    #[case] scores: bool,
    #[case] want: Result<Vec<&str>, ()>,
) {
    use application::ports::ScoreRepository;
    use domain::play::RulesetId;
    use domain::score::ClearLamp;
    let mut db = SqliteSongDb::open(&dir.path().join("song.db")).unwrap();
    let mut a = song("/b/1.bms", 1, None);
    a.info.notes = 1000;
    db.put_song(&a, 1).unwrap();
    db.put_song(&song("/b/2.bms", 2, None), 1).unwrap();
    let score_path = dir.path().join("score.db");
    if scores {
        let mut s = score_db::SqliteScoreDb::open(&score_path).unwrap();
        s.save(record(1, ClearLamp::Hard, RulesetId::Lr2)).unwrap();
        s.save(record(2, ClearLamp::FullCombo, RulesetId::Beatoraja))
            .unwrap();
    }
    let q = SqliteSongQuery::open(&dir.path().join("song.db"), &score_path).unwrap();
    let got = q.song_paths(sql).map_err(|_| ());
    let want = want.map(|v| v.iter().map(|p| PathBuf::from(sep(p))).collect::<Vec<_>>());
    assert_eq!(got, want);
}

/// SNG-08: Shift_JIS
#[rstest]
#[case::kana(&[0x83, 0x65, 0x83, 0x58, 0x83, 0x67], "テスト")]
#[case::ascii(b"#TITLE a", "#TITLE a")]
fn sng_08_shift_jis(#[case] bytes: &[u8], #[case] want: &str) {
    assert_eq!(shift_jis(bytes), want);
}
