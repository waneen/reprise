//! DOM-SKD-* のテスト（ルール表は lib.rs）
use super::lr2::*;
use super::*;
use rstest::rstest;
use std::collections::BTreeMap;

#[derive(Default)]
struct St {
    timers: BTreeMap<i32, i64>,
    ops: Vec<i32>,
    numbers: BTreeMap<i32, i64>,
    bar: f64,
    slider: f64,
    angle: f32,
    texts: BTreeMap<i32, String>,
}
impl SkinState for St {
    fn timer(&self, id: i32) -> Option<i64> {
        self.timers.get(&id).copied()
    }
    fn option(&self, id: i32) -> bool {
        self.ops.contains(&id)
    }
    fn number(&self, id: i32) -> Option<i64> {
        self.numbers.get(&id).copied()
    }
    fn bar(&self, _: i32) -> f64 {
        self.bar
    }
    fn slider(&self, _: i32) -> f64 {
        self.slider
    }
    fn turntable(&self, side: i32) -> f32 {
        self.angle * side as f32
    }
    fn text(&self, id: i32) -> Option<&str> {
        self.texts.get(&id).map(String::as_str)
    }
}
fn st(timers: &[(i32, i64)]) -> St {
    St {
        timers: timers.iter().copied().collect(),
        ..Default::default()
    }
}

fn kf(t: i32, dst: [f32; 4], a: u8, accel: i32) -> Keyframe {
    Keyframe {
        t_ms: t,
        dst,
        color: [a, 255, 255, 255],
        angle: 0.0,
        center: 0,
        blend: Blend::Alpha,
        accel,
        filter: false,
    }
}
fn src(rect: [i32; 4], divide: (u32, u32), cycle: i32, timer: i32) -> SourceRect {
    SourceRect {
        image: 0,
        rect,
        divide,
        cycle_ms: cycle,
        timer: TimerId(timer),
        params: [0; 5],
    }
}
fn el(kind: ElementKind, s: SourceRect, timer: i32, kfs: Vec<Keyframe>) -> Element {
    Element {
        kind,
        source: Some(s),
        timer: TimerId(timer),
        conditions: vec![],
        looping: Some(0),
        dst_extra: [0; 2],
        order: 0,
        keyframes: kfs,
    }
}
fn other(command: &str, index: i32) -> ElementKind {
    ElementKind::Other {
        command: command.into(),
        index,
    }
}
fn skin(elements: Vec<Element>) -> Skin {
    Skin {
        scene: SceneKind::Play7,
        resolution: (640, 480),
        images: vec![],
        elements,
        options: vec![],
        color_key: None,
        timing: Default::default(),
        fonts: vec![],
        image_fonts: vec![],
    }
}
fn size(_: u32) -> Option<(u32, u32)> {
    Some((256, 128))
}
/// 四角形だけを取り出す
fn quads(
    skin: &Skin,
    st: &dyn SkinState,
    view: &PlayView,
    size: &dyn Fn(u32) -> Option<(u32, u32)>,
) -> Vec<Quad> {
    draw_play(skin, st, view, size)
        .into_iter()
        .filter_map(|d| match d {
            Draw::Quad(q) => Some(q),
            Draw::Text(_) => None,
        })
        .collect()
}
fn dsts(q: &[Quad]) -> Vec<[f32; 4]> {
    q.iter().map(|q| q.dst).collect()
}
fn srcs(q: &[Quad]) -> Vec<[f32; 4]> {
    q.iter().map(|q| q.src).collect()
}

/// DOM-SKD-01: 条件は 0 が真、正は状態、負は否定。1000 以上は偽（否定なら真）
#[rstest]
#[case::zero(0, true)]
#[case::set(32, true)]
#[case::unset(33, false)]
#[case::negated_set(-32, false)]
#[case::negated_unset(-33, true)]
#[case::over_999(1032, false)]
#[case::negated_over_999(-1032, true)]
fn dom_skd_01_conditions(#[case] id: i32, #[case] want: bool) {
    let s = St {
        ops: vec![32, 1032],
        ..Default::default()
    };
    assert_eq!(op_true(&s, id), want);
}

