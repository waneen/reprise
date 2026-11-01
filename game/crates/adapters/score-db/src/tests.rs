//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::{fixture, rstest};
use tempfile::TempDir;

#[fixture]
fn dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn hash(n: u8) -> ChartHash {
    ChartHash {
        md5: [n; 16],
        sha256: None,
    }
}

fn record(n: u8, ruleset: RulesetId) -> ScoreRecord {
    ScoreRecord {
        chart: hash(n),
        ruleset,
        lamp: ClearLamp::Hard,
        ex_score: 250,
        pgreat: 100,
        great: 50,
        good: 5,
        bad: 2,
        poor: 3,
        max_combo: 120,
        min_bp: Some(5),
        play_count: 9,
        clear_count: 4,
        origin: ScoreOrigin::Native,
    }
}

/// SDB-01: フォルダとファイルを作る。開けなければ Io
#[rstest]
#[case::creates_dirs("a/b/score.db", true)]
#[case::directory_path("", false)]
fn sdb_01_open(dir: TempDir, #[case] rel: &str, #[case] ok: bool) {
    let p = dir.path().join(rel);
    let got = SqliteScoreDb::open(&p);
    assert_eq!(got.is_ok(), ok);
    if ok {
        assert!(p.exists());
        // 2 回目に開いてもよい
        assert!(SqliteScoreDb::open(&p).is_ok());
    } else {
        assert!(matches!(got, Err(PortError::Io(_))));
    }
}

/// SDB-02: 保存したものを読み戻せる。置き換えも
#[rstest]
#[case::plain(record(1, RulesetId::Lr2))]
#[case::unknown_bp_and_sha(ScoreRecord { min_bp: None, chart: ChartHash { md5: [2; 16], sha256: Some([7; 32]) }, ..record(2, RulesetId::Lr2) })]
#[case::imported(ScoreRecord { origin: ScoreOrigin::ImportedFrom("LR2".into()), lamp: ClearLamp::Max, ..record(3, RulesetId::Beatoraja) })]
fn sdb_02_round_trip(dir: TempDir, #[case] r: ScoreRecord) {
    let p = dir.path().join("score.db");
    let mut db = SqliteScoreDb::open(&p).unwrap();
    db.save(ScoreRecord {
        ex_score: 0,
        pgreat: 0,
        great: 0,
        ..r.clone()
    })
    .unwrap();
    db.save(r.clone()).unwrap();
    drop(db);
    let db = SqliteScoreDb::open(&p).unwrap();
    assert_eq!(db.find(&r.chart, r.ruleset).unwrap(), Some(r));
}

/// SDB-03: MD5・ルールが違えば別の記録
#[rstest]
#[case::same(1, RulesetId::Lr2, true)]
#[case::other_md5(2, RulesetId::Lr2, false)]
#[case::other_ruleset(1, RulesetId::Beatoraja, false)]
fn sdb_03_keys(dir: TempDir, #[case] n: u8, #[case] ruleset: RulesetId, #[case] found: bool) {
    let mut db = SqliteScoreDb::open(&dir.path().join("score.db")).unwrap();
    db.save(record(1, RulesetId::Lr2)).unwrap();
    assert_eq!(db.find(&hash(n), ruleset).unwrap().is_some(), found);
}

/// SDB-04: 列の値。ランプ 0..=10、ルール LR2 = 0・beatoraja = 1。知らないランプは NoPlay、知らないルールの行は無いもの
#[rstest]
#[case::no_play(ClearLamp::NoPlay, RulesetId::Lr2, (0, 0))]
#[case::full_combo(ClearLamp::FullCombo, RulesetId::Lr2, (8, 0))]
#[case::max_beatoraja(ClearLamp::Max, RulesetId::Beatoraja, (10, 1))]
fn sdb_04_codes(
    dir: TempDir,
    #[case] lamp: ClearLamp,
    #[case] ruleset: RulesetId,
    #[case] codes: (i64, i64),
) {
    let p = dir.path().join("score.db");
    let mut db = SqliteScoreDb::open(&p).unwrap();
    db.save(ScoreRecord {
        lamp,
        ..record(1, ruleset)
    })
    .unwrap();
    let raw = Connection::open(&p).unwrap();
    let stored: (i64, i64) = raw
        .query_row("SELECT lamp, ruleset FROM score", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(stored, codes);
    // 知らないランプは NoPlay
    raw.execute("UPDATE score SET lamp = 99", []).unwrap();
    let got = db.find(&hash(1), ruleset).unwrap().map(|r| r.lamp);
    assert_eq!(got, Some(ClearLamp::NoPlay));
    // 知らないルールの行は無いもの
    raw.execute("UPDATE score SET ruleset = 7", []).unwrap();
    assert_eq!(db.find(&hash(1), ruleset).unwrap(), None);
}
