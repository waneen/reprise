//! 判定・ゲージのテスト（DOM-JDG-*, DOM-LR2-*, DOM-BRJ-*）
use super::gauge::Inner;
use super::*;
use crate::chart::ChartMeta;
use rstest::rstest;

fn meta(rank: Option<i32>, percent: Option<f64>) -> ChartMeta {
    ChartMeta {
        judge_rank: rank,
        judge_rank_percent: percent,
        ..Default::default()
    }
}

fn total_meta(total: Option<f64>, percent: Option<f64>) -> ChartMeta {
    ChartMeta {
        total,
        total_percent: percent,
        ..Default::default()
    }
}

fn sym(ms: [i64; 4]) -> [Window; 4] {
    ms.map(|m| Window::ms(m, m))
}

/// 早/遅が非対称な幅（ms）: PG 1/1, GR 2/2, GD 3/3, BD 早 4・遅 5, 空POOR 早 10・遅 2
fn asym() -> JudgeWindows {
    JudgeWindows {
        grades: [
            Window::ms(1, 1),
            Window::ms(2, 2),
            Window::ms(3, 3),
            Window::ms(4, 5),
        ],
        empty_poor: Window::ms(10, 2),
    }
}

/// DOM-JDG-01: 最初に入った幅（両端含む）、無ければ空POOR、それも外なら作用しない
#[rstest]
#[case::exact(0, Some(Judge::PGreat))]
#[case::pg_edge_late(1_000, Some(Judge::PGreat))]
#[case::pg_edge_early(-1_000, Some(Judge::PGreat))]
#[case::gr(1_001, Some(Judge::Great))]
#[case::gd_early(-3_000, Some(Judge::Good))]
#[case::bd_late_edge(5_000, Some(Judge::Bad))]
#[case::bd_early_edge(-4_000, Some(Judge::Bad))]
#[case::empty_poor_early(-4_001, Some(Judge::EmptyPoor))]
#[case::empty_poor_early_edge(-10_000, Some(Judge::EmptyPoor))]
#[case::too_early(-10_001, None)]
#[case::too_late(5_001, None)]
fn dom_jdg_01_judge_by_window(#[case] delta_us: i64, #[case] want: Option<Judge>) {
    assert_eq!(asym().judge(delta_us), want);
}

/// DOM-JDG-02: 経過が BD の遅い側を超えたら見逃し（ちょうどはまだ）
#[rstest]
#[case::at_edge(5_000, false)]
#[case::past_edge(5_001, true)]
#[case::before_note(-1_000, false)]
fn dom_jdg_02_miss(#[case] elapsed_us: i64, #[case] want: bool) {
    assert_eq!(asym().is_missed(elapsed_us), want);
}

/// DOM-LR2-01: `#RANK` ごとの PG/GR/GD、BD ±200ms、空POOR は早 1000ms 未満。皿も同じ、bmson は既定
#[rstest]
#[case::very_hard(meta(Some(0), None), JudgeLane::Key, [8, 24, 40, 200])]
#[case::hard(meta(Some(1), None), JudgeLane::Key, [15, 30, 60, 200])]
#[case::normal(meta(Some(2), None), JudgeLane::Key, [18, 40, 100, 200])]
#[case::easy(meta(Some(3), None), JudgeLane::Key, [21, 60, 120, 200])]
#[case::rank4_is_normal(meta(Some(4), None), JudgeLane::Key, [18, 40, 100, 200])]
#[case::missing_is_normal(meta(None, None), JudgeLane::Key, [18, 40, 100, 200])]
#[case::bmson_is_normal(meta(Some(0), Some(100.0)), JudgeLane::Key, [18, 40, 100, 200])]
#[case::scratch_same(meta(Some(3), None), JudgeLane::Scratch, [21, 60, 120, 200])]
fn dom_lr2_01_judge_windows(#[case] m: ChartMeta, #[case] lane: JudgeLane, #[case] ms: [i64; 4]) {
    let want = JudgeWindows {
        grades: sym(ms),
        empty_poor: Window {
            early: 999_999,
            late: 0,
        },
    };
    assert_eq!(Lr2Ruleset.judge_windows(&m, lane), want);
}