/// DOM-SKD-02: 引く時刻。始端 ≤ 終端・t ≥ 始端・（ループ ≥ 0 か t ≤ 終端）のときだけ描く。
/// 始端 = 終端 かループ = 終端なら終端で止め、ループ < 終端なら [ループ, 終端) で繰り返し、ループ > 終端なら 0。タイマー停止は −1
#[rstest]
#[case::before_start(&[100, 200], 0, 50, None)]
#[case::timer_off(&[0, 200], 0, -1, None)]
#[case::timer_off_negative_start(&[-1, 200], 0, -1, Some(-1))]
#[case::inside(&[0, 200], 0, 150, Some(150))]
#[case::no_loop_hides_after_end(&[0, 200], -1, 201, None)]
#[case::no_loop_at_end(&[0, 200], -1, 200, Some(200))]
#[case::single_holds(&[100], 0, 5000, Some(100))]
#[case::loop_equal_end_holds(&[0, 200], 200, 5000, Some(200))]
#[case::loop_wraps(&[0, 200], 100, 250, Some(150))]
#[case::loop_wraps_from_zero(&[0, 200], 0, 450, Some(50))]
#[case::loop_after_end_is_zero(&[0, 200], 300, 250, Some(0))]
fn dom_skd_02_effective_time(
    #[case] times: &[i32],
    #[case] looping: i32,
    #[case] t: i64,
    #[case] want: Option<i64>,
) {
    let kfs: Vec<Keyframe> = times.iter().map(|&t| kf(t, [0.0; 4], 255, 0)).collect();
    assert_eq!(effective_time(&kfs, looping, t), want);
}

/// DOM-SKD-03: 時刻以下で最後のキーフレームから次へ、その加速で補間（0 = 線形、1 = r³、2 = 1−(1−r)³、他 = 止める）
#[rstest]
#[case::linear(0, 50, 50.0)]
#[case::ease_in(1, 50, 12.5)]
#[case::ease_out(2, 50, 87.5)]
#[case::step(3, 50, 0.0)]
#[case::at_keyframe(0, 100, 100.0)]
fn dom_skd_03_interpolate(#[case] accel: i32, #[case] t: i64, #[case] want_x: f32) {
    let kfs = vec![
        kf(0, [0.0, 0.0, 10.0, 10.0], 0, accel),
        kf(100, [100.0, 0.0, 10.0, 10.0], 200, 0),
    ];
    let p = interpolate(&kfs, t);
    assert_eq!(p.dst[0], want_x);
    let want_a = want_x * 2.0;
    assert_eq!(p.argb[0], want_a);
}

