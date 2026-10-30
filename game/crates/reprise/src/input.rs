//! キーボードとゲームパッドの入力を、割り当て（`domain::input::Bindings`）に従って操作
//! （皿の 2 方向・鍵盤 1〜7・START・SELECT）の押下・離しにする。毎フレーム、場面の処理より先に動く
use bevy::input::gamepad::GamepadButtonStateChangedEvent;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::input::{
    axis_pressed, AnalogScratch, Bindings, Source, SCRATCH_DOWN, SCRATCH_UP, SLOTS,
};
use std::collections::{HashMap, HashSet};

/// 読む軸（名前の付いた軸と、名前の無い軸の 0..16 番）
fn axes() -> impl Iterator<Item = GamepadAxis> {
    GamepadAxis::all()
        .into_iter()
        .chain((0..16).map(GamepadAxis::Other))
}

#[derive(Resource)]
pub struct InputMap {
    pub bindings: Bindings,
    /// 押されている物理入力（軸は向きごと）
    held: HashSet<Source>,
    analog: HashMap<String, AnalogScratch>,
    slot_held: [bool; SLOTS],
    start: Instant,
}

impl InputMap {
    pub fn new(bindings: Bindings) -> Self {
        Self {
            bindings,
            held: HashSet::new(),
            analog: HashMap::new(),
            slot_held: [false; SLOTS],
            start: Instant::now(),
        }
    }
}

/// このフレームの操作
#[derive(Resource, Default)]
pub struct SlotInput {
    /// 操作の押下（true）・離し（false）を起きた順に
    pub events: Vec<(usize, bool)>,
    pub held: [bool; SLOTS],
    pub pressed: [bool; SLOTS],
    /// このフレームで最後に押された物理入力（キー設定用）
    pub last_pressed: Option<Source>,
}

pub fn read_input(
    mut keys: MessageReader<KeyboardInput>,
    mut buttons: MessageReader<GamepadButtonStateChangedEvent>,
    pads: Query<&Gamepad>,
    mut map: ResMut<InputMap>,
    mut out: ResMut<SlotInput>,
) {
    let map = &mut *map;
    let now = map.start.elapsed().as_millis() as i64;
    let mut changes: Vec<(Source, bool)> = vec![];
    for k in keys.read().filter(|k| !k.repeat) {
        let src = Source::Key(format!("{:?}", k.key_code));
        changes.push((src, k.state == ButtonState::Pressed));
    }
    for b in buttons.read() {
        let src = Source::Button(format!("{:?}", b.button));
        changes.push((src, b.state == ButtonState::Pressed));
    }
    // 軸は値から向きごとの押下を求め、前のフレームと比べる
    for pad in &pads {
        for axis in axes() {
            let Some(v) = pad.get(axis) else { continue };
            let name = format!("{axis:?}");
            let plus = Source::Axis {
                axis: name.clone(),
                plus: true,
            };
            let minus = Source::Axis {
                axis: name.clone(),
                plus: false,
            };
            let scratch = |s: &Source| {
                matches!(
                    map.bindings.slot_of(s),
                    Some(SCRATCH_UP) | Some(SCRATCH_DOWN)
                )
            };
            let (p, m) = match map.bindings.analog_scratch_ms {
                Some(ms) if scratch(&plus) || scratch(&minus) => {
                    map.analog.entry(name).or_default().update(now, v, ms)
                }
                _ => (axis_pressed(v, true), axis_pressed(v, false)),
            };
            for (src, down) in [(plus, p), (minus, m)] {
                if map.held.contains(&src) != down {
                    changes.push((src, down));
                }
            }
        }
    }
    out.events.clear();
    out.pressed = [false; SLOTS];
    out.last_pressed = None;
    for (src, down) in changes {
        if down {
            map.held.insert(src.clone());
            out.last_pressed = Some(src.clone());
        } else {
            map.held.remove(&src);
        }
        let Some(slot) = map.bindings.slot_of(&src) else {
            continue;
        };
        let now_held = map.bindings.slots[slot]
            .iter()
            .any(|s| map.held.contains(s));
        if now_held != map.slot_held[slot] {
            map.slot_held[slot] = now_held;
            out.events.push((slot, now_held));
            if now_held {
                out.pressed[slot] = true;
            }
        }
    }
    out.held = map.slot_held;
}
