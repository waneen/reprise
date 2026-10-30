//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use domain::chart::ChartHash;
use domain::input::{SCRATCH_UP, START};
use domain::play::RulesetId;
use rstest::rstest;

fn timing(start_input: i32, rank: i32, update: i32, fade_out: i32) -> SceneTiming {
    SceneTiming {
        start_input,
        start_input_rank: rank,
        start_input_update: update,
        fade_out,
        ..Default::default()
    }
}

fn rec(ex: u32, max_combo: u32, min_bp: Option<u32>) -> ScoreRecord {
    let mut r = ScoreRecord::empty(
        ChartHash {
            md5: [1; 16],
            sha256: None,
        },
        RulesetId::Lr2,
    );
    r.ex_score = ex;
    r.max_combo = max_combo;
    r.min_bp = min_bp;
    r
}

/// PG 4・GR 1・GD 1・BD 1（EX 9）、総ノーツ 7・片付いたノーツ 7、コンボ 1・最大コンボ 3、HP 47、保存前 EX 10・最大コンボ 5・最小 BP 3
fn res() -> PlayResult {
    PlayResult {
        texts: [(10, "T".to_string())].into_iter().collect(),
        key_mode: KeyMode::K7,
        groove: true,
        options: PlayOptions {
            hi_speed: 250,
            judge_offset: -12,
            cover: 30,
            ..Default::default()
        },
        counts: JudgeCounts {
            pgreat: 4,
            great: 1,
            good: 1,
            bad: 1,
            poor: 0,
        },
        total: 7,
        processed: 7,
        combo: 1,
        max_combo: 3,
        hp: 47.0,
        lamp: ClearLamp::Failed,
        graph: vec![0; 1000],
        saved: Some(SavedScore {
            before: Some(rec(10, 5, Some(3))),
            after: rec(10, 5, Some(1)),
        }),
    }
}

/// 押す入力の種類
#[derive(Clone, Copy, Debug)]
enum In {
    None,
    Key(usize),
    Scratch,
    Start,
    Esc,
    Enter,
    MouseL(u8),
    MouseR(u8),
}

/// (時刻, 入力, 押し続けている鍵盤) を順に進め、最後のフレームの結果を返す
fn run(s: &mut ResultSkin, frames: &[(i64, In)], held: &[usize]) -> Option<ResultExit> {
    let mut out = None;
    for &(t, i) in frames {
        let ev = match i {
            In::Key(n) => vec![(key_slot(n), true)],
            In::Scratch => vec![(SCRATCH_UP, true)],
            In::Start => vec![(START, true)],
            _ => vec![],
        };
        let mut h = [false; SLOTS];
        for &k in held {
            h[key_slot(k)] = true;
        }
        out = s.frame(
            t,
            ResultInput {
                slots: &ev,
                held: h,
                escape: matches!(i, In::Esc),
                enter: matches!(i, In::Enter),
                mouse_left: if let In::MouseL(m) = i { m } else { 0 },
                mouse_right: if let In::MouseR(m) = i { m } else { 0 },
            },
        );
    }
    out
}

/// APP-RES-01: タイマー 0・1・11・150・151（入力開始 300、グラフ 500..1500）
#[rstest]
#[case::start(vec![0], [Some(0), None, None, None, None])]
#[case::input(vec![0, 301], [Some(301), Some(0), Some(0), None, None])]
#[case::chart_start(vec![0, 301, 501], [Some(501), Some(200), Some(200), Some(0), None])]
#[case::chart_end(vec![0, 301, 501, 1501], [Some(1501), Some(1200), Some(1200), Some(1000), Some(0)])]
#[case::exactly_not_yet(vec![300, 500, 1500], [Some(1500), Some(1000), Some(1000), Some(0), None])]
fn app_res_01_timers(#[case] frames: Vec<i64>, #[case] want: [Option<i64>; 5]) {
    let mut s = ResultSkin::new(timing(300, 0, 0, 0), &[], (500, 1500), res());
    assert_eq!(s.timer(0), Some(0));
    let f: Vec<(i64, In)> = frames.into_iter().map(|t| (t, In::None)).collect();
    run(&mut s, &f, &[]);
    let got = [0, 1, 11, 150, 151].map(|id| s.timer(id));
    assert_eq!(got, want);
}