/// DOM-SKD-04: コマ = (経過 % 周期) × コマ数 / 周期（経過 < 0・周期 ≤ 0 は 0）。コマは左上から行ごと、大きさは 幅/分割 の切り捨て。
/// 幅・高さが負なら画像全体。SRC と DST のタイマーが同じなら経過から最初のキーフレームの時刻を引く
#[rstest]
#[case::static_(src([0, 0, 40, 20], (2, 2), 0, 0), &[(0, 500)], 1000, [0.0, 0.0, 20.0, 10.0])]
#[case::frame_1(src([0, 0, 40, 20], (2, 2), 400, 10), &[(10, 100)], 1000, [20.0, 0.0, 20.0, 10.0])]
#[case::frame_2_next_row(src([0, 0, 40, 20], (2, 2), 400, 10), &[(10, 250)], 1000, [0.0, 10.0, 20.0, 10.0])]
#[case::wraps(src([0, 0, 40, 20], (2, 2), 400, 10), &[(10, 700)], 1000, [20.0, 10.0, 20.0, 10.0])]
#[case::timer_off(src([0, 0, 40, 20], (2, 2), 400, 10), &[], 1000, [0.0, 0.0, 20.0, 10.0])]
#[case::truncated_cells(src([5, 6, 41, 21], (2, 2), 0, 0), &[(0, 0)], 1000, [5.0, 6.0, 20.0, 10.0])]
#[case::whole_image(src([0, 0, -1, -1], (1, 2), 0, 0), &[(0, 0)], 1000, [0.0, 0.0, 256.0, 64.0])]
#[case::same_timer_minus_first_key(src([0, 0, 40, 20], (2, 2), 400, 41), &[(41, 1150)], 1000, [20.0, 0.0, 20.0, 10.0])]
fn dom_skd_04_source_frame(
    #[case] s: SourceRect,
    #[case] timers: &[(i32, i64)],
    #[case] first_key: i32,
    #[case] want: [f32; 4],
) {
    let mut timers = timers.to_vec();
    timers.push((
        41,
        *timers
            .iter()
            .find(|t| t.0 == 41)
            .map(|t| &t.1)
            .unwrap_or(&1000),
    ));
    let e = el(
        ElementKind::Image,
        s,
        41,
        vec![
            kf(first_key, [0.0, 0.0, 1.0, 1.0], 255, 0),
            kf(first_key + 1000, [0.0, 0.0, 1.0, 1.0], 255, 0),
        ],
    );
    let q = quads(&skin(vec![e]), &st(&timers), &PlayView::default(), &size);
    assert_eq!(srcs(&q), vec![want]);
}

/// DOM-SKD-05: α < 2 で合成ありなら描かない。負の幅・高さは位置をずらして正にする
#[rstest]
#[case::transparent(1.0, Blend::Alpha, [10.0, 10.0, 5.0, 5.0], None)]
#[case::transparent_no_blend(1.0, Blend::None, [10.0, 10.0, 5.0, 5.0], Some([10.0, 10.0, 5.0, 5.0]))]
#[case::negative_size(255.0, Blend::Alpha, [10.0, 10.0, -5.0, -4.0], Some([5.0, 6.0, 5.0, 4.0]))]
fn dom_skd_05_quad(
    #[case] a: f32,
    #[case] blend: Blend,
    #[case] dst: [f32; 4],
    #[case] want: Option<[f32; 4]>,
) {
    let pose = Pose {
        dst,
        argb: [a, 255.0, 255.0, 255.0],
        angle: 0.0,
        center: 0,
        blend,
        filter: false,
    };
    assert_eq!(quad(0, [0.0; 4], &pose).map(|q| q.dst), want);
}

/// DOM-SKD-06: 数字。コマ数 10 の倍数 = 0-9（空白は描かない）、11 の倍数 = 10 番が空白、24 の倍数 = 符号付き（11 = +、23 = −、負の数字は +12）。
/// 揃え 0 = 右（下位桁を残す）、1 = 左、2 = 中央（(桁 − 数字の桁数)/2 だけ右へ）。負の値は 24 の倍数だけ
#[rstest]
#[case::right(42, 0, 4, 10, Some((0.0, vec![None, None, Some(4), Some(2)])))]
#[case::right_overflow_keeps_low(12345, 0, 3, 10, Some((0.0, vec![Some(3), Some(4), Some(5)])))]
#[case::right_blank_glyph(7, 0, 3, 11, Some((0.0, vec![Some(10), Some(10), Some(7)])))]
#[case::left(42, 1, 4, 11, Some((0.0, vec![Some(4), Some(2), Some(10), Some(10)])))]
#[case::center(42, 2, 4, 10, Some((1.0, vec![Some(4), Some(2), None, None])))]
#[case::signed_plus(5, 0, 2, 24, Some((0.0, vec![Some(11), Some(10), Some(5)])))]
#[case::signed_minus(-5, 0, 2, 24, Some((0.0, vec![Some(23), Some(22), Some(17)])))]
#[case::negative_unsigned_hidden(-5, 0, 2, 10, None)]
#[case::no_digits(5, 0, 0, 10, None)]
#[case::bad_align(5, 3, 2, 10, None)]
#[case::bad_frames(5, 0, 2, 7, None)]
fn dom_skd_06_number_glyphs(
    #[case] value: i64,
    #[case] align: i32,
    #[case] digits: i32,
    #[case] frames: usize,
    #[case] want: Option<(f32, Vec<Option<usize>>)>,
) {
    assert_eq!(number_glyphs(value, align, digits, frames), want);
}

