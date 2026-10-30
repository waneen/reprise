//! LR2 のキー設定画面の規則（DOM-KCF-*）。OpenLR2 の `Scene06_Keyconfig.cpp`（`ProcI_Keyconfig`・`ProcS_Keyconfig`・
//! `GetKeyIDname`・`ConfigButtonToKeyID7`/`ConfigButtonFromKeyID7`）・`LR2_skinobject.cpp`（`SetObjectValue_Button` の
//! 100..157、条件 400..402）・`Main.cpp`（画面に入ったときの値）・`En_input.cpp`（`InputToButton` のキー番号）を仕様として参照した
//! （コードは転載していない）。
//!
//! LR2 は割り当てをキー番号（`buttonMap[キー番号][位置]`）で持つ。1..7 = 1P の鍵盤、10/11 = 1P の皿、12 = START、13 = SELECT、
//! 21..27・30..33 = 2P の同じもの（8・9 は 9 鍵の鍵盤）。キー設定の 7 鍵の並び（`config_button`）は 1..22 がこれらを順に指す。
use crate::input::{key_slot, Bindings, Side, Source, SCRATCH_DOWN, SCRATCH_UP, SELECT, START};

/// DOM-KCF-01: LR2 のキー番号が指す操作。reprise に無いもの（9 鍵の 8・9 など）は None
pub fn key_id_slot(key_id: i32) -> Option<(Side, usize)> {
    let (side, k) = match key_id {
        1..=19 => (Side::P1, key_id),
        21..=39 => (Side::P2, key_id - 20),
        _ => return None,
    };
    let slot = match k {
        1..=7 => key_slot(k as usize),
        10 => SCRATCH_UP,
        11 => SCRATCH_DOWN,
        12 => START,
        13 => SELECT,
        _ => return None,
    };
    Some((side, slot))
}

/// 7 鍵の並び 1..=22 のキー番号
const ORDER7: [i32; 22] = [
    1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 13, 21, 22, 23, 24, 25, 26, 27, 30, 31, 32, 33,
];

/// DOM-KCF-02: 7 鍵の並びの番号 → キー番号（LR2 `ConfigButtonToKeyID7`。範囲外は 0）
pub fn button_to_key_id7(button: i32) -> i32 {
    if (1..=22).contains(&button) {
        ORDER7[button as usize - 1]
    } else {
        0
    }
}

/// DOM-KCF-02: キー番号 → 7 鍵の並びの番号（LR2 `ConfigButtonFromKeyID7`。並びに無ければ 0）
pub fn button_from_key_id7(key_id: i32) -> i32 {
    ORDER7
        .iter()
        .position(|&k| k == key_id)
        .map_or(0, |i| i as i32 + 1)
}

/// キー設定の画面の状態（LR2 の `KeyInput.config_*`）。鍵盤の種類（`config_keymode`）は 7 鍵だけ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyConfig {
    /// 7 鍵の並びの番号（`config_button`。0..=22）
    pub button: i32,
    /// 割り当てを変えるキー番号（`config_button_inMap`。0 なら変えない）
    pub key_id: i32,
    /// 割り当てる位置（`config_key`。−1 なら割り当てない）
    pub place: i32,
}

/// 押された物理入力に対してしたこと
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assigned {
    /// 何もしない
    Nothing,
    /// 割り当てた（None は外した）。LR2 はここで保存する
    Set(Option<Source>),
}

impl Default for KeyConfig {
    /// DOM-KCF-03: 画面に入ったとき（LR2 `Main.cpp`）: 並び 1・キー番号 1（1P の鍵盤 1）・位置なし
    fn default() -> Self {
        Self {
            button: 1,
            key_id: 1,
            place: -1,
        }
    }
}

impl KeyConfig {
    /// DOM-KCF-04: BUTTON のクリック（`delta` は +1 / −1）。100..=139 はキー番号 n − 100 を選び（並びはそのキー番号の番号）、
    /// 140 は並びを ±1（0..=22 で回る）してそのキー番号を、150..=157 は位置 n − 150 を選ぶ。ほか（9 鍵・5 鍵の 141/142、
    /// 鍵盤の種類の 143 など）は何もしない。何か選んだら true
    pub fn click(&mut self, action: i32, delta: i32) -> bool {
        match action {
            100..=139 => {
                self.key_id = action - 100;
                self.button = button_from_key_id7(self.key_id);
            }
            140 => {
                let b = self.button + delta;
                self.button = if b > 22 {
                    0
                } else if b < 0 {
                    22
                } else {
                    b
                };
                self.key_id = button_to_key_id7(self.button);
            }
            150..=157 => self.place = action - 150,
            _ => return false,
        }
        true
    }

