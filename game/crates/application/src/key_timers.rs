//! 場面によらない鍵盤のタイマー（LR2 `ReactInput` のタイマー 100..107・120..127。OpenLR2 `LR2_gameloop.cpp`）。
//! キー設定画面（APP-KCF-03）とリザルト画面（APP-RES-04）で使う
use domain::input::{SCRATCH_DOWN, SCRATCH_UP};
use std::collections::BTreeMap;

/// 1P の皿・鍵盤のタイマー。皿は 2 方向で 1 つ（0 = 皿、1..=7 = 鍵盤）
#[derive(Debug, Default)]
pub(crate) struct KeyTimers {
    /// 皿の 2 方向の押下
    scratch: [bool; 2],
}

impl KeyTimers {
    /// 押したらタイマー 100+n を始めて 120+n を止め、離したら逆（皿はどちらかを押せば押した、両方離したら離した）
    pub(crate) fn apply(
        &mut self,
        starts: &mut BTreeMap<i32, i64>,
        now: i64,
        slot: usize,
        down: bool,
    ) {
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
        starts.insert(on + lane, now);
        starts.remove(&(off + lane));
    }
}