fn img(order: u32, x: f32) -> Element {
    let mut e = el(
        ElementKind::Image,
        src([0, 0, 8, 8], (1, 1), 0, 0),
        0,
        vec![kf(0, [x, 0.0, 8.0, 8.0], 255, 0)],
    );
    e.order = order;
    e
}

/// DOM-SKD-07: IMAGE・NUMBER・BARGRAPH・SLIDER は条件をすべて満たすときだけ描く。DST 引数 21 が 1/2 の IMAGE は皿の角度で回す。
/// BARGRAPH は値（0..=1、0 以下は描かない）で 向き 0 = 幅、1 = 高さ を縮め、SLIDER は 値 × 範囲 だけ 向き 0 上・1 右・2 下・3 左 へずらす。
/// 描画順は描画順キーの小さい順（同じなら並び順）
#[rstest]
#[case::condition_true({ let mut e = img(0, 0.0); e.conditions = vec![OptionId(32)]; vec![e] }, vec![[0.0, 0.0, 8.0, 8.0]], 0.0)]
#[case::condition_false({ let mut e = img(0, 0.0); e.conditions = vec![OptionId(33)]; vec![e] }, vec![], 0.0)]
#[case::turntable({ let mut e = img(0, 0.0); e.dst_extra = [2, 0]; vec![e] }, vec![[0.0, 0.0, 8.0, 8.0]], 60.0)]
#[case::bar_width(vec![el(ElementKind::Bar { value_ref: 1, direction: 0 }, src([0, 0, 8, 8], (1, 1), 0, 0), 0, vec![kf(0, [0.0, 0.0, 100.0, 10.0], 255, 0)])], vec![[0.0, 0.0, 25.0, 10.0]], 0.0)]
#[case::bar_height(vec![el(ElementKind::Bar { value_ref: 1, direction: 1 }, src([0, 0, 8, 8], (1, 1), 0, 0), 0, vec![kf(0, [0.0, 0.0, 100.0, 10.0], 255, 0)])], vec![[0.0, 0.0, 100.0, 2.5]], 0.0)]
#[case::slider_up(vec![el(ElementKind::Slider { value_ref: 6, direction: 0, range: 200 }, src([0, 0, 8, 8], (1, 1), 0, 0), 0, vec![kf(0, [0.0, 300.0, 8.0, 8.0], 255, 0)])], vec![[0.0, 250.0, 8.0, 8.0]], 0.0)]
#[case::slider_right(vec![el(ElementKind::Slider { value_ref: 6, direction: 1, range: 200 }, src([0, 0, 8, 8], (1, 1), 0, 0), 0, vec![kf(0, [0.0, 300.0, 8.0, 8.0], 255, 0)])], vec![[50.0, 300.0, 8.0, 8.0]], 0.0)]
#[case::order(vec![img(5, 1.0), img(2, 2.0), img(5, 3.0)], vec![[2.0, 0.0, 8.0, 8.0], [1.0, 0.0, 8.0, 8.0], [3.0, 0.0, 8.0, 8.0]], 0.0)]
fn dom_skd_07_elements(
    #[case] elements: Vec<Element>,
    #[case] want: Vec<[f32; 4]>,
    #[case] angle: f32,
) {
    let s = St {
        timers: [(0, 0)].into_iter().collect(),
        ops: vec![32],
        bar: 0.25,
        slider: 0.25,
        angle: 30.0,
        ..Default::default()
    };
    let q = quads(&skin(elements), &s, &PlayView::default(), &size);
    assert_eq!(dsts(&q), want);
    assert!(q.iter().all(|q| q.angle == angle));
}

