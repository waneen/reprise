//! APP-SEL-* のテスト（ルール表は lib.rs）
use super::*;
use domain::chart::ChartHash;
use domain::play::RulesetId;
use domain::select::SongInfo;
use rstest::rstest;
use std::path::PathBuf;

fn song(title: &str, subtitle: &str, key_mode: KeyMode, level: i32, difficulty: i32) -> SongInfo {
    SongInfo {
        path: PathBuf::from(format!("/bms/a/{title}.bms")),
        hash: ChartHash {
            md5: [7; 16],
            sha256: None,
        },
        title: title.into(),
        subtitle: subtitle.into(),
        artist: "ART".into(),
        genre: String::new(),
        level,
        difficulty,
        key_mode,
    }
}

fn record(lamp: ClearLamp) -> ScoreRecord {
    let mut r = ScoreRecord::empty(
        ChartHash {
            md5: [7; 16],
            sha256: None,
        },
        RulesetId::Lr2,
    );
    r.lamp = lamp;
    r.ex_score = 1234;
    r.max_combo = 500;
    r.min_bp = Some(12);
    r.play_count = 9;
    r.clear_count = 4;
    r.pgreat = 600;
    r.great = 34;
    r.good = 5;
    r.bad = 2;
    r.poor = 3;
    r
}

fn bar_of(info: Option<SongInfo>, rec: Option<ScoreRecord>) -> SelectBar {
    let lib = Library::new(vec![], vec![], info.into_iter().collect());
    let bar = if lib.songs().is_empty() {
        Bar::Folder {
            path: "/bms/pack".into(),
            title: "pack".into(),
        }
    } else {
        Bar::Song(0)
    };
    SelectBar::new(&lib, &bar, &|_| rec.clone())
}

fn skin_with(bar: &SelectBar) -> SelectSkin {
    let mut s = SelectSkin::new(SceneTiming::default(), &[900], PlayOptions::default());
    s.select(Some(bar));
    s
}

/// APP-SEL-01: 一覧の 1 本
#[rstest]
#[case::song(Some(song("T", "[A]", KeyMode::K7, 12, 3)), Some(record(ClearLamp::Hard)), BarView { kind: 0, title: "T [A]".into(), key_mode: 7, level: 12, difficulty: 3, lamp: 4 })]
#[case::song_no_record(Some(song("T", "", KeyMode::K5, 3, 0)), None, BarView { kind: 0, title: "T".into(), key_mode: 5, level: 3, difficulty: 0, lamp: 0 })]
#[case::full_combo(Some(song("T", "", KeyMode::K14, 1, 1)), Some(record(ClearLamp::FullCombo)), BarView { kind: 0, title: "T".into(), key_mode: 14, level: 1, difficulty: 1, lamp: 5 })]
#[case::folder(None, None, BarView { kind: 1, title: "pack".into(), key_mode: 0, level: 0, difficulty: 0, lamp: 0 })]
fn app_sel_01_bar(
    #[case] info: Option<SongInfo>,
    #[case] rec: Option<ScoreRecord>,
    #[case] want: BarView,
) {
    assert_eq!(bar_of(info, rec).view(), want);
}

/// APP-SEL-02: タイマー（(時刻, 位置が動いた, 選んだバーが変わった) の後のタイマー 1, 10, 11 の経過）
#[rstest]
#[case::before_input(vec![(100, false, false)], [None, None, None])]
#[case::input_starts_1_and_11(vec![(301, false, false)], [Some(0), None, Some(0)])]
#[case::changed_restarts_11(vec![(301, false, false), (400, false, true), (450, false, false)], [Some(149), None, Some(50)])]
#[case::changed_before_input_ignored(vec![(100, false, true), (150, false, false)], [None, None, None])]
#[case::moving_restarts_10(vec![(301, false, false), (400, true, false), (420, true, false)], [Some(119), Some(0), Some(119)])]
#[case::still_stops_10(vec![(400, true, false), (420, false, false)], [Some(20), None, Some(20)])]
fn app_sel_02_timers(#[case] frames: Vec<(i64, bool, bool)>, #[case] want: [Option<i64>; 3]) {
    let mut s = SelectSkin::new(
        SceneTiming {
            start_input: 300,
            ..Default::default()
        },
        &[],
        PlayOptions::default(),
    );
    assert_eq!(s.timer(0), Some(0));
    for (t, moved, changed) in frames {
        s.update(t, moved, changed);
    }
    assert_eq!([s.timer(1), s.timer(10), s.timer(11)], want);
    assert_eq!(s.input_ready(), s.timer(1).is_some());
}

