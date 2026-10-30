//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
//! LR2 と同じ定義（OpenLR2 の CREATE TABLE）で一時 DB を作って読む。
use super::*;
use rstest::{fixture, rstest};
use tempfile::TempDir;

const CREATE: &str = "CREATE TABLE score(hash TEXT primary key,clear INTEGER,perfect INTEGER,great INTEGER,good INTEGER,bad INTEGER,poor INTEGER,totalnotes INTEGER,maxcombo INTEGER,minbp INTEGER,playcount INTEGER,clearcount INTEGER,failcount INTEGER,rank INTEGER,rate INTEGER,clear_db INTEGER,op_history INTEGER,scorehash TEXT,ghost TEXT,clear_sd INTEGER,clear_ex INTEGER,op_best INTEGER, rseed INTEGER, complete INTEGER)";
const MD5: &str = "0123456789abcdef0123456789abcdef";

#[fixture]
fn dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// (hash, clear, perfect, great, good, bad, poor, maxcombo, minbp, playcount, clearcount)
type Row<'a> = (&'a str, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);

fn db(dir: &TempDir, rows: &[Row]) -> std::path::PathBuf {
    let p = dir.path().join("player.db");
    let c = Connection::open(&p).unwrap();
    c.execute(CREATE, []).unwrap();
    for r in rows {
        c.execute(
            "INSERT INTO score(hash,clear,perfect,great,good,bad,poor,maxcombo,minbp,playcount,clearcount) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10],
        )
        .unwrap();
    }
    p
}
fn row(hash: &str) -> Row<'_> {
    (hash, 2, 100, 50, 5, 2, 3, 120, 7, 9, 4)
}
fn import_one(dir: &TempDir, r: Row) -> Option<ScoreRecord> {
    Lr2ScoreDb.import(&db(dir, &[r])).unwrap().pop()
}

/// IMP-LR2-01: 開けない・テーブルが無ければ Io
#[rstest]
#[case::missing_file(false)]
#[case::no_score_table(true)]
fn imp_lr2_01_open_errors(dir: TempDir, #[case] create_empty: bool) {
    let p = dir.path().join("x.db");
    if create_empty {
        Connection::open(&p)
            .unwrap()
            .execute("CREATE TABLE other(a)", [])
            .unwrap();
    }
    assert!(matches!(Lr2ScoreDb.import(&p), Err(PortError::Io(_))));
}

/// IMP-LR2-02: hash
#[rstest]
#[case::lower_md5(MD5, true)]
#[case::upper_md5("0123456789ABCDEF0123456789ABCDEF", true)]
#[case::course_hash(&"0".repeat(96), false)]
#[case::not_hex("zz23456789abcdef0123456789abcdef", false)]
fn imp_lr2_02_hash(dir: TempDir, #[case] hash: &str, #[case] kept: bool) {
    let got = import_one(&dir, row(hash));
    assert_eq!(got.is_some(), kept);
    if let Some(r) = got {
        assert_eq!(r.chart.md5[0], 0x01);
        assert_eq!(r.chart.md5[15], 0xef);
    }
}

/// IMP-LR2-03: ランプ
#[rstest]
#[case::no_play(0, ClearLamp::NoPlay)]
#[case::failed(1, ClearLamp::Failed)]
#[case::easy(2, ClearLamp::Easy)]
#[case::normal(3, ClearLamp::Normal)]
#[case::hard(4, ClearLamp::Hard)]
#[case::full_combo(5, ClearLamp::FullCombo)]
#[case::unknown(9, ClearLamp::NoPlay)]
fn imp_lr2_03_lamp(dir: TempDir, #[case] clear: i64, #[case] expected: ClearLamp) {
    let mut r = row(MD5);
    r.1 = clear;
    assert_eq!(import_one(&dir, r).unwrap().lamp, expected);
}

/// IMP-LR2-04: 判定内訳と EX スコア
#[rstest]
#[case::typical((100, 50, 5, 2, 3), 250)]
#[case::all_zero((0, 0, 0, 0, 0), 0)]
fn imp_lr2_04_judges_and_ex(dir: TempDir, #[case] j: (i64, i64, i64, i64, i64), #[case] ex: u32) {
    let mut r = row(MD5);
    (r.2, r.3, r.4, r.5, r.6) = j;
    let got = import_one(&dir, r).unwrap();
    assert_eq!(
        (
            got.ex_score,
            got.pgreat,
            got.great,
            got.good,
            got.bad,
            got.poor
        ),
        (ex, j.0 as u32, j.1 as u32, j.2 as u32, j.3 as u32, j.4 as u32)
    );
}

/// IMP-LR2-05: コンボ・BP・回数
#[rstest]
#[case::plain(120, 7, 9, 4, (120, Some(7), 9, 4))]
#[case::negative_minbp_unknown(120, -1, 9, 4, (120, None, 9, 4))]
#[case::negative_counts_zero(-5, 0, -2, -1, (0, Some(0), 0, 0))]
fn imp_lr2_05_combo_bp_plays(
    dir: TempDir,
    #[case] combo: i64,
    #[case] minbp: i64,
    #[case] plays: i64,
    #[case] clears: i64,
    #[case] expected: (u32, Option<u32>, u32, u32),
) {
    let mut r = row(MD5);
    (r.7, r.8, r.9, r.10) = (combo, minbp, plays, clears);
    let got = import_one(&dir, r).unwrap();
    assert_eq!(
        (got.max_combo, got.min_bp, got.play_count, got.clear_count),
        expected
    );
}

/// IMP-LR2-06: ルール・出自
#[rstest]
#[case::single(1)]
#[case::three(3)]
fn imp_lr2_06_ruleset_and_origin(dir: TempDir, #[case] n: usize) {
    let hashes: Vec<String> = (0..n).map(|i| format!("{i:032x}")).collect();
    let rows: Vec<Row> = hashes.iter().map(|h| row(h)).collect();
    let got = Lr2ScoreDb.import(&db(&dir, &rows)).unwrap();
    assert_eq!(got.len(), n);
    assert!(got.iter().all(|r| r.ruleset == RulesetId::Lr2
        && r.origin == ScoreOrigin::ImportedFrom("LR2".into())
        && r.chart.sha256.is_none()));
}