/// DOM-SKD-08: ゲージは 50 区間。GROOVE 系は 1..39 区間目が 点灯 1 / 消灯 3、40 区間目以降が 点灯 0 / 消灯 2。HARD 系はすべて 0 / 2。
/// HP/2 − 乱数(0..=2) 以上 HP/2 未満の区間（1 区間目を除く）はちらついて消灯
#[rstest]
#[case::groove_first_segment(25, 0, false, 1, 1, 1)]
#[case::groove_unlit_low(25, 0, false, 30, 3, 3)]
#[case::groove_lit_high(45, 0, false, 41, 0, 0)]
#[case::groove_unlit_high(45, 0, false, 46, 2, 2)]
#[case::groove_tremble(25, 2, false, 24, 3, 1)]
#[case::survival_lit(25, 0, true, 10, 0, 0)]
#[case::survival_unlit(25, 0, true, 30, 2, 2)]
#[case::survival_tremble(25, 2, true, 24, 2, 0)]
fn dom_skd_08_gauge(
    #[case] hp: i32,
    #[case] tremble: i32,
    #[case] survival: bool,
    #[case] segment: usize,
    #[case] want: usize,
    #[case] want_without_tremble: usize,
) {
    assert_eq!(gauge_frames(hp, tremble, survival)[segment - 1], want);
    assert_eq!(
        gauge_frames(hp, 0, survival)[segment - 1],
        want_without_tremble
    );
}

fn judge_skin(noshift: i32) -> Skin {
    let mut j = el(
        other("NOWJUDGE_1P", 5),
        src([0, 0, 40, 10], (1, 1), 0, 46),
        46,
        vec![kf(0, [100.0, 200.0, 40.0, 10.0], 255, 0)],
    );
    j.source.as_mut().unwrap().params[0] = noshift;
    let mut c = el(
        other("NOWCOMBO_1P", 5),
        src([0, 0, 100, 10], (10, 1), 0, 46),
        46,
        vec![kf(0, [40.0, 0.0, 10.0, 10.0], 255, 0)],
    );
    // 揃え 1（左）、4 桁
    c.source.as_mut().unwrap().params = [0, 1, 4, 0, 0];
    let mut j_gd = j.clone();
    j_gd.kind = other("NOWJUDGE_1P", 2);
    skin(vec![j, c, j_gd])
}

/// DOM-SKD-09: NOWJUDGE は添字 5=PG … 1=POOR, 0=空POOR の 1 つだけを、タイマー 46 で、プレイ開始（タイマー 41 > 0）後に描く。
/// コンボは GD 以上のときだけ、判定の位置からの相対で描く。SRC 引数 1 が 0 なら判定を コンボの桁数 × コンボの幅/2 だけ左へ、
/// そうでなければコンボを (桁数 − 1) × 幅/2 だけ左へずらす
#[rstest]
#[case::shift_judge(0, Some(5), 12, true, vec![[90.0, 200.0, 40.0, 10.0], [130.0, 200.0, 10.0, 10.0], [140.0, 200.0, 10.0, 10.0]])]
#[case::noshift(1, Some(5), 12, true, vec![[100.0, 200.0, 40.0, 10.0], [135.0, 200.0, 10.0, 10.0], [145.0, 200.0, 10.0, 10.0]])]
#[case::bad_hides_combo(0, Some(2), 12, true, vec![[100.0, 200.0, 40.0, 10.0]])]
#[case::no_judge_yet(0, None, 0, true, vec![])]
#[case::before_play(0, Some(5), 12, false, vec![])]
fn dom_skd_09_judge_combo(
    #[case] noshift: i32,
    #[case] judge: Option<usize>,
    #[case] combo: i64,
    #[case] playing: bool,
    #[case] want: Vec<[f32; 4]>,
) {
    let mut timers = vec![(46, 10)];
    if playing {
        timers.push((41, 500));
    }
    let view = PlayView {
        judge,
        combo,
        ..Default::default()
    };
    let q = quads(&judge_skin(noshift), &st(&timers), &view, &size);
    assert_eq!(dsts(&q), want);
}

