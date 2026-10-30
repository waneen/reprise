//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
//! beatoraja と同じ列構成で一時 DB を作って読む（`root/player/p1/score.db` と `root/songdata.db`）。
use super::*;
use rstest::{fixture, rstest};
use tempfile::TempDir;

const SCORE: &str = "CREATE TABLE [score] ([sha256] TEXT NOT NULL,[mode] INTEGER,[clear] INTEGER,[epg] INTEGER,[lpg] INTEGER,[egr] INTEGER,[lgr] INTEGER,[egd] INTEGER,[lgd] INTEGER,[ebd] INTEGER,[lbd] INTEGER,[epr] INTEGER,[lpr] INTEGER,[ems] INTEGER,[lms] INTEGER,[notes] INTEGER,[combo] INTEGER,[minbp] INTEGER,[avgjudge] INTEGER NOT NULL DEFAULT 2147483647,[playcount] INTEGER,[clearcount] INTEGER,[trophy] TEXT,[ghost] TEXT,[option] INTEGER,[seed] INTEGER,[random] INTEGER,[date] INTEGER,[state] INTEGER,[scorehash] TEXT,PRIMARY KEY(sha256,mode))";
const SONG: &str = "CREATE TABLE song(md5 TEXT NOT NULL, sha256 TEXT NOT NULL, title TEXT)";

fn sha(i: u8) -> String {
    format!("{i:02x}").repeat(32)
}
fn md5(i: u8) -> String {
    format!("{i:02x}").repeat(16)
}

/// 1 行分: (sha256, mode, clear, [epg,lpg,egr,lgr,egd,lgd,ebd,lbd,epr,lpr,ems,lms], combo, minbp, playcount, clearcount)
#[derive(Clone)]
struct Row {
    sha: String,
    mode: i64,
    clear: i64,
    j: [i64; 12],
    combo: i64,
    minbp: i64,
    plays: i64,
    clears: i64,
}
fn row(i: u8) -> Row {
    Row {
        sha: sha(i),
        mode: 0,
        clear: 4,
        j: [60, 40, 30, 20, 3, 2, 1, 1, 2, 1, 5, 5],
        combo: 150,
        minbp: 6,
        plays: 12,
        clears: 5,
    }
}

#[fixture]
fn root() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// root に songdata.db（sha(i)→md5(i), i in songs）と player/p1/score.db を作る
fn setup(root: &TempDir, songs: &[u8], rows: &[Row]) -> PathBuf {
    let song = Connection::open(root.path().join("songdata.db")).unwrap();
    song.execute(SONG, []).unwrap();
    for i in songs {
        song.execute(
            "INSERT INTO song(md5, sha256, title) VALUES (?1, ?2, 't')",
            [md5(*i), sha(*i)],
        )
        .unwrap();
    }
    let dir = root.path().join("player/p1");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("score.db");
    let c = Connection::open(&p).unwrap();
    c.execute(SCORE, []).unwrap();
    for r in rows {
        c.execute(
            "INSERT INTO score(sha256,mode,clear,epg,lpg,egr,lgr,egd,lgd,ebd,lbd,epr,lpr,ems,lms,combo,minbp,playcount,clearcount) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            rusqlite::params![
                r.sha, r.mode, r.clear, r.j[0], r.j[1], r.j[2], r.j[3], r.j[4], r.j[5], r.j[6],
                r.j[7], r.j[8], r.j[9], r.j[10], r.j[11], r.combo, r.minbp, r.plays, r.clears
            ],
        )
        .unwrap();
    }
    p
}
fn import_one(root: &TempDir, r: Row) -> Option<ScoreRecord> {
    let p = setup(root, &[1], &[r]);
    BeatorajaScoreDb::beatoraja().import(&p).unwrap().pop()
}

/// IMP-BRJ-01: 開けない・テーブルが無ければ Io
#[rstest]
#[case::missing_score_db(false)]
#[case::no_score_table(true)]
fn imp_brj_01_open_errors(root: TempDir, #[case] create_empty: bool) {
    let p = root.path().join("score.db");
    if create_empty {
        Connection::open(&p)
            .unwrap()
            .execute("CREATE TABLE x(a)", [])
            .unwrap();
    }
    assert!(matches!(
        BeatorajaScoreDb::beatoraja().import(&p),
        Err(PortError::Io(_))
    ));
}

