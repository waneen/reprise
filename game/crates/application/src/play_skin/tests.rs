//! APP-SKN-* のテスト（ルール表は lib.rs）
use super::*;
use crate::play::{KeyId, KeyInput};
use crate::ports::AudioOutput;
use domain::chart::{Beat, ChartHash, ChartMeta, ImageId, Lane, Note, SoundId};
use rstest::rstest;

struct Mute;
impl AudioOutput for Mute {
    fn play(&mut self, _: SoundId, _: i64) {}
}

/// BPM 60（1 拍 = 1000ms）。レーン 1 に 1, 2 拍、レーン 2 に 3〜5 拍の LN（終端の音 1）
fn chart(measure0: Option<f64>) -> Chart {
    let note = |b: f64, lane: u8, kind: NoteKind| Note {
        beat: Beat(b),
        lane: Lane(lane),
        kind,
        sound: None,
    };
    Chart {
        hash: ChartHash {
            md5: [1; 16],
            sha256: None,
        },
        meta: ChartMeta {
            initial_bpm: Some(60.0),
            ..Default::default()
        },
        key_mode: KeyMode::K7,
        notes: vec![
            note(1.0, 1, NoteKind::Normal),
            note(2.0, 1, NoteKind::Normal),
            note(
                3.0,
                2,
                NoteKind::Long {
                    end: Beat(5.0),
                    end_sound: Some(SoundId(1)),
                },
            ),
        ],
        bgm: vec![],
        bga: vec![],
        timing: measure0
            .map(|length| TimingEvent::Measure { index: 0, length })
            .into_iter()
            .collect(),
        sounds: vec![],
        images: vec![],
    }
}

/// 入力開始 300、準備 500 + 500、プレイ開始 1000（譜面の先頭は画面の開始から 2000ms）、FADEOUT 500、CLOSE 300
const TIMING: SceneTiming = SceneTiming {
    start_input: 300,
    load_start: 500,
    load_end: 500,
    play_start: 1000,
    fade_out: 500,
    close: 300,
};

/// フレーム（時刻, 物理キーの変化）を順に進める。キー 0 → 皿、1 → レーン 1、2 → レーン 2
fn run(
    gauge: GaugeType,
    measure0: Option<f64>,
    frames: &[(i64, &[(KeyId, bool)])],
) -> (PlaySession, PlaySkin) {
    run_with(gauge, measure0, &BTreeMap::new(), frames)
}

fn run_with(
    gauge: GaugeType,
    measure0: Option<f64>,
    sound_ms: &BTreeMap<SoundId, i64>,
    frames: &[(i64, &[(KeyId, bool)])],
) -> (PlaySession, PlaySkin) {
    let c = chart(measure0);
    let input = KeyInput::new([(0, Lane(0)), (1, Lane(1)), (2, Lane(2))]);
    let mut s = PlaySession::new(&c, gauge, input).unwrap();
    let mut k = PlaySkin::new(&c, &s, gauge, TIMING, &[901], sound_ms, &BTreeMap::new());
    for (t, ev) in frames {
        let out = s.frame(*t, ev, &mut Mute);
        k.update(*t, &s, &out);
    }
    (s, k)
}

#[derive(Debug, Clone, Copy)]
enum P {
    Timer(i32, Option<i64>),
    Op(i32, bool),
    Num(i32, Option<i64>),
}
use P::*;

fn check(k: &PlaySkin, probes: &[P]) {
    for p in probes {
        match *p {
            Timer(id, want) => assert_eq!(k.timer(id), want, "timer {id}"),
            Op(id, want) => assert_eq!(k.option(id), want, "option {id}"),
            Num(id, want) => assert_eq!(k.number(id), want, "number {id}"),
        }
    }
}

const PRESS1: &[(KeyId, bool)] = &[(1, true)];
const UP1: &[(KeyId, bool)] = &[(1, false)];
const PRESS2: &[(KeyId, bool)] = &[(2, true)];
const NONE: &[(KeyId, bool)] = &[];

