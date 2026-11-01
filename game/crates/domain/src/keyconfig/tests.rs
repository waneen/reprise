//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use rstest::rstest;

fn k(name: &str) -> Source {
    Source::Key(name.into())
}

/// DOM-KCF-01: キー番号が指す操作
#[rstest]
#[case::key1(1, Some((Side::P1, key_slot(1))))]
#[case::key7(7, Some((Side::P1, key_slot(7))))]
#[case::key8_9k(8, None)]
#[case::key9_9k(9, None)]
#[case::scratch_up(10, Some((Side::P1, SCRATCH_UP)))]
#[case::scratch_down(11, Some((Side::P1, SCRATCH_DOWN)))]
#[case::start(12, Some((Side::P1, START)))]
#[case::select(13, Some((Side::P1, SELECT)))]
#[case::unused(14, None)]
#[case::p2_key1(21, Some((Side::P2, key_slot(1))))]
#[case::p2_key7(27, Some((Side::P2, key_slot(7))))]
#[case::p2_scratch_up(30, Some((Side::P2, SCRATCH_UP)))]
#[case::p2_select(33, Some((Side::P2, SELECT)))]
#[case::zero(0, None)]
#[case::beyond(40, None)]
fn dom_kcf_01_key_id(#[case] id: i32, #[case] want: Option<(Side, usize)>) {
    assert_eq!(key_id_slot(id), want);
}

/// DOM-KCF-02: 7 鍵の並び ⇄ キー番号
#[rstest]
#[case::none(0, 0)]
#[case::key1(1, 1)]
#[case::key7(7, 7)]
#[case::scratch_up(8, 10)]
#[case::select(11, 13)]
#[case::p2_key1(12, 21)]
#[case::p2_key7(18, 27)]
#[case::p2_scratch_up(19, 30)]
#[case::p2_select(22, 33)]
#[case::beyond(23, 0)]
fn dom_kcf_02_order7(#[case] button: i32, #[case] key_id: i32) {
    assert_eq!(button_to_key_id7(button), key_id);
    if key_id != 0 {
        assert_eq!(button_from_key_id7(key_id), button);
    }
    // 並びに無いキー番号は 0
    assert_eq!(button_from_key_id7(8), 0);
    assert_eq!(button_from_key_id7(0), 0);
}

/// DOM-KCF-03: 画面に入ったとき
#[rstest]
#[case::enter(KeyConfig { button: 1, key_id: 1, place: -1 })]
fn dom_kcf_03_enter(#[case] want: KeyConfig) {
    assert_eq!(KeyConfig::default(), want);
}

/// DOM-KCF-04: BUTTON のクリック（並び, キー番号, 位置）
#[rstest]
#[case::key_select(vec![(105, 1)], (5, 5, -1), true)]
#[case::key_select_left_side(vec![(110, -1)], (8, 10, -1), true)]
#[case::key_not_in_order(vec![(108, 1)], (0, 8, -1), true)]
#[case::key_zero(vec![(100, 1)], (0, 0, -1), true)]
#[case::order_next(vec![(140, 1)], (2, 2, -1), true)]
#[case::order_prev(vec![(140, -1)], (0, 0, -1), true)]
#[case::order_wraps_down(vec![(140, -1), (140, -1)], (22, 33, -1), true)]
#[case::order_wraps_up(vec![(133, 1), (140, 1)], (0, 0, -1), true)]
#[case::place(vec![(153, 1)], (1, 1, 3), true)]
#[case::place_last(vec![(157, -1)], (1, 1, 7), true)]
#[case::nine_keys(vec![(141, 1)], (1, 1, -1), false)]
#[case::five_keys(vec![(142, 1)], (1, 1, -1), false)]
#[case::mode(vec![(143, 1)], (1, 1, -1), false)]
#[case::other(vec![(13, 1)], (1, 1, -1), false)]
fn dom_kcf_04_click(
    #[case] clicks: Vec<(i32, i32)>,
    #[case] want: (i32, i32, i32),
    #[case] last_handled: bool,
) {
    let mut c = KeyConfig::default();
    let mut handled = false;
    for (a, d) in clicks {
        handled = c.click(a, d);
    }
    assert_eq!((c.button, c.key_id, c.place), want);
    assert_eq!(handled, last_handled);
}

fn axis(plus: bool) -> Source {
    Source::Axis {
        axis: "LeftStickY".into(),
        plus,
    }
}

