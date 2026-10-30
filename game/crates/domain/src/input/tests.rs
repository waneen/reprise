//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

fn k(name: &str) -> Source {
    Source::Key(name.into())
}

/// DOM-INP-01: 既定の割り当て（2P は無し）
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
    assert_eq!(b.slots[slot], vec![Some(k(name))]);
    assert!(b.p2[slot].is_empty());
    assert_eq!(b.analog_scratch_ms, None);
}

fn axis() -> Source {
    Source::Axis {
        axis: "LeftStickX".into(),
        plus: true,
    }
}

/// DOM-INP-02: 位置に置く・外す（同じ物理入力をほかの操作・位置にも置ける。末尾の空きは持たない）と、物理入力の付いている操作
#[rstest]
#[case::append(Side::P1, key_slot(1), 1, Some(k("KeyA")), vec![Some(k("KeyZ")), Some(k("KeyA"))], vec![key_slot(1)])]
#[case::replace(Side::P1, key_slot(1), 0, Some(k("KeyA")), vec![Some(k("KeyA"))], vec![key_slot(1)])]
#[case::hole(Side::P1, key_slot(1), 3, Some(axis()), vec![Some(k("KeyZ")), None, None, Some(axis())], vec![key_slot(1)])]
#[case::same_source_twice(Side::P1, key_slot(1), 1, Some(k("KeyS")), vec![Some(k("KeyZ")), Some(k("KeyS"))], vec![key_slot(1), key_slot(2)])]
#[case::remove_trims(Side::P1, key_slot(1), 0, None, vec![], vec![])]
#[case::beyond_places(Side::P1, key_slot(1), 16, Some(k("KeyA")), vec![Some(k("KeyZ"))], vec![])]
#[case::p2_not_in_p1(Side::P2, key_slot(1), 0, Some(k("KeyA")), vec![Some(k("KeyZ"))], vec![])]
fn dom_inp_02_set(
    #[case] side: Side,
    #[case] slot: usize,
    #[case] place: usize,
    #[case] src: Option<Source>,
    #[case] want_p1: Vec<Option<Source>>,
    #[case] slots_of_src: Vec<usize>,
) {
    let mut b = Bindings::default();
    b.set(side, slot, place, src.clone());
    assert_eq!(b.slots[slot], want_p1);
    if side == Side::P2 {
        assert_eq!(b.place(Side::P2, slot, place), src.as_ref());
    }
    if let Some(s) = &src {
        assert_eq!(b.slots_of(s).collect::<Vec<_>>(), slots_of_src);
    }
    b.clear_keys();
    assert!(b.slots.iter().chain(b.p2.iter()).all(Vec::is_empty));
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

/// DOM-INP-05: 読み方ごとの操作（1P・2P・SP は 1P と 2P をまとめて 1P の操作に）
#[rstest]
#[case::p1_reads_p1(Reading::P1, "KeyZ", vec![key_slot(1)])]
#[case::p1_ignores_p2(Reading::P1, "KeyM", vec![])]
#[case::p2_reads_p2(Reading::P2, "KeyM", vec![key_slot(1)])]
#[case::p2_ignores_p1(Reading::P2, "KeyZ", vec![])]
#[case::single_reads_p1(Reading::Single, "KeyZ", vec![key_slot(1)])]
#[case::single_reads_p2_as_p1(Reading::Single, "KeyM", vec![key_slot(1)])]
#[case::single_p2_start(Reading::Single, "KeyP", vec![START])]
fn dom_inp_05_reading(#[case] reading: Reading, #[case] key: &str, #[case] want: Vec<usize>) {
    let mut b = Bindings::default();
    b.set(Side::P2, key_slot(1), 0, Some(k("KeyM")));
    b.set(Side::P2, START, 0, Some(k("KeyP")));
    let src = k(key);
    assert_eq!(b.slots_in(reading, &src).collect::<Vec<_>>(), want);
    for &slot in &want {
        assert!(b.sources_in(reading, slot).any(|s| s == &src));
    }
}