/// APP-SKN-01: タイマー 0 は画面の開始、1 は `#STARTINPUT` を過ぎてから、40 は `#LOADSTART`+`#LOADEND`、
/// 41 は譜面の先頭（さらに `#PLAYSTART` 後）から。変わらない条件:
/// 32（オートでない）・41（BGA あり）・42/43（GROOVE 系/HARD 系）・81（読み込み済み）・160..164（鍵盤数）・170/171・172/173（LN）・スキン設定
#[rstest]
#[case::before_play(GaugeType::Groove, &[(-1500, NONE)], vec![Timer(0, Some(500)), Timer(1, Some(0)), Timer(40, None), Timer(41, None), Op(32, true), Op(33, false), Op(40, false), Op(41, true), Op(42, true), Op(43, false), Op(81, true), Op(160, true), Op(161, false), Op(170, true), Op(173, true), Op(172, false), Op(901, true), Op(900, false)])]
#[case::before_input(GaugeType::Groove, &[(-1800, NONE)], vec![Timer(0, Some(200)), Timer(1, None)])]
#[case::ready(GaugeType::Groove, &[(-1500, NONE), (-500, NONE)], vec![Timer(1, Some(1000)), Timer(40, Some(500)), Timer(41, None)])]
#[case::playing(GaugeType::Groove, &[(-1500, NONE), (100, NONE)], vec![Timer(0, Some(2100)), Timer(40, Some(1100)), Timer(41, Some(100))])]
#[case::survival(GaugeType::Hard, &[], vec![Op(42, false), Op(43, true)])]
fn app_skn_01_start(
    #[case] gauge: GaugeType,
    #[case] frames: &[(i64, &[(KeyId, bool)])],
    #[case] probes: Vec<P>,
) {
    let (_, k) = run(gauge, None, frames);
    check(&k, &probes);
}

/// APP-SKN-02: 判定でタイマー 46 を、PG/GR ならさらに 50+レーン を始める。最後の判定は条件 241(PG)..246(空POOR)。
/// BD 以下から 1 秒未満は 248、それ以外は 247。地雷は 46 だけを始め直す
#[rstest]
#[case::pgreat(&[(1000, PRESS1), (1100, NONE)], vec![Timer(46, Some(100)), Timer(51, Some(100)), Op(241, true), Op(247, true), Op(248, false)])]
#[case::bad(&[(1150, PRESS1), (1200, NONE)], vec![Timer(46, Some(50)), Timer(51, None), Op(244, true), Op(241, false), Op(248, true), Op(247, false)])]
#[case::miss_expires(&[(1150, PRESS1), (2150, NONE)], vec![Op(247, true), Op(248, false)])]
#[case::empty_poor(&[(300, PRESS1)], vec![Op(246, true), Op(248, true)])]
#[case::nothing_yet(&[(300, NONE)], vec![Timer(46, None), Op(241, false), Op(246, false), Op(247, true)])]
fn app_skn_02_judge(#[case] frames: &[(i64, &[(KeyId, bool)])], #[case] probes: Vec<P>) {
    let (_, k) = run(GaugeType::Groove, None, frames);
    check(&k, &probes);
}

/// APP-SKN-03: 押したらタイマー 100+レーン を始めて 120+レーン を止め、離したら逆。押下中の LN の間は 70+レーン
#[rstest]
#[case::key_on(&[(500, PRESS1), (600, NONE)], vec![Timer(101, Some(100)), Timer(121, None)])]
#[case::key_off(&[(500, PRESS1), (700, UP1), (750, NONE)], vec![Timer(101, None), Timer(121, Some(50))])]
#[case::ln_held(&[(3000, PRESS2), (3500, NONE)], vec![Timer(72, Some(500)), Timer(102, Some(500))])]
#[case::ln_not_held(&[(2900, NONE)], vec![Timer(72, None)])]
fn app_skn_03_keys(#[case] frames: &[(i64, &[(KeyId, bool)])], #[case] probes: Vec<P>) {
    let (_, k) = run(GaugeType::Groove, None, frames);
    check(&k, &probes);
}