fn note_skin() -> Skin {
    let mut note = el(
        ElementKind::NoteLane { lane: 1 },
        src([0, 0, 20, 5], (1, 1), 0, 0),
        0,
        vec![kf(0, [50.0, 400.0, 20.0, 5.0], 100, 0)],
    );
    note.order = 10;
    let part = |cmd: &str, y: i32| {
        el(
            other(cmd, 1),
            src([0, y, 20, 5], (2, 1), 100, 70),
            0,
            vec![],
        )
    };
    skin(vec![
        note,
        part("LN_START", 10),
        part("LN_END", 20),
        part("LN_BODY", 30),
        part("MINE", 40),
    ])
}

/// DOM-SKD-10: ノーツは DST_NOTE の位置（判定位置）から 拍数 × 480 × ハイスピード / 600 だけ上に描く（過ぎたものは判定位置に留める）。
/// α は 255。LN は 胴（終端の下端から始端まで）・終端・始端 の順で、押していない LN はコマ 0。プレイ開始前は描かない
#[rstest]
#[case::note(NoteView { lane: 1, head: 1.0, tail: None, mine: false, held: false }, vec![[50.0, 240.0, 20.0, 5.0]], vec![[0.0, 0.0, 20.0, 5.0]])]
#[case::passed_stays(NoteView { lane: 1, head: -0.5, tail: None, mine: false, held: false }, vec![[50.0, 400.0, 20.0, 5.0]], vec![[0.0, 0.0, 20.0, 5.0]])]
#[case::other_lane(NoteView { lane: 2, head: 1.0, tail: None, mine: false, held: false }, vec![], vec![])]
#[case::above_screen(NoteView { lane: 1, head: 3.0, tail: None, mine: false, held: false }, vec![], vec![])]
#[case::mine(NoteView { lane: 1, head: 0.5, tail: None, mine: true, held: false }, vec![[50.0, 320.0, 20.0, 5.0]], vec![[10.0, 40.0, 10.0, 5.0]])]
#[case::ln(NoteView { lane: 1, head: 0.5, tail: Some(1.5), mine: false, held: false },
    vec![[50.0, 165.0, 20.0, 155.0], [50.0, 160.0, 20.0, 5.0], [50.0, 320.0, 20.0, 5.0]],
    vec![[0.0, 30.0, 10.0, 5.0], [0.0, 20.0, 10.0, 5.0], [0.0, 10.0, 10.0, 5.0]])]
#[case::held_ln_animates(NoteView { lane: 1, head: 0.0, tail: Some(1.0), mine: false, held: true },
    vec![[50.0, 245.0, 20.0, 155.0], [50.0, 240.0, 20.0, 5.0], [50.0, 400.0, 20.0, 5.0]],
    vec![[10.0, 30.0, 10.0, 5.0], [10.0, 20.0, 10.0, 5.0], [10.0, 10.0, 10.0, 5.0]])]
fn dom_skd_10_notes(
    #[case] n: NoteView,
    #[case] want_dst: Vec<[f32; 4]>,
    #[case] want_src: Vec<[f32; 4]>,
) {
    let view = PlayView {
        hi_speed: 200.0,
        notes: vec![n],
        ..Default::default()
    };
    let s = st(&[(0, 0), (41, 100), (70, 60)]);
    let q = quads(&note_skin(), &s, &view, &size);
    assert_eq!(dsts(&q), want_dst);
    assert_eq!(srcs(&q), want_src);
    assert!(q
        .iter()
        .all(|q| q.argb[0] == 255 && q.blend == Blend::Alpha));
    let before = quads(&note_skin(), &st(&[(0, 0)]), &view, &size);
    assert!(before.is_empty());
}

