//! キーボードとゲームパッドの入力を、割り当て（`domain::input::Bindings`）に従って操作
//! （皿の 2 方向・鍵盤 1〜7・START・SELECT）の押下・離しにする。毎フレーム、場面の処理より先に動く。
//! LR2 の `InputToButton` と同じく、1P・2P の操作（選曲・キー設定・リザルト）と、SP のプレイ用に 1P と 2P の割り当てを
//! まとめた 1P の操作の 3 通りに読む（DOM-INP-05）
use bevy::input::gamepad::GamepadButtonStateChangedEvent;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use domain::input::{
    axis_pressed, AnalogScratch, Bindings, Reading, Source, SCRATCH_DOWN, SCRATCH_UP, SLOTS,
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
    /// 読み方（[`READINGS`] の順）ごとの操作の押下
    slot_held: [[bool; SLOTS]; 3],
    start: Instant,
}

impl InputMap {
    pub fn new(bindings: Bindings) -> Self {
        Self {
            bindings,
            held: HashSet::new(),
            analog: HashMap::new(),
            slot_held: [[false; SLOTS]; 3],
            start: Instant::now(),
        }
    }
}

/// 割り当ての読み方（[`SlotInput`] の `p1`・`p2`・`single` の順）
const READINGS: [Reading; 3] = [Reading::P1, Reading::P2, Reading::Single];

/// 1 つの読み方でのこのフレームの操作
#[derive(Default)]
pub struct Slots {
    /// 操作の押下（true）・離し（false）を起きた順に
    pub events: Vec<(usize, bool)>,
    pub held: [bool; SLOTS],
    pub pressed: [bool; SLOTS],
}

/// このフレームの操作
#[derive(Resource, Default)]
pub struct SlotInput {
    /// 1P の操作（1P の割り当て）
    pub p1: Slots,
    /// 2P の操作（2P の割り当て）
    pub p2: Slots,
    /// SP のプレイの 1P の操作（1P と 2P の割り当てをまとめて読む。LR2 `InputToButton` の player 1）
    pub single: Slots,
    /// このフレームで押された物理入力（押した順。割り当ての無いものも。キー設定で使う）
    pub sources: Vec<Source>,
}

impl SlotInput {
    /// 1P か 2P の操作 `slot` をこのフレームで押した（LR2 の選曲・リザルトの `p1_buttonInput[n] == 1 || p2_buttonInput[n] == 1`）
    pub fn pressed_any(&self, slot: usize) -> bool {
        self.p1.pressed[slot] || self.p2.pressed[slot]
    }
    /// 1P か 2P の操作 `slot` を押している
    pub fn held_any(&self, slot: usize) -> bool {
        self.p1.held[slot] || self.p2.held[slot]
    }
    /// 1P か 2P の操作 `slot` をこのフレームで離した（どちらも押していない）
    pub fn released_any(&self, slot: usize) -> bool {
        let up = |s: &Slots| s.events.contains(&(slot, false));
        (up(&self.p1) || up(&self.p2)) && !self.held_any(slot)
    }
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
                map.bindings
                    .slots_in(Reading::Single, s)
                    .any(|i| i == SCRATCH_UP || i == SCRATCH_DOWN)
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
    let out = &mut *out;
    let sides = [&mut out.p1, &mut out.p2, &mut out.single];
    for o in sides {
        o.events.clear();
        o.pressed = [false; SLOTS];
    }
    out.sources.clear();
    for (src, down) in changes {
        if down {
            map.held.insert(src.clone());
            out.sources.push(src.clone());
        } else {
            map.held.remove(&src);
        }
        // 同じ物理入力をいくつかの操作に置いてもよい（LR2 と同じ）
        for (r, reading) in READINGS.into_iter().enumerate() {
            let o = match r {
                0 => &mut out.p1,
                1 => &mut out.p2,
                _ => &mut out.single,
            };
            let slots: Vec<usize> = map.bindings.slots_in(reading, &src).collect();
            for slot in slots {
                let now_held = map
                    .bindings
                    .sources_in(reading, slot)
                    .any(|s| map.held.contains(s));
                if now_held != map.slot_held[r][slot] {
                    map.slot_held[r][slot] = now_held;
                    o.events.push((slot, now_held));
                    if now_held {
                        o.pressed[slot] = true;
                    }
                }
            }
        }
    }
    out.p1.held = map.slot_held[0];
    out.p2.held = map.slot_held[1];
    out.single.held = map.slot_held[2];
}