/// APP-RES-02: 入力で進める（入力開始 300、ランク 1000、記録更新 500、グラフの伸び終わり 5000）→ (151, 152, 2, 伸び終わり)
#[rstest]
#[case::before_input(true, vec![(300, In::Esc)], (None, None, None, None))]
#[case::first_shows_rank(true, vec![(400, In::Esc)], (Some(0), None, None, Some(400)))]
#[case::key(true, vec![(400, In::Key(7))], (Some(0), None, None, Some(400)))]
#[case::enter(true, vec![(400, In::Enter)], (Some(0), None, None, Some(400)))]
#[case::mouse_left_held(true, vec![(400, In::MouseL(2))], (Some(0), None, None, Some(400)))]
#[case::mouse_right_held(true, vec![(400, In::MouseR(2))], (Some(0), None, None, Some(400)))]
#[case::mouse_click_ignored(true, vec![(400, In::MouseL(1)), (410, In::MouseR(1))], (None, None, None, None))]
#[case::scratch_start_ignored(true, vec![(400, In::Scratch), (410, In::Start)], (None, None, None, None))]
#[case::rank_too_early(true, vec![(400, In::Esc), (1400, In::Esc)], (Some(1000), None, None, Some(400)))]
#[case::update(true, vec![(400, In::Esc), (1401, In::Esc)], (Some(1001), Some(0), None, Some(400)))]
#[case::not_saved_closes(false, vec![(400, In::Esc), (1401, In::Esc)], (Some(1001), None, Some(0), Some(400)))]
#[case::update_too_early(true, vec![(400, In::Esc), (1401, In::Esc), (1901, In::Esc)], (Some(1501), Some(500), None, Some(400)))]
#[case::close(true, vec![(400, In::Esc), (1401, In::Esc), (1902, In::Esc)], (Some(1502), Some(501), Some(0), Some(400)))]
#[case::chart_ended_by_time(true, vec![(5001, In::None), (6002, In::Esc)], (Some(1001), Some(0), None, None))]
fn app_res_02_advance(
    #[case] saved: bool,
    #[case] frames: Vec<(i64, In)>,
    #[case] want: (Option<i64>, Option<i64>, Option<i64>, Option<i64>),
) {
    let mut r = res();
    if !saved {
        r.saved = None;
    }
    let mut s = ResultSkin::new(timing(300, 1000, 500, 1000), &[], (0, 5000), r);
    run(&mut s, &frames, &[]);
    assert_eq!(
        (s.timer(151), s.timer(152), s.timer(2), s.chart_end()),
        want
    );
}

/// APP-RES-03: 閉じる（1902 で閉じ始める。`#FADEOUT` 500）。押し続けている鍵盤で行き先が決まる
#[rstest]
#[case::fading(500, 2402, &[], None)]
#[case::to_select(500, 2403, &[], Some(ResultExit::Select))]
#[case::retry(500, 2403, &[1, 2], Some(ResultExit::Retry))]
#[case::retry_7_6(500, 2403, &[7, 6], Some(ResultExit::Retry))]
#[case::white_only(500, 2403, &[1, 3], Some(ResultExit::Select))]
#[case::black_only(500, 2403, &[2, 4], Some(ResultExit::Select))]
#[case::no_fadeout(0, 1903, &[], Some(ResultExit::Select))]
fn app_res_03_close(
    #[case] fade_out: i32,
    #[case] t: i64,
    #[case] held: &[usize],
    #[case] want: Option<ResultExit>,
) {
    let mut s = ResultSkin::new(timing(300, 1000, 500, fade_out), &[], (0, 5000), res());
    let start = [(400, In::Esc), (1401, In::Esc), (1902, In::Esc)];
    assert_eq!(run(&mut s, &start, &[]), None);
    // 閉じ始めた後の入力は進めない
    assert_eq!(run(&mut s, &[(t, In::Esc)], held), want);
    assert_eq!(s.timer(2), Some(t - 1902));
}