/// APP-SKN-04: 表示 HP（偶数）が増えて 100 未満ならタイマー 42、100 なら 44（100 未満・途中終了で止める）。途中終了で 3、
/// 全ノーツ COMBO で 48、全ノーツを片付けたら 143。条件 230..239 は HP の 10% ごとの帯、240 は 100%
#[rstest]
#[case::gauge_up(GaugeType::Groove, &[(1000, PRESS1), (1100, NONE)], vec![Timer(42, Some(100)), Timer(44, None), Op(237, true), Op(240, false)])]
#[case::gauge_max(GaugeType::Groove, &[(1000, PRESS1), (1100, UP1), (2000, PRESS1), (2100, NONE)], vec![Timer(42, None), Timer(44, Some(100)), Op(240, true), Op(239, false)])]
#[case::start_band(GaugeType::Groove, &[(0, NONE)], vec![Op(232, true), Op(231, false)])]
#[case::survival_full(GaugeType::Hard, &[(0, NONE)], vec![Timer(44, Some(2000)), Op(240, true)])]
#[case::dead(GaugeType::Hazard, &[(1300, NONE), (1400, NONE)], vec![Timer(3, Some(100)), Timer(44, None), Timer(143, None)])]
#[case::full_combo(GaugeType::Groove, &[(1000, PRESS1), (1100, UP1), (2000, PRESS1), (2100, UP1), (3000, PRESS2), (5001, NONE), (5101, NONE)], vec![Timer(48, Some(100)), Timer(143, Some(100))])]
fn app_skn_04_gauge_and_progress(
    #[case] gauge: GaugeType,
    #[case] frames: &[(i64, &[(KeyId, bool)])],
    #[case] probes: Vec<P>,
) {
    let (_, k) = run(gauge, None, frames);
    check(&k, &probes);
}

/// APP-SKN-05: 数値。100 = (GD + (GR + PG×2)×2) × 50000 / 総ノーツ、101 = EX、102/103 = 片付けたノーツに対する率と小数 2 桁、
/// 104/105/106 = コンボ・最大コンボ・総ノーツ、107 = 表示 HP、110..114 = 判定数、115 = 総ノーツに対する率、160 = BPM、
/// 161/162 = 経過の分・秒、163/164 = 残りの分・秒（1 秒未満は無し）
#[rstest]
#[case::two_pgreats(&[(1000, PRESS1), (1100, UP1), (2000, PRESS1), (2100, UP1), (2600, NONE)], vec![
    Num(100, Some(133333)), Num(101, Some(4)), Num(102, Some(100)), Num(103, Some(0)), Num(104, Some(2)), Num(105, Some(2)),
    Num(106, Some(3)), Num(107, Some(100)), Num(110, Some(2)), Num(111, Some(0)), Num(114, Some(0)), Num(115, Some(66)),
    Num(160, Some(60)), Num(161, Some(0)), Num(162, Some(2)), Num(163, Some(0)), Num(164, Some(2)),
])]
#[case::great_rate(&[(1030, PRESS1), (1100, UP1)], vec![Num(101, Some(1)), Num(102, Some(50)), Num(103, Some(0)), Num(111, Some(1))])]
#[case::before_any(&[(500, NONE)], vec![Num(102, None), Num(161, None), Num(162, None), Num(163, Some(0)), Num(164, Some(4)), Num(999, None)])]
fn app_skn_05_numbers(#[case] frames: &[(i64, &[(KeyId, bool)])], #[case] probes: Vec<P>) {
    let (_, k) = run(GaugeType::Groove, None, frames);
    check(&k, &probes);
}

/// APP-SKN-06: 描画材料。ノーツは判定までの拍数、ゲージは HP/2。タイマー 140 は 1 拍 = 1000 で小節線ごとに 0 から。
/// BARGRAPH 1・SLIDER 6 は曲の進み具合（曲の長さまで）。小節線は判定までの拍数
#[rstest]
#[case::rhythm(None, 1500, 1500, 0.3)]
#[case::rhythm_next_measure(None, 4500, 500, 0.9)]
#[case::rhythm_short_measure(Some(0.5), 2500, 500, 0.5)]
fn app_skn_06_view(
    #[case] measure0: Option<f64>,
    #[case] t: i64,
    #[case] rhythm: i64,
    #[case] progress: f64,
) {
    let (s, k) = run(GaugeType::Groove, measure0, &[(t, NONE)]);
    assert_eq!(k.timer(140), Some(rhythm));
    assert_eq!(k.bar(1), progress);
    assert_eq!(k.slider(6), progress);
    assert_eq!(k.bar(2), 0.0);
    let (s0, k0) = run(GaugeType::Groove, None, &[(0, NONE)]);
    let v = k0.view(&s0, 200.0, 4.0, 1);
    let notes: Vec<(i32, f64, Option<f64>)> =
        v.notes.iter().map(|n| (n.lane, n.head, n.tail)).collect();
    assert_eq!(
        notes,
        vec![(1, 1.0, None), (1, 2.0, None), (2, 3.0, Some(5.0))]
    );
    assert_eq!(v.lines, vec![0.0, 4.0]);
    assert_eq!(
        (v.gauge, v.survival, v.tremble, v.hi_speed),
        (10, false, 1, 200.0)
    );
    let _ = s;
}

