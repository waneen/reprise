//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use crate::ports::PortError;
use rstest::rstest;
use std::path::PathBuf;
use std::sync::Mutex;

/// APP-CF-01: `.lr2folder` のファイル
#[rstest]
#[case::plain("a.lr2folder", true)]
#[case::upper("FAV.LR2FOLDER", true)]
#[case::only_ext(".lr2folder", false)]
#[case::other("a.lr2folder.txt", false)]
fn app_cf_01_is_lr2folder(#[case] name: &str, #[case] want: bool) {
    assert_eq!(is_lr2folder(name), want);
}

/// (題名, category, info_a, info_b, command, max, banner)
type Want<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str, i32, &'a str);

/// APP-CF-02: `.lr2folder` の読み方
#[rstest]
#[case::basic(
    "#TITLE Hard Clear\r\n#COMMAND clear = 4\r\n#MAXTRACKS 30\r\n",
    ("Hard Clear", "", "", "", "clear = 4", 30, "")
)]
#[case::aliases(
    "#title  X\n#CATEGORY c\n#INFORMATION_A ia\n#INFORMATION_B ib\n#TAG level > 10\n#PLAYLEVEL 5\n#BANNER b.png\n",
    (" X", "c", "ia", "ib", "level > 10", 5, "b.png")
)]
#[case::genre_artist("#GENRE g\n#ARTIST a\n#SUBARTIST s\n", ("", "g", "a", "s", "", 0, ""))]
#[case::later_wins("#TITLE a\n#TITLE b\n#TAG x = 1\n#COMMAND y = 2\n", ("b", "", "", "", "y = 2", 0, ""))]
#[case::title_suffix("#TITLE Easy Clear (Lv.12)\n", ("Easy Clear", "", "", "", "", 0, ""))]
#[case::title_brackets("#TITLE 段位 [発狂]\n", ("段位", "", "", "", "", 0, ""))]
#[case::title_leading_token("#TITLE (All)\n", ("(All)", "", "", "", "", 0, ""))]
#[case::if_block("#IF 2\n#TITLE skip\n#ENDIF\n#IF 1\n#TITLE keep\n#ENDIF\n", ("keep", "", "", "", "", 0, ""))]
#[case::not_command(" #TITLE x\nTITLE y\n#PLAYLEVEL x\n", ("", "", "", "", "", 0, ""))]
fn app_cf_02_parse(#[case] text: &str, #[case] want: Want) {
    let p = parse(Path::new("/b/f.lr2folder"), text);
    let f = &p.folder;
    assert_eq!(
        (
            f.title.as_str(),
            f.category.as_str(),
            f.info_a.as_str(),
            f.info_b.as_str(),
            f.command.as_str(),
            f.max,
            p.banner.as_str()
        ),
        want
    );
    assert_eq!(
        (f.path.clone(), f.kind),
        (PathBuf::from("/b/f.lr2folder"), 2)
    );
}

/// APP-CF-03: 登録したフォルダの .lr2folder の種類
#[rstest]
#[case::new_song("__NEWSONG__", 3)]
#[case::rival("__RIVAL__", 4)]
#[case::expert("__EXPERT__", 6)]
#[case::nonstop("__NONSTOP__", 6)]
#[case::grade("__GRADE__", 6)]
#[case::sql("clear = 1", 2)]
fn app_cf_03_root_kind(#[case] command: &str, #[case] want: i32) {
    assert_eq!(root_kind(command), want);
}

fn folder(command: &str, max: i32) -> CustomFolder {
    CustomFolder {
        command: command.into(),
        max,
        kind: 2,
        ..Default::default()
    }
}

const HEAD: &str = "SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE";

/// APP-CF-04: 開くときの SQL
#[rstest]
#[case::plain("clear = 4", 0, Some((format!("{HEAD} clear = 4 ORDER BY folder ASC,mode ASC,difficulty ASC,level ASC"), false)))]
#[case::order_by("level > 3 ORDER BY level DESC", 10, Some((format!("{HEAD} level > 3 ORDER BY level DESC"), true)))]
#[case::random_one("clear = 0", 1, Some((format!("{HEAD} clear = 0 ORDER BY random()"), true)))]
#[case::random_written("1 = 1 ORDER BY random() LIMIT 3", 1, Some((format!("{HEAD} 1 = 1 ORDER BY random() LIMIT 3"), true)))]
#[case::new_song("__NEWSONG__", 0, Some((format!("{HEAD} adddate > 13600 ORDER BY folder ASC,mode ASC,difficulty ASC,level ASC"), false)))]
#[case::course("__GRADE__", 0, None)]
#[case::rival("__RIVAL__", 0, None)]
#[case::short("ab", 0, None)]
fn app_cf_04_query(#[case] command: &str, #[case] max: i32, #[case] want: Option<(String, bool)>) {
    assert_eq!(query(&folder(command, max), 100_000), want);
}

struct Fake {
    result: Result<Vec<PathBuf>, ()>,
    asked: Mutex<Vec<String>>,
}

impl SongQuery for Fake {
    fn song_paths(&self, sql: &str) -> Result<Vec<PathBuf>, PortError> {
        self.asked.lock().unwrap().push(sql.into());
        self.result
            .clone()
            .map_err(|_| PortError::Parse("x".into()))
    }
}

/// APP-CF-05: カスタムフォルダの譜面を引く
#[rstest]
#[case::ok("clear = 4", 0, Ok(vec!["/a.bms"]), Some((vec!["/a.bms"], false)), 1)]
#[case::ordered("clear = 4", 1, Ok(vec![]), Some((vec![], true)), 1)]
#[case::sql_error("clear = 4", 0, Err(()), None, 1)]
#[case::not_openable("__EXPERT__", 0, Ok(vec!["/a.bms"]), None, 0)]
fn app_cf_05_evaluate(
    #[case] command: &str,
    #[case] max: i32,
    #[case] result: Result<Vec<&str>, ()>,
    #[case] want: Option<(Vec<&str>, bool)>,
    #[case] asked: usize,
) {
    let db = Fake {
        result: result.map(|v| v.iter().map(PathBuf::from).collect()),
        asked: Mutex::new(vec![]),
    };
    let got = evaluate(&db, &folder(command, max), 0);
    assert_eq!(
        got,
        want.map(|(p, ordered)| CustomResult {
            paths: p.iter().map(PathBuf::from).collect(),
            ordered
        })
    );
    assert_eq!(db.asked.lock().unwrap().len(), asked);
}