/// DOM-LR2-02: TOTAL は整数に切り捨て。1 未満・無しなら既定式 × 0.8 を切り捨て
#[rstest]
#[case::given(Some(300.0), 1000, 300)]
#[case::truncated(Some(250.7), 1000, 250)]
#[case::missing_under_400(None, 100, 176)]
#[case::missing_399(None, 399, 223)]
#[case::missing_under_600(None, 500, 256)]
#[case::missing_over_600(None, 1000, 352)]
#[case::below_one(Some(0.5), 100, 176)]
#[case::negative(Some(-10.0), 100, 176)]
fn dom_lr2_02_total(#[case] total: Option<f64>, #[case] notes: usize, #[case] want: i64) {
    assert_eq!(lr2::effective_total(total, notes), want);
}

/// DOM-LR2-03: ダメージ係数 = max(ノーツ数の段階 × 10, 100 / 回復値) / 10。回復値 = ⌊(TOTAL − 80) × 0.125 / 2⌋（0 以下は 1）
#[rstest]
#[case::few_notes(300, 10, 10.0)]
#[case::notes_25(300, 25, 9.0)]
#[case::notes_750(300, 750, 1.5)]
#[case::many_notes(300, 1000, 1.0)]
#[case::low_total_wins(200, 1000, 100.0 / 7.0 / 10.0)]
#[case::recover_floor_one(80, 1000, 10.0)]
fn dom_lr2_03_damage_factor(#[case] total: i64, #[case] notes: usize, #[case] want: f64) {
    assert!((lr2::damage_factor(total, notes) - want).abs() < 1e-9);
}

fn lr2_gauge(ty: GaugeType) -> Option<Gauge> {
    // TOTAL 300 / 750 ノーツ: 1 ノーツ 0.4、ダメージ係数 1.5
    Lr2Ruleset.gauge(ty, &total_meta(Some(300.0), None), 750)
}

/// DOM-LR2-04: 初期値（GROOVE/EASY 20、他 100）と判定ごとの増減（PG, GR, GD, BD, PR, 空POOR）
#[rstest]
#[case::groove(GaugeType::Groove, Some((20.0, [0.4, 0.4, 0.2, -4.0, -6.0, -2.0])))]
#[case::hard(GaugeType::Hard, Some((100.0, [0.1, 0.1, 0.05, -9.0, -15.0, -3.0])))]
#[case::death(GaugeType::Hazard, Some((100.0, [0.0, 0.0, 0.0, -100.0, -100.0, 0.0])))]
#[case::easy(GaugeType::Easy, Some((20.0, [0.48, 0.48, 0.24, -3.2, -4.8, -1.6])))]
#[case::p_attack(GaugeType::PAttack, Some((100.0, [0.1, -1.0, -100.0, -100.0, -100.0, -100.0])))]
#[case::g_attack(GaugeType::GAttack, Some((100.0, [-15.0, -1.0, 0.1, -6.0, -15.0, -3.0])))]
#[case::no_assist_easy(GaugeType::AssistEasy, None)]
#[case::no_ex_hard(GaugeType::ExHard, None)]
fn dom_lr2_04_gauge_table(#[case] ty: GaugeType, #[case] want: Option<(f64, [f64; 6])>) {
    let got = lr2_gauge(ty).map(|g| match g.0 {
        Inner::Lr2(g) => (g.hp, g.delta),
        _ => unreachable!(),
    });
    match (got, want) {
        (Some((hp, d)), Some((whp, wd))) => {
            assert_eq!(hp, whp);
            for (a, b) in d.iter().zip(wd) {
                assert!((a - b).abs() < 1e-6, "{d:?} != {wd:?}");
            }
        }
        (None, None) => {}
        (g, w) => panic!("{g:?} != {w:?}"),
    }
}