/// APP-SKN-07: 表示の追従。スコア（数値 100）は 500ms で、HP（数値 107・ゲージ）は差 × 10ms で直線的に新しい値へ。
/// 追いかけている途中で値が変わったら、その時点の表示から追いかけ直す
#[rstest]
#[case::score_at_judge(&[(1000, PRESS1)], 100, Some(0))]
#[case::score_halfway(&[(1000, PRESS1), (1250, NONE)], 100, Some(33333))]
#[case::score_done(&[(1000, PRESS1), (1500, NONE)], 100, Some(66666))]
#[case::score_retarget(&[(1000, PRESS1), (1250, UP1), (2000, PRESS1), (2250, NONE)], 100, Some(99999))]
#[case::hp_at_judge(&[(0, NONE), (1000, PRESS1)], 107, Some(20))]
#[case::hp_halfway(&[(0, NONE), (1000, PRESS1), (1260, NONE)], 107, Some(46))]
#[case::hp_done(&[(0, NONE), (1000, PRESS1), (1520, NONE)], 107, Some(72))]
fn app_skn_07_follow(
    #[case] frames: &[(i64, &[(KeyId, bool)])],
    #[case] id: i32,
    #[case] want: Option<i64>,
) {
    let (_, k) = run(GaugeType::Groove, None, frames);
    assert_eq!(k.number(id), want);
}

/// APP-SKN-08: 曲の長さ = 各オブジェクトの時刻 + その音の長さ の最大。それを過ぎるとタイマー 2、
/// そこから `#FADEOUT` を過ぎたら場面を閉じる。途中終了はタイマー 3 から `#CLOSE`
#[rstest]
#[case::playing(&[], GaugeType::Groove, &[(5000, NONE)], None, None, false)]
#[case::song_end(&[], GaugeType::Groove, &[(5001, NONE)], Some(0), None, false)]
#[case::faded(&[], GaugeType::Groove, &[(5001, NONE), (5502, NONE)], Some(501), None, true)]
#[case::sound_tail(&[(1, 800)], GaugeType::Groove, &[(5500, NONE)], None, None, false)]
#[case::dead(&[], GaugeType::Hazard, &[(1300, NONE)], None, Some(0), false)]
#[case::closed(&[], GaugeType::Hazard, &[(1300, NONE), (1601, NONE)], None, Some(301), true)]
fn app_skn_08_scene_end(
    #[case] sounds: &[(u32, i64)],
    #[case] gauge: GaugeType,
    #[case] frames: &[(i64, &[(KeyId, bool)])],
    #[case] t2: Option<i64>,
    #[case] t3: Option<i64>,
    #[case] over: bool,
) {
    let sound_ms = sounds.iter().map(|&(s, l)| (SoundId(s), l)).collect();
    let (_, k) = run_with(gauge, None, &sound_ms, frames);
    assert_eq!((k.timer(2), k.timer(3), k.scene_over()), (t2, t3, over));
}

