//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use domain::input::{key_slot, Side, SCRATCH_DOWN, SCRATCH_UP, START};
use rstest::rstest;

fn timing(start_input: i32, fade_out: i32) -> SceneTiming {
    SceneTiming {
        start_input,
        fade_out,
        ..Default::default()
    }
}

fn click(action: i32) -> ButtonClick {
    ButtonClick { action, delta: 1 }
}

/// APP-KCF-01: タイマー 0・1・11・2 と閉じる時刻（(時刻, Esc を押す) → 閉じたフレーム）
#[rstest]
#[case::before_input(vec![(100, false)], vec![(0, Some(100)), (1, None), (2, None)], None)]
#[case::after_input(vec![(100, false), (250, false), (349, false)], vec![(1, Some(99)), (11, Some(99)), (2, None)], None)]
#[case::fade(vec![(300, true), (400, false), (800, false), (801, false)], vec![(2, Some(501))], Some(3))]
#[case::no_fade_next_frame(vec![(300, true), (301, false)], vec![(2, Some(1))], Some(1))]
fn app_kcf_01_timers(
    #[case] frames: Vec<(i64, bool)>,
    #[case] timers: Vec<(i32, Option<i64>)>,
    #[case] closed_at: Option<usize>,
) {
    let fade = if closed_at == Some(1) { 0 } else { 500 };
    let mut b = Bindings::default();
    let mut s = KeyConfigSkin::new(timing(200, fade), &[], &b);
    let esc = [key("Escape")];
    let mut closed = None;
    for (i, (t, e)) in frames.into_iter().enumerate() {
        let input = KeyConfigInput {
            pressed: if e { &esc } else { &[] },
            ..Default::default()
        };
        if s.frame(t, &mut b, input).close && closed.is_none() {
            closed = Some(i);
        }
    }
    for (id, want) in timers {
        assert_eq!(s.timer(id), want, "timer {id}");
    }
    assert_eq!(closed, closed_at);
}

/// APP-KCF-02: 条件 400 とスキンの条件、文字列 40..=55、BUTTON の状態値
#[rstest]
#[case::enter(vec![], vec![(40, Some("Z")), (41, Some("-")), (55, Some("-")), (39, None), (56, None)], vec![(101, Some(1)), (140, Some(1)), (150, Some(0))])]
#[case::select_start(vec![click(112)], vec![(40, Some("Q"))], vec![(101, Some(0)), (112, Some(1)), (140, Some(10))])]
#[case::select_place(vec![click(151)], vec![(40, Some("Z"))], vec![(151, Some(1)), (150, Some(0))])]
fn app_kcf_02_state(
    #[case] clicks: Vec<ButtonClick>,
    #[case] texts: Vec<(i32, Option<&str>)>,
    #[case] buttons: Vec<(i32, Option<i32>)>,
) {
    let mut b = Bindings::default();
    let mut s = KeyConfigSkin::new(timing(0, 0), &[901], &b);
    s.frame(
        1,
        &mut b,
        KeyConfigInput {
            clicks: &clicks,
            ..Default::default()
        },
    );
    assert!(s.option(400) && s.option(901));
    assert!(!s.option(401) && !s.option(402));
    for (id, want) in texts {
        assert_eq!(s.text(id), want, "text {id}");
    }
    for (id, want) in buttons {
        assert_eq!(s.button(id), want, "button {id}");
    }
}