/// APP-SEL-03: 条件
#[rstest]
#[case::song_7k(Some(song("T", "", KeyMode::K7, 12, 2)), Some(record(ClearLamp::Easy)), vec![2, 5, 20, 50, 52, 102, 152, 160, 185, 900])]
#[case::song_high_level(Some(song("T", "", KeyMode::K7, 13, 0)), None, vec![2, 5, 20, 50, 52, 100, 150, 160, 186, 900])]
#[case::song_5k(Some(song("T", "", KeyMode::K5, 10, 6)), None, vec![2, 5, 20, 50, 52, 100, 161, 186, 900])]
#[case::folder(None, None, vec![1, 20, 50, 52, 150, 900])]
fn app_sel_03_options(
    #[case] info: Option<SongInfo>,
    #[case] rec: Option<ScoreRecord>,
    #[case] want: Vec<i32>,
) {
    let s = skin_with(&bar_of(info, rec));
    let got: Vec<i32> = (0..1000).filter(|&i| s.option(i)).collect();
    assert_eq!(got, want);
}

/// APP-SEL-04: 数値・文字列・BARGRAPH 3
#[rstest]
#[case::song(Some(song("T", "S", KeyMode::K7, 6, 2)), Some(record(ClearLamp::Hard)),
    vec![(42, Some(6)), (96, Some(6)), (71, Some(1234)), (75, Some(500)), (76, Some(12)), (77, Some(9)), (78, Some(4)), (79, Some(5)), (80, Some(600)), (84, Some(3)), (10, Some(100)), (70, None)],
    vec![(10, Some("T")), (11, Some("S")), (12, Some("T S")), (13, None), (14, Some("ART")), (17, Some("6")), (18, Some("2")), (22, Some("T S")), (27, Some("6"))],
    0.5)]
#[case::folder(None, None,
    vec![(42, None), (71, Some(0)), (76, Some(0)), (79, Some(0))],
    vec![(10, Some("pack")), (12, Some("pack")), (11, None), (17, None)],
    0.0)]
fn app_sel_04_values(
    #[case] info: Option<SongInfo>,
    #[case] rec: Option<ScoreRecord>,
    #[case] numbers: Vec<(i32, Option<i64>)>,
    #[case] texts: Vec<(i32, Option<&str>)>,
    #[case] level_bar: f64,
) {
    let s = skin_with(&bar_of(info, rec));
    for (id, want) in numbers {
        assert_eq!(s.number(id), want, "数値 {id}");
    }
    for (id, want) in texts {
        assert_eq!(s.text(id), want, "文字列 {id}");
    }
    assert_eq!(s.bar(3), level_bar);
}

/// APP-SEL-05: SLIDER 1
#[rstest]
#[case::top(0, 3, 0.0)]
#[case::last(2999, 3, 1.0)]
#[case::middle(1000, 2, 1000.0 / 1999.0)]
#[case::empty(0, 0, 0.0)]
fn app_sel_05_list_slider(#[case] position: i64, #[case] n: usize, #[case] want: f64) {
    let mut s = SelectSkin::new(SceneTiming::default(), &[], PlayOptions::default());
    s.set_list(position, n);
    assert_eq!(s.slider(1), want);
}

/// APP-SEL-06: BUTTON の状態値
#[rstest]
#[case::panel(1, Some(0))]
#[case::filter(10, Some(0))]
#[case::key_config(13, Some(0))]
#[case::replay(19, Some(0))]
#[case::fx(20, None)]
#[case::none(9, None)]
fn app_sel_06_buttons(#[case] action: i32, #[case] want: Option<i32>) {
    let s = SelectSkin::new(SceneTiming::default(), &[], PlayOptions::default());
    assert_eq!(s.button(action), want);
}
