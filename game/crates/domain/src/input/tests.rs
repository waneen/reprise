//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

fn k(name: &str) -> Source {
    Source::Key(name.into())
}

/// DOM-INP-01: 既定の割り当て
#[rstest]
#[case::scratch_up(SCRATCH_UP, "ShiftLeft")]
#[case::scratch_down(SCRATCH_DOWN, "ControlLeft")]
#[case::key1(key_slot(1), "KeyZ")]
#[case::key2(key_slot(2), "KeyS")]
#[case::key7(key_slot(7), "KeyV")]
#[case::start(START, "KeyQ")]
#[case::select(SELECT, "KeyW")]
fn dom_inp_01_default(#[case] slot: usize, #[case] name: &str) {
    let b = Bindings::default();
    assert_eq!(b.slots[slot], vec![k(name)]);
    assert_eq!(b.analog_scratch_ms, None);
}

/// DOM-INP-02: 割り当て（同じ物理入力は 1 つの操作だけ）と外す
#[rstest]
#[case::add(key_slot(1), k("KeyA"), vec![k("KeyZ"), k("KeyA")], None)]
#[case::move_from_other(key_slot(1), k("KeyS"), vec![k("KeyZ"), k("KeyS")], Some(key_slot(2)))]
#[case::same_slot_once(key_slot(1), k("KeyZ"), vec![k("KeyZ")], None)]
#[case::pad(SCRATCH_UP, Source::Axis { axis: "LeftStickX".into(), plus: true }, vec![k("ShiftLeft"), Source::Axis { axis: "LeftStickX".into(), plus: true }], None)]
fn dom_inp_02_bind(
    #[case] slot: usize,
    #[case] src: Source,
    #[case] want: Vec<Source>,
    #[case] emptied: Option<usize>,
) {
    let mut b = Bindings::default();
    b.bind(slot, src.clone());
    assert_eq!(b.slots[slot], want);
    assert_eq!(b.slot_of(&src), Some(slot));
    if let Some(e) = emptied {
        assert!(b.slots[e].is_empty());
    }
    b.clear(slot);
    assert!(b.slots[slot].is_empty());
    assert_eq!(b.slot_of(&src), None);
}

/// DOM-INP-03: 軸をボタンとして読む
#[rstest]
#[case::plus_over(0.95, true, true)]
#[case::plus_at(0.9, true, false)]
#[case::plus_neg(-0.95, true, false)]
#[case::minus_under(-0.95, false, true)]
#[case::minus_at(-0.9, false, false)]
fn dom_inp_03_axis_as_button(#[case] v: f32, #[case] plus: bool, #[case] want: bool) {
    assert_eq!(axis_pressed(v, plus), want);
}

/// DOM-INP-04: アナログ皿（(時刻, 値) → (正, 負)）
#[rstest]
#[case::first_value_is_origin(vec![(0, 0.5)], vec![(false, false)])]
#[case::plus(vec![(0, 0.0), (1, 0.1)], vec![(false, false), (true, false)])]
#[case::minus(vec![(0, 0.0), (1, -0.1)], vec![(false, false), (false, true)])]
#[case::keeps_while_moving_recently(vec![(0, 0.0), (1, 0.1), (100, 0.1)], vec![(false, false), (true, false), (true, false)])]
#[case::stops_after_threshold(vec![(0, 0.0), (1, 0.1), (102, 0.1)], vec![(false, false), (true, false), (false, false)])]
#[case::turn_back(vec![(0, 0.0), (1, 0.1), (2, 0.05)], vec![(false, false), (true, false), (false, true)])]
#[case::wrap_plus(vec![(0, 0.95), (1, -0.95)], vec![(false, false), (true, false)])]
#[case::wrap_minus(vec![(0, -0.95), (1, 0.95)], vec![(false, false), (false, true)])]
fn dom_inp_04_analog_scratch(#[case] frames: Vec<(i64, f32)>, #[case] want: Vec<(bool, bool)>) {
    let mut a = AnalogScratch::default();
    let got: Vec<(bool, bool)> = frames
        .into_iter()
        .map(|(t, v)| a.update(t, v, 100))
        .collect();
    assert_eq!(got, want);
}
