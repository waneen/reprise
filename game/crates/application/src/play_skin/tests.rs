//! APP-SKN-* のテスト（ルール表は lib.rs）
use super::*;
use crate::play::{KeyId, KeyInput};
use crate::ports::AudioOutput;
use domain::chart::{Beat, ChartHash, ChartMeta, Note, SoundId};
use rstest::rstest;

struct Mute;
impl AudioOutput for Mute {
    fn play(&mut self, _: SoundId, _: i64) {}
}

/// BPM 60（1 拍 = 1000ms）。レーン 1 に 1, 2 拍、レーン 2 に 3〜5 拍の LN
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
                    end_sound: None,
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

/// フレーム（時刻, 物理キーの変化）を順に進める。キー 0 → 皿、1 → レーン 1、2 → レーン 2
fn run(
    gauge: GaugeType,
    measure0: Option<f64>,
    frames: &[(i64, &[(KeyId, bool)])],
) -> (PlaySession, PlaySkin) {
    let c = chart(measure0);
    let input = KeyInput::new([(0, Lane(0)), (1, Lane(1)), (2, Lane(2))]);
    let mut s = PlaySession::new(&c, gauge, input).unwrap();
    let mut k = PlaySkin::new(&c, &s, gauge, 2000, &[901]);
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

/// APP-SKN-01: 画面の開始でタイマー 0・1・40 が動き、41 は譜面の先頭から。変わらない条件:
/// 32（オートでない）・40（BGA なし）・42/43（GROOVE 系/HARD 系）・81（読み込み済み）・160..164（鍵盤数）・170/171・172/173（LN）・スキン設定
#[rstest]
#[case::before_play(GaugeType::Groove, &[(-1500, NONE)], vec![Timer(0, Some(500)), Timer(1, Some(500)), Timer(40, Some(500)), Timer(41, None), Op(32, true), Op(33, false), Op(40, true), Op(42, true), Op(43, false), Op(81, true), Op(160, true), Op(161, false), Op(170, true), Op(173, true), Op(172, false), Op(901, true), Op(900, false)])]
#[case::playing(GaugeType::Groove, &[(-1500, NONE), (100, NONE)], vec![Timer(0, Some(2100)), Timer(41, Some(100))])]
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
#[case::two_pgreats(&[(1000, PRESS1), (1100, UP1), (2000, PRESS1), (2100, UP1)], vec![
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
/// BARGRAPH 1・SLIDER 6 は曲の進み具合（最後のオブジェクトまで）
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
    assert_eq!(
        (v.gauge, v.survival, v.tremble, v.hi_speed),
        (10, false, 1, 200.0)
    );
    let _ = s;
}
