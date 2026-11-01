//! 入力の割り当て（キー設定）。物理入力（キーボードのキー・ゲームパッドのボタンと軸）を
//! 「操作」（皿の 2 方向・鍵盤 1〜7・START・SELECT）に割り当てる。物理入力の名前は表示側が決める文字列
//! （キーボードは W3C UI Events の `code` に倣った名前。例: `KeyZ`・`ShiftLeft`。Windows キーは `SuperLeft`）。

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

/// 1 つの操作に置ける物理入力の数（LR2 の `buttonMap[キー][16]`）
pub const PLACES: usize = 16;

/// 1P・2P
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    P1,
    P2,
}

/// 割り当ての読み方（LR2 `InputToButton` の第 3 引数 player。DOM-INP-05）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reading {
    /// 1P の操作として 1P の割り当てを読む（player 0 の 1P 側）
    P1,
    /// 2P の操作として 2P の割り当てを読む（player 0 の 2P 側）
    P2,
    /// SP のプレイ: 1P の操作として 1P と 2P の割り当てをまとめて読む（player 1）
    Single,
}

/// 操作ごとの物理入力（DOM-INP-01, 02）。位置（0..16）は LR2 の `buttonMap[キー][位置]` と同じで、空いた位置は None
/// （末尾の空きは持たない）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    /// 1P の操作
    pub slots: [Vec<Option<Source>>; SLOTS],
    /// 2P の操作（選曲・キー設定・リザルトでは 2P の操作として、SP のプレイでは 1P の操作として読む。DOM-INP-05）
    pub p2: [Vec<Option<Source>>; SLOTS],
    /// 皿に割り当てた軸をアナログ皿として読む（DOM-INP-04）。None ならボタンとして読む（DOM-INP-03）
    pub analog_scratch_ms: Option<u32>,
}

fn key(name: &str) -> Vec<Option<Source>> {
    vec![Some(Source::Key(name.into()))]
}

impl Default for Bindings {
    /// 既定: 皿 = 左 Shift / 左 Ctrl、鍵盤 = Z S X D C F V、START = Q、SELECT = W。2P は無し
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
            p2: Default::default(),
            analog_scratch_ms: None,
        }
    }
}

impl Bindings {
    fn side(&self, side: Side) -> &[Vec<Option<Source>>; SLOTS] {
        match side {
            Side::P1 => &self.slots,
            Side::P2 => &self.p2,
        }
    }

    /// 位置 `place` の物理入力
    pub fn place(&self, side: Side, slot: usize, place: usize) -> Option<&Source> {
        self.side(side).get(slot)?.get(place)?.as_ref()
    }

    /// 位置 `place`（0..16）に置く（None なら外す）。同じ物理入力をほかの操作・位置に置いていてもそのまま（LR2 と同じ）
    pub fn set(&mut self, side: Side, slot: usize, place: usize, source: Option<Source>) {
        if slot >= SLOTS || place >= PLACES {
            return;
        }
        let v = match side {
            Side::P1 => &mut self.slots[slot],
            Side::P2 => &mut self.p2[slot],
        };
        if v.len() <= place {
            v.resize(place + 1, None);
        }
        v[place] = source;
        while v.last().is_some_and(Option::is_none) {
            v.pop();
        }
    }

    /// 1P・2P の割り当てをすべて外す（アナログ皿の設定は残す）
    pub fn clear_keys(&mut self) {
        self.slots = Default::default();
        self.p2 = Default::default();
    }

    /// 1P の操作 `slot` に置いた物理入力
    pub fn sources(&self, slot: usize) -> impl Iterator<Item = &Source> {
        self.sources_in(Reading::P1, slot)
    }

    /// 物理入力を置いている 1P の操作（いくつでも）
    pub fn slots_of<'a>(&'a self, source: &'a Source) -> impl Iterator<Item = usize> + 'a {
        self.slots_in(Reading::P1, source)
    }

    /// 読み方 `reading` で操作 `slot` に当たる物理入力（DOM-INP-05。SP は 1P の後に 2P）
    pub fn sources_in(&self, reading: Reading, slot: usize) -> impl Iterator<Item = &Source> {
        let (a, b): (&[Option<Source>], &[Option<Source>]) = match reading {
            Reading::P1 => (&self.slots[slot], &[]),
            Reading::P2 => (&self.p2[slot], &[]),
            Reading::Single => (&self.slots[slot], &self.p2[slot]),
        };
        a.iter().chain(b).flatten()
    }

    /// 読み方 `reading` で物理入力が当たる操作（いくつでも）
    pub fn slots_in<'a>(
        &'a self,
        reading: Reading,
        source: &'a Source,
    ) -> impl Iterator<Item = usize> + 'a {
        (0..SLOTS).filter(move |&i| self.sources_in(reading, i).any(|s| s == source))
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