/// APP-RES-04: タイマー 1 の後の鍵盤のタイマー（(時刻, 操作, 押した)）→ (100+n, 120+n)
#[rstest]
#[case::before_input(vec![(300, 2, true)], 1, (None, None))]
#[case::press(vec![(301, 2, true), (350, 9, false)], 1, (Some(49), None))]
#[case::release(vec![(301, 2, true), (400, 2, false), (450, 9, false)], 1, (None, Some(50)))]
#[case::scratch(vec![(301, SCRATCH_UP, true)], 0, (Some(0), None))]
fn app_res_04_key_timers(
    #[case] frames: Vec<(i64, usize, bool)>,
    #[case] n: i32,
    #[case] want: (Option<i64>, Option<i64>),
) {
    let mut s = ResultSkin::new(timing(300, 0, 0, 0), &[], (0, 0), res());
    for (t, slot, down) in frames {
        s.frame(
            t,
            ResultInput {
                slots: &[(slot, down)],
                ..Default::default()
            },
        );
    }
    assert_eq!((s.timer(100 + n), s.timer(120 + n)), want);
}

/// APP-RES-05: 条件（EX 9 / 総ノーツ 7 = ランク 5 (A)、保存前 EX 10 = ランク 6 (AA)、BP 1 < 保存前 3）
#[rstest]
#[case::base(res(), vec![(32, true), (33, false), (42, true), (43, false), (90, false), (91, true), (160, true), (161, false), (303, true), (302, false), (308, false), (322, true), (321, false), (330, false), (331, false), (332, true), (335, false), (343, true), (342, false), (350, true), (351, false), (901, true)])]
#[case::cleared_hard({ let mut r = res(); r.lamp = ClearLamp::Hard; r.groove = false; r.key_mode = KeyMode::K5; r }, vec![(90, true), (91, false), (42, false), (43, true), (160, false), (161, true)])]
#[case::aaa_updated({ let mut r = res(); r.counts.pgreat = 7; r.counts.great = 0; r.max_combo = 7; r }, vec![(300, true), (303, false), (340, true), (330, true), (331, true), (335, true)])]
#[case::f_zero({ let mut r = res(); r.counts = JudgeCounts::default(); r }, vec![(308, true), (307, false), (347, true), (306, false)])]
#[case::e({ let mut r = res(); r.counts = JudgeCounts { great: 1, ..Default::default() }; r }, vec![(307, true), (308, false), (347, true)])]
#[case::no_record({ let mut r = res(); r.saved = None; r }, vec![(320, false), (321, false), (322, false), (327, false), (330, true), (331, true), (332, false), (335, false)])]
#[case::old_f({ let mut r = res(); r.saved.as_mut().unwrap().before = Some(rec(0, 0, None)); r }, vec![(327, true), (335, true), (332, false)])]
fn app_res_05_options(#[case] r: PlayResult, #[case] want: Vec<(i32, bool)>) {
    let s = ResultSkin::new(timing(0, 0, 0, 0), &[901], (0, 0), r);
    for (id, w) in want {
        assert_eq!(s.option(id), w, "option {id}");
    }
}