/// APP-SKN-09: 皿の角度。回している間はその向きに 720°/s、それ以外はプレイ開始後 360°/s、開始前は止まる
#[rstest]
#[case::idle_before(0, -1000, 0.0)]
#[case::idle_playing(0, 250, 90.0)]
#[case::up(-1, 250, 180.0)]
#[case::down(1, 125, 90.0)]
fn app_skn_09_turntable(#[case] dir: i32, #[case] t: i64, #[case] angle: f32) {
    let c = chart(None);
    let input = KeyInput::new([(0, Lane(0))]);
    let s = PlaySession::new(&c, GaugeType::Groove, input).unwrap();
    let mut k = PlaySkin::new(
        &c,
        &s,
        GaugeType::Groove,
        TIMING,
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    k.update(t.min(0), &s, &[]);
    k.set_scratch(dir);
    k.update(t.max(0), &s, &[]);
    assert_eq!(k.turntable(1), angle);
}

/// APP-SKN-10: 文字列 10 題名・11 副題・12 題名 + 空白 + 副題・13 ジャンル・14 アーティスト・15 副アーティスト。空は無し
#[rstest]
#[case::with_subtitle("[A]", vec![(10, Some("T")), (11, Some("[A]")), (12, Some("T [A]")), (13, Some("G")), (14, Some("AR")), (15, None), (16, None)])]
#[case::no_subtitle("", vec![(11, None), (12, Some("T"))])]
fn app_skn_10_texts(#[case] subtitle: &str, #[case] want: Vec<(i32, Option<&str>)>) {
    let mut c = chart(None);
    c.meta.title = "T".into();
    c.meta.subtitle = subtitle.into();
    c.meta.genre = "G".into();
    c.meta.artist = "AR".into();
    let input = KeyInput::new([(0, Lane(0))]);
    let s = PlaySession::new(&c, GaugeType::Groove, input).unwrap();
    let k = PlaySkin::new(
        &c,
        &s,
        GaugeType::Groove,
        TIMING,
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    for (id, w) in want {
        assert_eq!(k.text(id), w, "text {id}");
    }
}

/// APP-SKN-11: BGA は譜面の時刻（≤ 今）で層を切り替える。POOR 層の初期値は 0 番の画像。BD 以下から 500ms 未満は POOR 層だけ
/// （後ろの黒なし）、それ以外は 1 層目（後ろに黒）・2 層目。読めない画像は描かない。
/// 0.5 拍に 1 層目 = 1、1.5 拍に 2 層目 = 2、2.5 拍に POOR 層 = 4、3.5 拍に 1 層目 = 3（読めない）
#[rstest]
#[case::nothing_yet(true, &[(300, NONE)], vec![])]
#[case::base(true, &[(500, NONE)], vec![(11, true)])]
#[case::base_and_layer(true, &[(1000, PRESS1), (1100, UP1), (1600, NONE)], vec![(11, true), (12, false)])]
#[case::poor_default_image(true, &[(600, NONE), (1150, PRESS1), (1200, NONE)], vec![(10, false)])]
#[case::poor_expires(true, &[(600, NONE), (1150, PRESS1), (1650, NONE)], vec![(11, true), (12, false)])]
#[case::poor_changed(true, &[(2150, PRESS1), (2600, NONE)], vec![(14, false)])]
#[case::no_poor_image(false, &[(600, NONE), (1150, PRESS1), (1200, NONE)], vec![(11, true)])]
#[case::unloaded_base(true, &[(1000, PRESS1), (1100, UP1), (2000, PRESS1), (2100, UP1), (3000, PRESS2), (3600, NONE)], vec![(12, false)])]
fn app_skn_11_bga(
    #[case] with_image0: bool,
    #[case] frames: &[(i64, &[(KeyId, bool)])],
    #[case] want: Vec<(u32, bool)>,
) {
    let mut c = chart(None);
    c.bga = vec![
        (Beat(0.5), BgaLayer::Base, ImageId(1)),
        (Beat(1.5), BgaLayer::Layer, ImageId(2)),
        (Beat(2.5), BgaLayer::Poor, ImageId(4)),
        (Beat(3.5), BgaLayer::Base, ImageId(3)),
    ];
    let mut images: BTreeMap<ImageId, u32> = [(1, 11), (2, 12), (4, 14)]
        .into_iter()
        .map(|(i, n)| (ImageId(i), n))
        .collect();
    if with_image0 {
        images.insert(ImageId(0), 10);
    }
    let input = KeyInput::new([(0, Lane(0)), (1, Lane(1)), (2, Lane(2))]);
    let mut s = PlaySession::new(&c, GaugeType::Groove, input).unwrap();
    let mut k = PlaySkin::new(
        &c,
        &s,
        GaugeType::Groove,
        TIMING,
        &[],
        &BTreeMap::new(),
        &images,
    );
    for (t, ev) in frames {
        let out = s.frame(*t, ev, &mut Mute);
        k.update(*t, &s, &out);
    }
    let got: Vec<(u32, bool)> = k
        .view(&s, 200.0, 4.0, 0)
        .bga
        .iter()
        .map(|b| (b.image, b.backbox))
        .collect();
    assert_eq!(got, want);
    assert!(k.option(41) && !k.option(40));
}
