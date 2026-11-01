//! プレイ中のルール：判定・ゲージ・プレイ進行。
//! LR2 / beatoraja でルールが違うので Ruleset をドメインの戦略として持つ。
use crate::chart::ChartMeta;

pub mod beatoraja;
mod gauge;
mod judge;
pub mod lr2;
mod lr2_play;
pub mod scroll;
mod timing;
pub use beatoraja::BeatorajaRuleset;
pub use gauge::{Gauge, GaugeType};
pub use judge::{JudgeLane, JudgeWindows, Window};
pub use lr2::Lr2Ruleset;
pub use lr2_play::{JudgeCounts, KeyState, Lr2Play, PendingNote, PlayEvent};
pub use scroll::HsFix;
pub use timing::TimingMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Judge {
    PGreat,
    Great,
    Good,
    Bad,
    Poor,
    EmptyPoor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RulesetId {
    Lr2,
    Beatoraja,
}

/// 判定・ゲージのルール。実装は `Lr2Ruleset`（OpenLR2 準拠）と `BeatorajaRuleset`（7 鍵）
pub trait Ruleset {
    fn id(&self) -> RulesetId;
    /// 譜面の `#RANK`（bmson は `judge_rank`）に応じた判定幅
    fn judge_windows(&self, meta: &ChartMeta, lane: JudgeLane) -> JudgeWindows;
    /// 譜面の TOTAL とノーツ数から初期状態のゲージを作る。この Ruleset に無い種類は `None`
    fn gauge(&self, ty: GaugeType, meta: &ChartMeta, notes: usize) -> Option<Gauge>;
}

#[cfg(test)]
mod tests;