/// DOM-SKD-11: 小節線は DST_LINE の位置から 拍数 × 480 × ハイスピード / 600 上に、判定位置を過ぎていないものだけ α 255 で描く。プレイ開始後だけ
#[rstest]
#[case::ahead(vec![1.0, 0.0], true, vec![[10.0, 240.0, 100.0, 1.0], [10.0, 400.0, 100.0, 1.0]])]
#[case::passed_hidden(vec![-0.5], true, vec![])]
#[case::above_screen(vec![3.0], true, vec![])]
#[case::before_play(vec![1.0], false, vec![])]
fn dom_skd_11_measure_lines(
    #[case] lines: Vec<f64>,
    #[case] playing: bool,
    #[case] want: Vec<[f32; 4]>,
) {
    let e = el(
        other("LINE", 0),
        src([0, 0, 8, 1], (1, 1), 0, 0),
        0,
        vec![kf(0, [10.0, 400.0, 100.0, 1.0], 100, 0)],
    );
    let view = PlayView {
        hi_speed: 200.0,
        lines,
        ..Default::default()
    };
    let mut timers = vec![(0, 0)];
    if playing {
        timers.push((41, 10));
    }
    let q = quads(&skin(vec![e]), &st(&timers), &view, &size);
    assert_eq!(dsts(&q), want);
    assert!(q.iter().all(|q| q.argb[0] == 255));
}

/// 文字列 10 = "A"、画像フォント 0（大きさ 10）、システムフォント 0・1
fn text_skin(image_font: Option<ImageFont>, font: u32, dst: [f32; 4]) -> Skin {
    let e = Element {
        kind: ElementKind::Text {
            value_ref: 10,
            font,
            align: 0,
        },
        source: None,
        timer: TimerId(0),
        conditions: vec![],
        looping: Some(0),
        dst_extra: [0; 2],
        order: 0,
        keyframes: vec![Keyframe {
            angle: 30.0,
            ..kf(0, dst, 200, 0)
        }],
    };
    let mut sk = skin(vec![e]);
    sk.image_fonts = vec![image_font];
    sk.fonts = vec![
        Some(SystemFont {
            size: 24,
            thickness: 3,
            kind: 1,
        }),
        None,
    ];
    sk
}
fn glyph_font(size: i32) -> ImageFont {
    ImageFont {
        size,
        margin: 0,
        glyphs: [(
            'A',
            FontGlyph {
                image: 3,
                rect: [0, 0, 8, 10],
            },
        )]
        .into_iter()
        .collect(),
    }
}