    /// DOM-KCF-05: このフレームで押された物理入力（押した順）で割り当てる（LR2 `ProcI_Keyconfig`）。位置を選んでいて
    /// キー番号が 0 でなく reprise にある操作のとき、最初に押されたものが禁止キー（Esc・Enter・矢印・F1..F12）でなければ、
    /// Delete も押されていれば外し、そうでなければその物理入力を置く
    pub fn assign(&self, bindings: &mut Bindings, pressed: &[Source]) -> Assigned {
        if self.place < 0 || self.key_id <= 0 {
            return Assigned::Nothing;
        }
        let Some((side, slot)) = key_id_slot(self.key_id) else {
            return Assigned::Nothing;
        };
        let Some(first) = pressed.first() else {
            return Assigned::Nothing;
        };
        if banned(first) {
            return Assigned::Nothing;
        }
        let delete = Source::Key("Delete".into());
        let src = (!pressed.contains(&delete)).then(|| first.clone());
        bindings.set(side, slot, self.place as usize, src.clone());
        Assigned::Set(src)
    }

    /// DOM-KCF-06: 文字列 40..=55（LR2 `ProcS_Keyconfig`）。40..=47 は `-` にし、キー番号が 0 でなければ 40 + 位置 を
    /// 位置 0..16 の物理入力の名前（[`lr2_name`]。無ければ `-`）にする。`texts[i]` が文字列 40 + i で、
    /// キー番号が 0 のときの 48..=55 は前のまま
    pub fn texts(&self, bindings: &Bindings, texts: &mut [String; 16]) {
        for t in texts.iter_mut().take(8) {
            *t = "-".into();
        }
        if self.key_id <= 0 {
            return;
        }
        let slot = key_id_slot(self.key_id);
        for (i, t) in texts.iter_mut().enumerate() {
            *t = slot
                .and_then(|(side, s)| bindings.place(side, s, i))
                .map_or_else(|| "-".into(), lr2_name);
        }
    }

    /// DOM-KCF-07: BUTTON の状態値（LR2 `ButtonByInput` のコマ）。100..=139 は選んだキー番号なら 1、140..=142 は並びの番号、
    /// 143 は鍵盤の種類（7 鍵 = 0）、150..=157 は選んだ位置なら 1。ほかは分からない
    pub fn button_value(&self, action: i32) -> Option<i32> {
        match action {
            100..=139 => Some((self.key_id == action - 100) as i32),
            140..=142 => Some(self.button),
            143 => Some(0),
            150..=157 => Some((self.place == action - 150) as i32),
            _ => None,
        }
    }
}

/// 割り当てない（押しても何もしない）キー（LR2 `bannedInput`）
fn banned(s: &Source) -> bool {
    let Source::Key(k) = s else {
        return false;
    };
    matches!(
        k.as_str(),
        "Escape" | "Enter" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
    ) || k
        .strip_prefix('F')
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| (1..=12).contains(&n))
}

