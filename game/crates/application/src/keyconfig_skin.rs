//! LR2 のキー設定画面（APP-KCF-*）。キー設定スキン（`#INFORMATION` 種別 8）で描くための状態と、1 フレームの処理。
//! OpenLR2 の `Scene06_Keyconfig.cpp`（`ProcI_Keyconfig`・`ProcS_Keyconfig`）・`LR2_gameloop.cpp`（`ReactInput` のタイマー 100..129 と
//! `SetObjectValue_Button`）・`Main.cpp`（場面の開始・`#STARTINPUT`）・`LR2_skinobject.cpp`（条件 400..402）を仕様として参照した。
//! キーの割り当ての規則そのものは `domain::keyconfig`。
use domain::input::{Bindings, Source, SCRATCH_DOWN, SCRATCH_UP};
use domain::keyconfig::{Assigned, KeyConfig};
use domain::skin::lr2::{ButtonClick, SkinState};
use domain::skin::SceneTiming;
use std::collections::{BTreeMap, BTreeSet};

/// 1 フレームの入力
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyConfigInput<'a> {
    /// このフレームで押された物理入力（押した順。キーボードの Esc・Delete・F1・F2 も含む）
    pub pressed: &'a [Source],
    /// クリックした BUTTON（LR2 のマウスの左ボタンの状態 1 の位置）
    pub clicks: &'a [ButtonClick],
    /// マウスの右ボタンを押した
    pub right_click: bool,
    /// 1P の操作の押下（true）・離し（false）。起きた順
    pub slots: &'a [(usize, bool)],
}

/// 1 フレームの結果
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyConfigOutput {
    /// 割り当てを保存する（LR2 は割り当てたとき・F1 で `keyconfig.xml` に書く）
    pub save: bool,
    /// 画面を閉じて選曲に戻る
    pub close: bool,
}

/// キー設定画面の状態。時刻は画面の開始からの ms
pub struct KeyConfigSkin {
    timing: SceneTiming,
    skin_options: BTreeSet<i32>,
    now: i64,
    starts: BTreeMap<i32, i64>,
    config: KeyConfig,
    /// 文字列 40..=55
    texts: [String; 16],
    /// 皿の 2 方向の押下
    scratch: [bool; 2],
    /// Esc か右クリックで閉じ始めた（タイマー 2）
    exiting: bool,
}

fn key(name: &str) -> Source {
    Source::Key(name.into())
}

impl KeyConfigSkin {
    /// APP-KCF-01, 02: 画面の開始（タイマー 0）。キー設定は DOM-KCF-03 の値で、文字列を作る
    pub fn new(timing: SceneTiming, skin_options: &[i32], bindings: &Bindings) -> Self {
        let mut s = Self {
            timing,
            skin_options: skin_options.iter().copied().collect(),
            now: 0,
            starts: BTreeMap::new(),
            config: KeyConfig::default(),
            texts: Default::default(),
            scratch: [false; 2],
            exiting: false,
        };
        s.starts.insert(0, 0);
        s.config.texts(bindings, &mut s.texts);
        s
    }

    /// キー設定の状態
    pub fn config(&self) -> &KeyConfig {
        &self.config
    }

    /// APP-KCF-04: 1 フレーム進める（LR2 の順: 入力のタイマーと BUTTON のクリック → 割り当て → F1 → F2 → Esc・右クリック）。
    /// `bindings` は今の割り当て（LR2 の `config.input.buttonMap`。この画面の入力もこれで読む）
    pub fn frame(
        &mut self,
        now_ms: i64,
        bindings: &mut Bindings,
        input: KeyConfigInput<'_>,
    ) -> KeyConfigOutput {
        let mut out = KeyConfigOutput::default();
        self.now = now_ms;
        // APP-KCF-01: `#STARTINPUT` を過ぎたらタイマー 1 と 11
        if now_ms > self.timing.start_input as i64 && !self.starts.contains_key(&1) {
            self.starts.insert(1, now_ms);
            self.starts.insert(11, now_ms);
        }
        // APP-KCF-01: 閉じ始めてから `#FADEOUT`（0 なら次のフレーム）で閉じる
        if self.exiting {
            let t = self.timer(2).unwrap_or(0);
            if self.timing.fade_out == 0 || t > self.timing.fade_out as i64 {
                out.close = true;
            }
        }
        // APP-KCF-03: タイマー 1 の後は押した・離した鍵盤のタイマー
        if self.starts.contains_key(&1) {
            for &(slot, down) in input.slots {
                self.key_timer(slot, down);
            }
        }
        // BUTTON のクリック（閉じ始めた後も受ける）
        let mut changed = false;
        for c in input.clicks {
            changed |= self.config.click(c.action, c.delta);
        }
        if self.exiting {
            if changed {
                self.config.texts(bindings, &mut self.texts);
            }
            return out;
        }
        if let Assigned::Set(_) = self.config.assign(bindings, input.pressed) {
            out.save = true;
            changed = true;
        }
        // F1: 既定の割り当て（保存する）、F2: すべて外す（保存はしない）
        if input.pressed.contains(&key("F1")) {
            let analog = bindings.analog_scratch_ms;
            *bindings = Bindings {
                analog_scratch_ms: analog,
                ..Bindings::default()
            };
            out.save = true;
            changed = true;
        }
        if input.pressed.contains(&key("F2")) {
            bindings.clear_keys();
            changed = true;
        }
        if changed {
            self.config.texts(bindings, &mut self.texts);
        }
        if input.pressed.contains(&key("Escape")) || input.right_click {
            self.exiting = true;
            self.starts.insert(2, now_ms);
        }
        out
    }

    /// 1P の操作のタイマー（皿は 2 方向で 1 つ。0 = 皿、1..=7 = 鍵盤）
    fn key_timer(&mut self, slot: usize, down: bool) {
        let lane = match slot {
            SCRATCH_UP | SCRATCH_DOWN => {
                self.scratch[slot] = down;
                if !down && self.scratch.iter().any(|&h| h) {
                    // もう片方を押している間は離していない
                    return;
                }
                0
            }
            2..=8 => slot as i32 - 1,
            _ => return,
        };
        let (on, off) = if down { (100, 120) } else { (120, 100) };
        self.starts.insert(on + lane, self.now);
        self.starts.remove(&(off + lane));
    }
}

impl SkinState for KeyConfigSkin {
    fn timer(&self, id: i32) -> Option<i64> {
        self.starts.get(&id).map(|s| self.now - s)
    }

    /// APP-KCF-02: 条件 400（7 鍵のキー設定）とスキンの条件
    fn option(&self, id: i32) -> bool {
        id == 400 || self.skin_options.contains(&id)
    }

    fn number(&self, _id: i32) -> Option<i64> {
        None
    }

    fn bar(&self, _kind: i32) -> f64 {
        0.0
    }

    fn slider(&self, _kind: i32) -> f64 {
        0.0
    }

    fn turntable(&self, _side: i32) -> f32 {
        0.0
    }

    /// APP-KCF-02: 文字列 40..=55
    fn text(&self, id: i32) -> Option<&str> {
        let i = usize::try_from(id - 40).ok()?;
        self.texts.get(i).map(String::as_str)
    }

    /// APP-KCF-02: BUTTON の状態値（DOM-KCF-07）
    fn button(&self, action: i32) -> Option<i32> {
        self.config.button_value(action)
    }
}

#[cfg(test)]
mod tests;