/// DOM-LR2-05: 更新。更新前の値を偶数に切り捨てて 30 以下なら GROOVE/EASY 以外の BD/PR/空POOR は ×0.6。
/// GROOVE/EASY は 2 で止まる。0..=100 に収め、更新前の切り捨て値が 0 以下なら 0
#[rstest]
#[case::groove_bad(GaugeType::Groove, 50.0, Judge::Bad, 46.0)]
#[case::groove_low_not_reduced(GaugeType::Groove, 20.0, Judge::Bad, 16.0)]
#[case::groove_floor_2(GaugeType::Groove, 3.0, Judge::Poor, 2.0)]
#[case::groove_cap_100(GaugeType::Groove, 99.9, Judge::PGreat, 100.0)]
#[case::hard_bad(GaugeType::Hard, 50.0, Judge::Bad, 41.0)]
#[case::hard_31_is_30_reduced(GaugeType::Hard, 31.0, Judge::Bad, 25.6)]
#[case::hard_32_not_reduced(GaugeType::Hard, 32.5, Judge::Bad, 23.5)]
#[case::hard_low_gain_not_reduced(GaugeType::Hard, 30.0, Judge::PGreat, 30.1)]
#[case::hard_to_zero(GaugeType::Hard, 5.0, Judge::Poor, 0.0)]
#[case::hard_below_2_dies(GaugeType::Hard, 1.5, Judge::PGreat, 0.0)]
#[case::hard_dead_stays(GaugeType::Hard, 0.0, Judge::PGreat, 0.0)]
fn dom_lr2_05_gauge_update(
    #[case] ty: GaugeType,
    #[case] start: f64,
    #[case] j: Judge,
    #[case] want: f64,
) {
    let mut g = lr2_gauge(ty).unwrap();
    if let Inner::Lr2(x) = &mut g.0 {
        x.hp = start;
    }
    g.apply(j);
    assert!((g.value() - want).abs() < 1e-9, "{} != {want}", g.value());
}

/// DOM-LR2-06: クリア: HARD・G-ATTACK・P-ATTACK は 2 以上、それ以外は 80 以上
#[rstest]
#[case::hard_2(GaugeType::Hard, 2.0, true)]
#[case::hard_under_2(GaugeType::Hard, 1.9, false)]
#[case::g_attack_2(GaugeType::GAttack, 2.0, true)]
#[case::p_attack_2(GaugeType::PAttack, 2.0, true)]
#[case::groove_80(GaugeType::Groove, 80.0, true)]
#[case::groove_under_80(GaugeType::Groove, 79.9, false)]
#[case::easy_80(GaugeType::Easy, 80.0, true)]
#[case::death_needs_80(GaugeType::Hazard, 79.0, false)]
fn dom_lr2_06_clear(#[case] ty: GaugeType, #[case] hp: f64, #[case] want: bool) {
    let mut g = lr2_gauge(ty).unwrap();
    if let Inner::Lr2(x) = &mut g.0 {
        x.hp = hp;
    }
    assert_eq!(g.qualified(), want);
}

/// DOM-BRJ-01: 判定倍率。`#RANK` 0..=4 は 25/50/75/100/125%、範囲外・無しは 75%。
/// bmson は 0 未満 75%、5 未満は `#RANK`、それ以外はそのまま %（`#RANK` より優先）
#[rstest]
#[case::very_hard(meta(Some(0), None), 25)]
#[case::hard(meta(Some(1), None), 50)]
#[case::normal(meta(Some(2), None), 75)]
#[case::easy(meta(Some(3), None), 100)]
#[case::very_easy(meta(Some(4), None), 125)]
#[case::out_of_range(meta(Some(5), None), 75)]
#[case::negative(meta(Some(-1), None), 75)]
#[case::missing(meta(None, None), 75)]
#[case::bmson_100(meta(None, Some(100.0)), 100)]
#[case::bmson_200(meta(None, Some(200.0)), 200)]
#[case::bmson_small_is_rank(meta(None, Some(3.0)), 100)]
#[case::bmson_negative(meta(None, Some(-1.0)), 75)]
#[case::bmson_over_rank(meta(Some(0), Some(150.0)), 150)]
fn dom_brj_01_judge_rate(#[case] m: ChartMeta, #[case] want: i64) {
    assert_eq!(beatoraja::judge_rate(&m), want);
}