/// APP-RES-06: 数値
#[rstest]
#[case::base(res(), vec![(10, Some(250)), (12, Some(-12)), (14, Some(30)), (100, Some(135714)), (101, Some(9)), (102, Some(64)), (103, Some(28)), (104, Some(1)), (105, Some(3)), (106, Some(7)), (107, Some(46)), (110, Some(4)), (111, Some(1)), (112, Some(1)), (113, Some(1)), (114, Some(0)), (115, Some(64)), (116, Some(28)), (170, Some(10)), (171, Some(9)), (172, Some(-1)), (173, Some(5)), (174, Some(3)), (175, Some(-2)), (176, Some(3)), (177, Some(1)), (178, Some(-2)), (183, Some(71)), (184, Some(42)), (999, None)])]
#[case::five_keys({ let mut r = res(); r.key_mode = KeyMode::K5; r }, vec![(100, Some(67850))])]
#[case::no_record({ let mut r = res(); r.saved = None; r }, vec![(170, Some(0)), (172, Some(9)), (173, Some(0)), (176, Some(0)), (178, Some(1)), (183, Some(0))])]
#[case::unprocessed_bp({ let mut r = res(); r.processed = 5; r }, vec![(177, Some(3)), (102, Some(90))])]
#[case::no_notes({ let mut r = res(); r.total = 0; r.processed = 0; r.counts = JudgeCounts::default(); r }, vec![(100, None), (102, None), (115, None), (116, None), (183, Some(0)), (184, Some(0))])]
fn app_res_06_numbers(#[case] r: PlayResult, #[case] want: Vec<(i32, Option<i64>)>) {
    let s = ResultSkin::new(timing(0, 0, 0, 0), &[], (0, 0), r);
    for (id, w) in want {
        assert_eq!(s.number(id), w, "number {id}");
    }
}

/// APP-RES-07: BARGRAPH
#[rstest]
#[case::base(res(), vec![(10, 9.0 / 14.0), (27, 9.0 / 14.0), (11, 9.0 / 14.0), (20, 4.0 / 7.0), (21, 1.0 / 7.0), (22, 1.0 / 7.0), (23, 1.0 / 7.0), (24, 0.0), (25, 3.0 / 7.0), (26, 1.0), (1, 0.0)])]
#[case::projected({ let mut r = res(); r.processed = 5; r }, vec![(11, 12.0 / 14.0)])]
#[case::projected_capped({ let mut r = res(); r.processed = 3; r }, vec![(11, 1.0)])]
#[case::low_score_five_keys({ let mut r = res(); r.key_mode = KeyMode::K5; r.counts = JudgeCounts { good: 1, ..Default::default() }; r }, vec![(26, 7142.0 / 10000.0)])]
#[case::no_notes({ let mut r = res(); r.total = 0; r.processed = 0; r }, vec![(10, 0.0), (11, 0.0), (20, 0.0), (26, 0.0)])]
fn app_res_07_bargraph(#[case] r: PlayResult, #[case] want: Vec<(i32, f64)>) {
    let s = ResultSkin::new(timing(0, 0, 0, 0), &[], (0, 0), r);
    for (id, w) in want {
        assert!(
            (s.bar(id) - w).abs() < 1e-9,
            "bar {id}: {} != {w}",
            s.bar(id)
        );
    }
}

/// APP-RES-08: 文字列 10..15（プレイの材料のまま）
#[rstest]
#[case::title(10, Some("T"))]
#[case::none(11, None)]
fn app_res_08_texts(#[case] id: i32, #[case] want: Option<&str>) {
    let s = ResultSkin::new(timing(0, 0, 0, 0), &[], (0, 0), res());
    assert_eq!(s.text(id), want);
}

/// APP-RES-09: プレイを閉じた後の行き先
#[rstest]
#[case::auto(true, 7, JudgeCounts { pgreat: 1, ..Default::default() }, AfterPlay::Select)]
#[case::nothing_processed(false, 0, JudgeCounts::default(), AfterPlay::Select)]
#[case::good(false, 3, JudgeCounts { good: 1, poor: 2, ..Default::default() }, AfterPlay::Result)]
#[case::pgreat(false, 1, JudgeCounts { pgreat: 1, ..Default::default() }, AfterPlay::Result)]
#[case::bad_poor_only(false, 3, JudgeCounts { bad: 1, poor: 2, ..Default::default() }, AfterPlay::Select)]
fn app_res_09_after_play(
    #[case] auto: bool,
    #[case] processed: u32,
    #[case] counts: JudgeCounts,
    #[case] want: AfterPlay,
) {
    assert_eq!(after_play(auto, processed, &counts), want);
}