/// キーボードのキーの LR2 の名前（LR2 `GetKeyIDname`。LR2 の DirectInput のキー番号はキーの位置（スキャンコード）で、
/// W3C の `code` も位置で決まるので、同じ位置のキーを対応させる）
const KEY_NAMES: &[(&str, &str)] = &[
    ("Escape", "ESCAPE"),
    ("Digit1", "1"),
    ("Digit2", "2"),
    ("Digit3", "3"),
    ("Digit4", "4"),
    ("Digit5", "5"),
    ("Digit6", "6"),
    ("Digit7", "7"),
    ("Digit8", "8"),
    ("Digit9", "9"),
    ("Digit0", "0"),
    ("Minus", "MINUS"),
    ("Equal", "EQUALS"),
    ("Backspace", "BACK"),
    ("Tab", "TAB"),
    ("KeyQ", "Q"),
    ("KeyW", "W"),
    ("KeyE", "E"),
    ("KeyR", "R"),
    ("KeyT", "T"),
    ("KeyY", "Y"),
    ("KeyU", "U"),
    ("KeyI", "I"),
    ("KeyO", "O"),
    ("KeyP", "P"),
    ("BracketLeft", "LBRACKET"),
    ("BracketRight", "RBRACKET"),
    ("Enter", "RETURN"),
    ("ControlLeft", "LCONTROL"),
    ("KeyA", "A"),
    ("KeyS", "S"),
    ("KeyD", "D"),
    ("KeyF", "F"),
    ("KeyG", "G"),
    ("KeyH", "H"),
    ("KeyJ", "J"),
    ("KeyK", "K"),
    // LR2 はこれだけ小文字
    ("KeyL", "l"),
    ("Semicolon", "SEMICOLON"),
    ("Quote", "APOSTROPHE"),
    ("Backquote", "GRAVE"),
    ("ShiftLeft", "LSHIFT"),
    ("Backslash", "BACKSLASH"),
    ("KeyZ", "Z"),
    ("KeyX", "X"),
    ("KeyC", "C"),
    ("KeyV", "V"),
    ("KeyB", "B"),
    ("KeyN", "N"),
    ("KeyM", "M"),
    ("Comma", "COMMA"),
    ("Period", "PERIOD"),
    ("Slash", "SLASH"),
    ("ShiftRight", "RSHIFT"),
    ("NumpadMultiply", "MULTIPLY"),
    ("AltLeft", "LMENU"),
    ("Space", "SPACE"),
    ("CapsLock", "CAPITAL"),
    ("F1", "F1"),
    ("F2", "F2"),
    ("F3", "F3"),
    ("F4", "F4"),
    ("F5", "F5"),
    ("F6", "F6"),
    ("F7", "F7"),
    ("F8", "F8"),
    ("F9", "F9"),
    ("F10", "F10"),
    ("NumLock", "NUMLOCK"),
    ("ScrollLock", "SCROLL"),
    ("Numpad7", "NUMPAD7"),
    ("Numpad8", "NUMPAD8"),
    ("Numpad9", "NUMPAD9"),
    ("NumpadSubtract", "SUBTRACT"),
    ("Numpad4", "NUMPAD4"),
    ("Numpad5", "NUMPAD5"),
    ("Numpad6", "NUMPAD6"),
    ("NumpadAdd", "ADD"),
    ("Numpad1", "NUMPAD1"),
    ("Numpad2", "NUMPAD2"),
    ("Numpad3", "NUMPAD3"),
    ("Numpad0", "NUMPAD0"),
    ("NumpadDecimal", "DECIMAL"),
    ("IntlBackslash", "OEM_102"),
    ("F11", "F11"),
    ("F12", "F12"),
    ("F13", "F13"),
    ("F14", "F14"),
    ("F15", "F15"),
    ("KanaMode", "KANA"),
    ("IntlRo", "ABNT_C1"),
    ("Convert", "CONVERT"),
    ("NonConvert", "NOCONVERT"),
    ("IntlYen", "YEN"),
    ("MediaTrackPrevious", "PREVTRACK"),
    ("MediaTrackNext", "NEXTTRACK"),
    ("NumpadEnter", "NUMPADENTER"),
    ("ControlRight", "RCONTROL"),
    ("AudioVolumeMute", "MUTE"),
    ("LaunchApp2", "CALCULATOR"),
    ("MediaPlayPause", "PLAYPAUSE"),
    ("MediaStop", "MEDIASTOP"),
    ("AudioVolumeDown", "VOLUMEDOWN"),
    ("AudioVolumeUp", "VOLUMEUP"),
    ("BrowserHome", "WEBHOME"),
    ("NumpadDivide", "DIVIDE"),
    ("PrintScreen", "SYSRQ"),
    ("AltRight", "RMENU"),
    ("Pause", "PAUSE"),
    ("Home", "HOME"),
    ("ArrowUp", "UP"),
    ("PageUp", "PRIOR"),
    ("ArrowLeft", "LEFT"),
    ("ArrowRight", "RIGHT"),
    ("End", "END"),
    ("ArrowDown", "DOWN"),
    ("PageDown", "NEXT"),
    ("Insert", "INSERT"),
    ("Delete", "DELETE"),
    ("SuperLeft", "LWIN"),
    ("SuperRight", "RWIN"),
    ("ContextMenu", "APPS"),
    ("Power", "POWER"),
    ("Sleep", "SLEEP"),
    ("WakeUp", "WAKE"),
    ("BrowserSearch", "WEBSEARCH"),
    ("BrowserFavorites", "WEBFAVORITES"),
    ("BrowserRefresh", "WEBREFRESH"),
    ("BrowserStop", "WEBSTOP"),
    ("BrowserForward", "WEBFORWARD"),
    ("BrowserBack", "WEBBACK"),
    ("LaunchApp1", "MYCOMPUTER"),
    ("LaunchMail", "MAIL"),
    ("MediaSelect", "MEDIASELECT"),
];

/// DOM-KCF-08: 物理入力の表示名。キーボードは LR2 の名前（LR2 に無いキーは reprise の名前のまま）、
/// ゲームパッドのボタンは reprise の名前、軸は 名前 + `+` / `-`（LR2 のパッドの番号の振り方とは対応させられないため）
pub fn lr2_name(s: &Source) -> String {
    match s {
        Source::Key(k) => KEY_NAMES
            .iter()
            .find(|(w3c, _)| w3c == k)
            .map_or_else(|| k.clone(), |(_, n)| n.to_string()),
        Source::Button(b) => b.clone(),
        Source::Axis { axis, plus } => format!("{axis}{}", if *plus { "+" } else { "-" }),
    }
}

#[cfg(test)]
mod tests;