/// DOM-BRJ-02: 幅 = 基準 × 倍率（PG..BD、µs 切り捨て）。空POOR は固定。皿は別の基準
#[rstest]
#[case::key_100(3, JudgeLane::Key, [(20_000, 20_000), (60_000, 60_000), (150_000, 150_000), (220_000, 280_000)], (500_000, 150_000))]
#[case::key_75(2, JudgeLane::Key, [(15_000, 15_000), (45_000, 45_000), (112_500, 112_500), (165_000, 210_000)], (500_000, 150_000))]
#[case::key_25(0, JudgeLane::Key, [(5_000, 5_000), (15_000, 15_000), (37_500, 37_500), (55_000, 70_000)], (500_000, 150_000))]
#[case::scratch_100(3, JudgeLane::Scratch, [(30_000, 30_000), (70_000, 70_000), (160_000, 160_000), (230_000, 290_000)], (500_000, 160_000))]
fn dom_brj_02_judge_windows(
    #[case] rank: i32,
    #[case] lane: JudgeLane,
    #[case] grades: [(i64, i64); 4],
    #[case] ep: (i64, i64),
) {
    let w = |(early, late): (i64, i64)| Window { early, late };
    let want = JudgeWindows {
        grades: grades.map(w),
        empty_poor: w(ep),
    };
    assert_eq!(
        BeatorajaRuleset.judge_windows(&meta(Some(rank), None), lane),
        want
    );
}

/// DOM-BRJ-03: TOTAL。`#TOTAL` が正ならそのまま、bmson の `total` が正なら % × 既定値（優先）、
/// それ以外は既定値 max(260, 7.605n / (0.01n + 6.5))
#[rstest]
#[case::given(Some(300.0), None, 1000, 300.0)]
#[case::default_floor(None, None, 100, 260.0)]
#[case::default_formula(None, None, 1000, 7605.0 / 16.5)]
#[case::zero_is_default(Some(0.0), None, 100, 260.0)]
#[case::negative_is_default(Some(-5.0), None, 100, 260.0)]
#[case::bmson_percent(None, Some(50.0), 1000, 0.5 * 7605.0 / 16.5)]
#[case::bmson_over_total(Some(300.0), Some(120.0), 100, 312.0)]
#[case::bmson_zero_is_default(None, Some(0.0), 100, 260.0)]
fn dom_brj_03_total(
    #[case] total: Option<f64>,
    #[case] percent: Option<f64>,
    #[case] notes: usize,
    #[case] want: f64,
) {
    let got = beatoraja::effective_total(&total_meta(total, percent), notes);
    assert!((got - want).abs() < 1e-9, "{got} != {want}");
}

fn brj_gauge(ty: GaugeType, total: f64) -> Option<Gauge> {
    BeatorajaRuleset.gauge(ty, &total_meta(Some(total), None), 1000)
}