/// DOM-KCF-05: 押した物理入力で割り当てる（キー番号, 位置, 押したもの → 結果と、キー番号の操作（無ければ 1P の鍵盤 1）の位置）
#[rstest]
#[case::key(1, 0, vec![k("KeyA")], Assigned::Set(Some(k("KeyA"))), vec![Some(k("KeyA"))])]
#[case::second_place(1, 1, vec![k("KeyA")], Assigned::Set(Some(k("KeyA"))), vec![Some(k("KeyZ")), Some(k("KeyA"))])]
#[case::hole(1, 3, vec![Source::Button("South".into())], Assigned::Set(Some(Source::Button("South".into()))), vec![Some(k("KeyZ")), None, None, Some(Source::Button("South".into()))])]
#[case::axis(1, 0, vec![axis(false)], Assigned::Set(Some(axis(false))), vec![Some(axis(false))])]
#[case::first_pressed(1, 0, vec![k("KeyB"), k("KeyA")], Assigned::Set(Some(k("KeyB"))), vec![Some(k("KeyB"))])]
#[case::delete_clears(1, 0, vec![k("Delete")], Assigned::Set(None), vec![])]
#[case::delete_with_other(1, 0, vec![k("KeyA"), k("Delete")], Assigned::Set(None), vec![])]
#[case::banned_escape(1, 0, vec![k("Escape")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::banned_enter(1, 0, vec![k("Enter")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::banned_arrow(1, 0, vec![k("ArrowLeft")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::banned_f1(1, 0, vec![k("F1")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::banned_f12(1, 0, vec![k("F12")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::banned_first_blocks(1, 0, vec![k("F2"), k("KeyA")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::f13_ok(1, 0, vec![k("F13")], Assigned::Set(Some(k("F13"))), vec![Some(k("F13"))])]
#[case::numpad_enter_ok(1, 0, vec![k("NumpadEnter")], Assigned::Set(Some(k("NumpadEnter"))), vec![Some(k("NumpadEnter"))])]
#[case::no_place(1, -1, vec![k("KeyA")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::no_key(0, 0, vec![k("KeyA")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::nine_key_slot(8, 0, vec![k("KeyA")], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::nothing_pressed(1, 0, vec![], Assigned::Nothing, vec![Some(k("KeyZ"))])]
#[case::start(12, 1, vec![k("KeyP")], Assigned::Set(Some(k("KeyP"))), vec![Some(k("KeyQ")), Some(k("KeyP"))])]
#[case::p2_scratch(30, 0, vec![k("KeyP")], Assigned::Set(Some(k("KeyP"))), vec![Some(k("KeyP"))])]
fn dom_kcf_05_assign(
    #[case] key_id: i32,
    #[case] place: i32,
    #[case] pressed: Vec<Source>,
    #[case] want: Assigned,
    #[case] places: Vec<Option<Source>>,
) {
    let c = KeyConfig {
        button: button_from_key_id7(key_id),
        key_id,
        place,
    };
    let mut b = Bindings::default();
    assert_eq!(c.assign(&mut b, &pressed), want);
    let got = match key_id_slot(key_id) {
        Some((Side::P2, s)) => b.p2[s].clone(),
        Some((Side::P1, s)) => b.slots[s].clone(),
        None => b.slots[key_slot(1)].clone(),
    };
    assert_eq!(got, places);
}

/// DOM-KCF-06: 文字列 40..=55（キー番号 0 は 40..=47 だけ `-` にして 48..=55 は前のまま）
#[rstest]
#[case::key1(1, vec![Some(k("KeyZ")), None, Some(k("KeyL"))], vec!["Z", "-", "l"], "-")]
#[case::empty_slot(12, vec![], vec![], "-")]
#[case::nine_key(8, vec![], vec![], "-")]
#[case::zero(0, vec![], vec![], "old")]
fn dom_kcf_06_texts(
    #[case] key_id: i32,
    #[case] places: Vec<Option<Source>>,
    #[case] head: Vec<&str>,
    #[case] rest: &str,
) {
    let mut b = Bindings::default();
    if let Some((side, slot)) = key_id_slot(key_id) {
        b.set(side, slot, 0, None);
        for (i, p) in places.into_iter().enumerate() {
            b.set(side, slot, i, p);
        }
    }
    let c = KeyConfig {
        button: button_from_key_id7(key_id),
        key_id,
        place: -1,
    };
    let mut texts: [String; 16] = std::array::from_fn(|_| "old".into());
    c.texts(&b, &mut texts);
    for (i, t) in texts.iter().enumerate() {
        let want = head
            .get(i)
            .copied()
            .unwrap_or(if i < 8 { "-" } else { rest });
        assert_eq!(t, want, "{i}");
    }
}

/// DOM-KCF-07: BUTTON の状態値
#[rstest]
#[case::selected_key(101, Some(1))]
#[case::other_key(102, Some(0))]
#[case::order(140, Some(9))]
#[case::order_9k_button(141, Some(9))]
#[case::mode(143, Some(0))]
#[case::selected_place(152, Some(1))]
#[case::other_place(150, Some(0))]
#[case::unknown(158, None)]
#[case::unknown_low(99, None)]
fn dom_kcf_07_button_value(#[case] action: i32, #[case] want: Option<i32>) {
    let c = KeyConfig {
        button: 9,
        key_id: 1,
        place: 2,
    };
    assert_eq!(c.button_value(action), want);
}

/// DOM-KCF-08: 表示名
#[rstest]
#[case::letter(k("KeyZ"), "Z")]
#[case::l_lower(k("KeyL"), "l")]
#[case::shift(k("ShiftLeft"), "LSHIFT")]
#[case::ctrl(k("ControlLeft"), "LCONTROL")]
#[case::digit(k("Digit0"), "0")]
#[case::numpad(k("Numpad7"), "NUMPAD7")]
#[case::space(k("Space"), "SPACE")]
#[case::yen(k("IntlYen"), "YEN")]
#[case::win(k("SuperLeft"), "LWIN")]
#[case::unknown_key(k("Lang1"), "Lang1")]
#[case::button(Source::Button("South".into()), "South")]
#[case::axis_plus(axis(true), "LeftStickY+")]
#[case::axis_minus(axis(false), "LeftStickY-")]
fn dom_kcf_08_name(#[case] s: Source, #[case] want: &str) {
    assert_eq!(lr2_name(&s), want);
}