/// DOM-SKD-12: 文字列は DST の幅・高さが 0 でなく、文字列が空でないときだけ描く。同じ番号の画像フォント（大きさ > 0）で、
/// 無ければ同じ番号のシステムフォントで描く。角度は使わない
#[rstest]
#[case::image_font(Some(glyph_font(10)), 0, [100.0, 50.0, 200.0, 20.0], "A", vec!["quad 3 [100.0, 50.0, 16.0, 20.0] 0"])]
#[case::system_font(None, 0, [100.0, 50.0, 200.0, 20.0], "A", vec!["text 24 A [100.0, 50.0, 200.0, 20.0] 200"])]
#[case::zero_size_image_font_uses_system(Some(glyph_font(0)), 0, [100.0, 50.0, 200.0, 20.0], "A", vec!["text 24 A [100.0, 50.0, 200.0, 20.0] 200"])]
#[case::zero_width(Some(glyph_font(10)), 0, [100.0, 50.0, 0.0, 20.0], "A", vec![])]
#[case::empty_text(Some(glyph_font(10)), 0, [100.0, 50.0, 200.0, 20.0], "", vec![])]
#[case::no_font(None, 1, [100.0, 50.0, 200.0, 20.0], "A", vec![])]
fn dom_skd_12_text(
    #[case] image_font: Option<ImageFont>,
    #[case] font: u32,
    #[case] dst: [f32; 4],
    #[case] text: &str,
    #[case] want: Vec<&str>,
) {
    let s = St {
        timers: [(0, 0)].into_iter().collect(),
        texts: [(10, text.to_string())].into_iter().collect(),
        ..Default::default()
    };
    let got: Vec<String> = draw_play(
        &text_skin(image_font, font, dst),
        &s,
        &PlayView::default(),
        &size,
    )
    .into_iter()
    .map(|d| match d {
        Draw::Quad(q) => format!("quad {} {:?} {}", q.image, q.dst, q.angle),
        Draw::Text(t) => format!("text {} {} {:?} {}", t.font.size, t.text, t.dst, t.argb[0]),
    })
    .collect();
    assert_eq!(got, want);
}

/// DOM-SKD-13: 画像フォントの並べ方。幅 = Σ(文字の幅 + 間隔) − 間隔、縦の倍率 = h / 大きさ、横の倍率 = 縦の倍率 × min(1, w / 幅)。
/// 揃え 1 は 幅 × 横の倍率 / 2、2 は 幅 × 横の倍率 だけ左へ（切り捨て）。無い文字は `?`、空白は描かずに進める。位置は四捨五入
#[rstest]
#[case::left("AB", [100.0, 50.0, 200.0, 20.0], 0, vec![[100.0, 50.0, 16.0, 20.0], [120.0, 50.0, 12.0, 20.0]])]
#[case::shrink_to_width("AB", [100.0, 50.0, 8.0, 20.0], 0, vec![[100.0, 50.0, 8.0, 20.0], [110.0, 50.0, 6.0, 20.0]])]
#[case::center("AB", [100.0, 50.0, 200.0, 20.0], 1, vec![[84.0, 50.0, 16.0, 20.0], [104.0, 50.0, 12.0, 20.0]])]
#[case::right("AB", [100.0, 50.0, 200.0, 20.0], 2, vec![[68.0, 50.0, 16.0, 20.0], [88.0, 50.0, 12.0, 20.0]])]
#[case::unknown_is_question("C", [100.0, 50.0, 200.0, 20.0], 0, vec![[100.0, 50.0, 8.0, 20.0]])]
#[case::space_advances("A A", [100.0, 50.0, 200.0, 20.0], 0, vec![[100.0, 50.0, 16.0, 20.0], [134.0, 50.0, 16.0, 20.0]])]
#[case::rounds("A", [100.4, 50.6, 200.0, 20.0], 0, vec![[100.0, 51.0, 16.0, 20.0]])]
fn dom_skd_13_image_font_layout(
    #[case] text: &str,
    #[case] dst: [f32; 4],
    #[case] align: i32,
    #[case] want: Vec<[f32; 4]>,
) {
    let g = |image: u32, x: i32, w: i32| FontGlyph {
        image,
        rect: [x, 0, w, 10],
    };
    let f = ImageFont {
        size: 10,
        margin: 2,
        glyphs: [
            ('A', g(1, 0, 8)),
            ('B', g(1, 8, 6)),
            ('?', g(2, 20, 4)),
            (' ', g(1, 30, 5)),
        ]
        .into_iter()
        .collect(),
    };
    let got: Vec<[f32; 4]> = layout_text(&f, text, dst, align)
        .into_iter()
        .map(|(_, _, d)| d)
        .collect();
    assert_eq!(got, want);
    let empty = ImageFont {
        size: 10,
        ..Default::default()
    };
    assert!(layout_text(&empty, "A", dst, align).is_empty());
}