/// DOM-BRJ-04: 初期値・ボーダーと補正後の増減（PG, GR, GD, BD, PR, 空POOR）。
/// EASY 系は増加 × TOTAL / ノーツ数、HARD/EX-HARD は増加 × min(0.15, (2·TOTAL − 320) / ノーツ数) / 0.15
#[rstest]
#[case::assist_easy(GaugeType::AssistEasy, 300.0, Some((20.0, 60.0, [0.3, 0.3, 0.15, -1.5, -3.0, -0.5])))]
#[case::easy(GaugeType::Easy, 300.0, Some((20.0, 80.0, [0.3, 0.3, 0.15, -1.5, -4.5, -1.0])))]
#[case::groove(GaugeType::Groove, 300.0, Some((20.0, 80.0, [0.3, 0.3, 0.15, -3.0, -6.0, -2.0])))]
#[case::hard(GaugeType::Hard, 300.0, Some((100.0, 0.0, [0.15, 0.12, 0.03, -5.0, -10.0, -5.0])))]
#[case::hard_low_total(GaugeType::Hard, 200.0, Some((100.0, 0.0, [0.08, 0.064, 0.016, -5.0, -10.0, -5.0])))]
#[case::ex_hard(GaugeType::ExHard, 300.0, Some((100.0, 0.0, [0.15, 0.06, 0.0, -8.0, -16.0, -8.0])))]
#[case::hazard(GaugeType::Hazard, 300.0, Some((100.0, 0.0, [0.15, 0.06, 0.0, -100.0, -100.0, -10.0])))]
#[case::no_p_attack(GaugeType::PAttack, 300.0, None)]
#[case::no_g_attack(GaugeType::GAttack, 300.0, None)]
fn dom_brj_04_gauge_table(
    #[case] ty: GaugeType,
    #[case] total: f64,
    #[case] want: Option<(f32, f32, [f32; 6])>,
) {
    let got = brj_gauge(ty, total).map(|g| match g.0 {
        Inner::Beatoraja(g) => (g.value, g.border, g.delta),
        _ => unreachable!(),
    });
    match (got, want) {
        (Some((v, b, d)), Some((wv, wb, wd))) => {
            assert_eq!((v, b), (wv, wb));
            for (a, b) in d.iter().zip(wd) {
                assert!((a - b).abs() < 1e-6, "{d:?} != {wd:?}");
            }
        }
        (None, None) => {}
        (g, w) => panic!("{g:?} != {w:?}"),
    }
}

/// DOM-BRJ-05: 更新。減少時は「値 < 閾値」の最初の行の倍率（HARD: 10/20/30/40/50 未満で ×0.4..0.8）。
/// 最小（EASY 系 2、他 0）..100 に収め、0 になったら以後動かない
#[rstest]
#[case::groove_pg(GaugeType::Groove, 50.0, Judge::PGreat, 50.3)]
#[case::groove_floor_2(GaugeType::Groove, 3.0, Judge::Poor, 2.0)]
#[case::groove_cap_100(GaugeType::Groove, 99.9, Judge::PGreat, 100.0)]
#[case::hard_guts_under_50(GaugeType::Hard, 45.0, Judge::Bad, 41.0)]
#[case::hard_guts_under_10(GaugeType::Hard, 5.0, Judge::Bad, 3.0)]
#[case::hard_at_50_no_guts(GaugeType::Hard, 50.0, Judge::Bad, 45.0)]
#[case::hard_high_no_guts(GaugeType::Hard, 60.0, Judge::Bad, 55.0)]
#[case::hard_to_zero(GaugeType::Hard, 3.0, Judge::Poor, 0.0)]
#[case::hard_dead_stays(GaugeType::Hard, 0.0, Judge::PGreat, 0.0)]
#[case::hazard_bad(GaugeType::Hazard, 100.0, Judge::Bad, 0.0)]
fn dom_brj_05_gauge_update(
    #[case] ty: GaugeType,
    #[case] start: f32,
    #[case] j: Judge,
    #[case] want: f64,
) {
    let mut g = brj_gauge(ty, 300.0).unwrap();
    if let Inner::Beatoraja(x) = &mut g.0 {
        x.value = start;
    }
    g.apply(j);
    assert!((g.value() - want).abs() < 1e-4, "{} != {want}", g.value());
}

/// DOM-BRJ-06: クリア: 値が 0 より大きくボーダー以上
#[rstest]
#[case::groove_80(GaugeType::Groove, 80.0, true)]
#[case::groove_under_80(GaugeType::Groove, 79.9, false)]
#[case::assist_easy_60(GaugeType::AssistEasy, 60.0, true)]
#[case::assist_easy_under_60(GaugeType::AssistEasy, 59.0, false)]
#[case::hard_alive(GaugeType::Hard, 0.1, true)]
#[case::hard_dead(GaugeType::Hard, 0.0, false)]
#[case::ex_hard_dead(GaugeType::ExHard, 0.0, false)]
fn dom_brj_06_clear(#[case] ty: GaugeType, #[case] value: f32, #[case] want: bool) {
    let mut g = brj_gauge(ty, 300.0).unwrap();
    if let Inner::Beatoraja(x) = &mut g.0 {
        x.value = value;
    }
    assert_eq!(g.qualified(), want);
}