/// APP-KCF-03: 鍵盤のタイマー（タイマー 1 の後。皿は 2 方向で 1 つ、両方離したら離した）
#[rstest]
#[case::key_press(vec![(key_slot(3), true)], vec![(103, Some(0)), (123, None)])]
#[case::key_release(vec![(key_slot(3), true), (key_slot(3), false)], vec![(103, None), (123, Some(0))])]
#[case::scratch(vec![(SCRATCH_UP, true)], vec![(100, Some(0)), (120, None)])]
#[case::scratch_other_held(vec![(SCRATCH_UP, true), (SCRATCH_DOWN, true), (SCRATCH_UP, false)], vec![(100, Some(0)), (120, None)])]
#[case::scratch_both_released(vec![(SCRATCH_UP, true), (SCRATCH_DOWN, true), (SCRATCH_UP, false), (SCRATCH_DOWN, false)], vec![(100, None), (120, Some(0))])]
#[case::start_has_no_timer(vec![(START, true)], vec![(109, None), (110, None)])]
fn app_kcf_03_key_timers(
    #[case] events: Vec<(usize, bool)>,
    #[case] want: Vec<(i32, Option<i64>)>,
) {
    let mut b = Bindings::default();
    let mut s = KeyConfigSkin::new(timing(10, 0), &[], &b);
    // タイマー 1 の前は動かない
    s.frame(
        5,
        &mut b,
        KeyConfigInput {
            slots: &events,
            ..Default::default()
        },
    );
    assert!((100..130).all(|i| s.timer(i).is_none()));
    s.frame(
        20,
        &mut b,
        KeyConfigInput {
            slots: &events,
            ..Default::default()
        },
    );
    for (id, w) in want {
        assert_eq!(s.timer(id), w, "timer {id}");
    }
}

/// APP-KCF-04: 1 フレームの処理（クリック → 割り当て → F1 → F2 → Esc・右クリック）。
/// (クリック, 押したもの, 右クリック, 閉じ始めた後か) → (保存, 1P の鍵盤 1, 文字列 40, 閉じ始めた)
#[rstest]
#[case::assign(vec![click(150)], vec![key("KeyA")], false, false, (true, vec![Some(key("KeyA"))], "A", false))]
#[case::no_place(vec![], vec![key("KeyA")], false, false, (false, vec![Some(key("KeyZ"))], "Z", false))]
#[case::delete(vec![click(150)], vec![key("Delete")], false, false, (true, vec![], "-", false))]
#[case::f1_defaults(vec![], vec![key("F1")], false, false, (true, vec![Some(key("KeyZ"))], "Z", false))]
#[case::f2_clears(vec![], vec![key("F2")], false, false, (false, vec![], "-", false))]
#[case::escape(vec![], vec![key("Escape")], false, false, (false, vec![Some(key("KeyZ"))], "Z", true))]
#[case::right_click(vec![], vec![], true, false, (false, vec![Some(key("KeyZ"))], "Z", true))]
#[case::exiting_ignores_keys(vec![click(150)], vec![key("KeyA")], false, true, (false, vec![Some(key("KeyZ"))], "Z", true))]
#[case::exiting_still_clicks(vec![click(112)], vec![], false, true, (false, vec![Some(key("KeyZ"))], "Q", true))]
fn app_kcf_04_frame(
    #[case] clicks: Vec<ButtonClick>,
    #[case] pressed: Vec<Source>,
    #[case] right_click: bool,
    #[case] exiting: bool,
    #[case] want: (bool, Vec<Option<Source>>, &str, bool),
) {
    let mut b = Bindings {
        analog_scratch_ms: Some(50),
        ..Bindings::default()
    };
    b.set(Side::P1, key_slot(1), 0, Some(key("KeyK")));
    b.set(Side::P1, key_slot(1), 0, Some(key("KeyZ")));
    // F1 を試すために 2P にも置いておく
    b.set(Side::P2, key_slot(1), 0, Some(key("KeyM")));
    let mut s = KeyConfigSkin::new(timing(0, 1000), &[], &b);
    if exiting {
        s.frame(
            1,
            &mut b,
            KeyConfigInput {
                right_click: true,
                ..Default::default()
            },
        );
    }
    let out = s.frame(
        2,
        &mut b,
        KeyConfigInput {
            pressed: &pressed,
            clicks: &clicks,
            right_click,
            slots: &[],
        },
    );
    let (save, key1, text40, exit) = want;
    assert_eq!(out.save, save);
    assert!(!out.close);
    assert_eq!(b.slots[key_slot(1)], key1);
    assert_eq!(s.text(40), Some(text40));
    assert_eq!(s.timer(2).is_some(), exit);
    // F1・F2 は 2P も替え、アナログ皿の設定は残す
    let f = pressed.iter().any(|p| *p == key("F1") || *p == key("F2"));
    assert_eq!(b.p2[key_slot(1)].is_empty(), f);
    assert_eq!(b.analog_scratch_ms, Some(50));
}