/// IMP-BRJ-02: sha256 と mode
#[rstest]
#[case::normal(sha(1), 0, true)]
#[case::upper_hex(sha(1).to_uppercase(), 0, true)]
#[case::course(format!("{}{}", sha(1), sha(1)), 0, false)]
#[case::cn_mode(sha(1), 1, false)]
#[case::course_mode(sha(1), 30010, false)]
fn imp_brj_02_sha_and_mode(
    root: TempDir,
    #[case] s: String,
    #[case] mode: i64,
    #[case] kept: bool,
) {
    let r = Row {
        sha: s,
        mode,
        ..row(1)
    };
    assert_eq!(import_one(&root, r).is_some(), kept);
}

/// IMP-BRJ-03: 曲 DB で MD5 を引く
#[rstest]
#[case::found(vec![1, 2], 2, Some(2))]
#[case::not_in_song_db(vec![1], 3, None)]
fn imp_brj_03_md5_from_song_db(
    root: TempDir,
    #[case] songs: Vec<u8>,
    #[case] i: u8,
    #[case] md5_byte: Option<u8>,
) {
    let p = setup(&root, &songs, &[row(i)]);
    let got = BeatorajaScoreDb::beatoraja().import(&p).unwrap();
    assert_eq!(got.first().map(|r| r.chart.md5), md5_byte.map(|b| [b; 16]));
}

/// IMP-BRJ-04: ランプ
#[rstest]
#[case::no_play(0, ClearLamp::NoPlay)]
#[case::failed(1, ClearLamp::Failed)]
#[case::assist(2, ClearLamp::AssistEasy)]
#[case::light_assist(3, ClearLamp::LightAssistEasy)]
#[case::easy(4, ClearLamp::Easy)]
#[case::normal(5, ClearLamp::Normal)]
#[case::hard(6, ClearLamp::Hard)]
#[case::ex_hard(7, ClearLamp::ExHard)]
#[case::full_combo(8, ClearLamp::FullCombo)]
#[case::perfect(9, ClearLamp::Perfect)]
#[case::max(10, ClearLamp::Max)]
#[case::unknown(11, ClearLamp::NoPlay)]
fn imp_brj_04_lamp(root: TempDir, #[case] clear: i64, #[case] expected: ClearLamp) {
    let r = Row { clear, ..row(1) };
    assert_eq!(import_one(&root, r).unwrap().lamp, expected);
}

/// IMP-BRJ-05: 判定内訳と EX スコア
#[rstest]
#[case::typical([60, 40, 30, 20, 3, 2, 1, 1, 2, 1, 5, 5], (250, 100, 50, 5, 2, 3))]
#[case::zeros([0; 12], (0, 0, 0, 0, 0, 0))]
fn imp_brj_05_judges_and_ex(
    root: TempDir,
    #[case] j: [i64; 12],
    #[case] expected: (u32, u32, u32, u32, u32, u32),
) {
    let got = import_one(&root, Row { j, ..row(1) }).unwrap();
    assert_eq!(
        (
            got.ex_score,
            got.pgreat,
            got.great,
            got.good,
            got.bad,
            got.poor
        ),
        expected
    );
}

/// IMP-BRJ-06: コンボ・BP・回数
#[rstest]
#[case::plain(150, 6, 12, 5, (150, Some(6), 12, 5))]
#[case::minbp_unset(150, 2147483647, 12, 5, (150, None, 12, 5))]
#[case::minbp_negative(150, -1, 12, 0, (150, None, 12, 0))]
fn imp_brj_06_combo_bp_plays(
    root: TempDir,
    #[case] combo: i64,
    #[case] minbp: i64,
    #[case] plays: i64,
    #[case] clears: i64,
    #[case] expected: (u32, Option<u32>, u32, u32),
) {
    let r = Row {
        combo,
        minbp,
        plays,
        clears,
        ..row(1)
    };
    let got = import_one(&root, r).unwrap();
    assert_eq!(
        (got.max_combo, got.min_bp, got.play_count, got.clear_count),
        expected
    );
}

/// IMP-BRJ-07: ルール・出自・SHA-256
#[rstest]
#[case::beatoraja(BeatorajaScoreDb::beatoraja(), RulesetId::Beatoraja, "beatoraja")]
#[case::lr2oraja(BeatorajaScoreDb::lr2oraja(), RulesetId::Lr2, "LR2oraja")]
fn imp_brj_07_ruleset_and_origin(
    root: TempDir,
    #[case] imp: BeatorajaScoreDb,
    #[case] ruleset: RulesetId,
    #[case] name: &str,
) {
    let p = setup(&root, &[1], &[row(1)]);
    let got = imp.import(&p).unwrap().pop().unwrap();
    assert_eq!(imp.source_name(), name);
    assert_eq!(
        (got.ruleset, got.origin, got.chart.sha256),
        (
            ruleset,
            ScoreOrigin::ImportedFrom(name.into()),
            Some([1; 32])
        )
    );
}
