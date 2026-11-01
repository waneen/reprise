//! 入力の割り当て（キー設定）。物理入力（キーボードのキー・ゲームパッドのボタンと軸）を
//! 「操作」（皿の 2 方向・鍵盤 1〜7・START・SELECT）に割り当てる。物理入力の名前は表示側が決める文字列。

/// 操作の数: 0 = 皿（上）、1 = 皿（下）、2..=8 = 鍵盤 1〜7、9 = START、10 = SELECT
pub const SLOTS: usize = 11;
pub const SCRATCH_UP: usize = 0;
pub const SCRATCH_DOWN: usize = 1;
pub const START: usize = 9;
pub const SELECT: usize = 10;

/// 鍵盤 n（1..=7）の操作番号
pub const fn key_slot(n: usize) -> usize {
    n + 1
}

/// 物理入力
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// キーボードのキー
    Key(String),
    /// ゲームパッドのボタン
    Button(String),
    /// ゲームパッドの軸の正（plus）か負の向き
    Axis { axis: String, plus: bool },
}

/// 操作ごとの物理入力（DOM-INP-01, 02）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    pub slots: [Vec<Source>; SLOTS],
    /// 皿に割り当てた軸をアナログ皿として読む（DOM-INP-04）。None ならボタンとして読む（DOM-INP-03）
    pub analog_scratch_ms: Option<u32>,
}

fn key(name: &str) -> Vec<Source> {
    vec![Source::Key(name.into())]
}

impl Default for Bindings {
    /// 既定: 皿 = 左 Shift / 左 Ctrl、鍵盤 = Z S X D C F V、START = Q、SELECT = W
    fn default() -> Self {
        Self {
            slots: [
                key("ShiftLeft"),
                key("ControlLeft"),
                key("KeyZ"),
                key("KeyS"),
                key("KeyX"),
                key("KeyD"),
                key("KeyC"),
                key("KeyF"),
                key("KeyV"),
                key("KeyQ"),
                key("KeyW"),
            ],
            analog_scratch_ms: None,
        }
    }
}

impl Bindings {
    /// 割り当てる。同じ物理入力は 1 つの操作にしか付かない（ほかの操作からは外す）
    pub fn bind(&mut self, slot: usize, source: Source) {
        for s in self.slots.iter_mut() {
            s.retain(|x| *x != source);
        }
        self.slots[slot].push(source);
    }
    pub fn clear(&mut self, slot: usize) {
        self.slots[slot].clear();
    }
    /// 物理入力が付いている操作
    pub fn slot_of(&self, source: &Source) -> Option<usize> {
        self.slots.iter().position(|s| s.contains(source))
    }
}

/// 軸をボタンとして読む: 0.9 を超えたら正、−0.9 未満なら負が押されている（beatoraja と同じ。DOM-INP-03）
pub fn axis_pressed(value: f32, plus: bool) -> bool {
    if plus {
        value > 0.9
    } else {
        value < -0.9
    }
}

/// アナログ皿（beatoraja の版 1 に倣う。DOM-INP-04）: 軸の値が動いたら動いた向きに回したとし、
/// 止まってから `stop_ms` を過ぎたら離したとする。値は −1..1 で、端で反対側へ回り込む
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnalogScratch {
    old: Option<f32>,
    active: bool,
    /// 正の向きに回している
    plus: bool,
    last_move_ms: i64,
}

impl AnalogScratch {
    /// 今の値から (正の向きを押している, 負の向きを押している) を返す
    pub fn update(&mut self, now_ms: i64, value: f32, stop_ms: u32) -> (bool, bool) {
        let Some(old) = self.old else {
            self.old = Some(value);
            return (false, false);
        };
        if old != value {
            // 回り込みを考えて近い方の向き
            let plus = if old < value {
                value - old <= 1.0 - value + old
            } else {
                old - value > value + 1.0 - old
            };
            self.plus = plus;
            self.active = true;
            self.last_move_ms = now_ms;
            self.old = Some(value);
        } else if self.active && now_ms - self.last_move_ms > stop_ms as i64 {
            self.active = false;
        }
        (self.active && self.plus, self.active && !self.plus)
    }
}

#[cfg(test)]
mod tests;
