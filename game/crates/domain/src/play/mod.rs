//! プレイ中のルール：判定・ゲージ・ノーツ位置計算。
//! LR2 / beatoraja でルールが違うので Ruleset をドメインの戦略として持つ。
use crate::chart::{ChartMeta, Lane};

pub mod beatoraja;
mod gauge;
mod judge;
pub mod lr2;
mod lr2_play;
mod timing;
pub use beatoraja::BeatorajaRuleset;
pub use gauge::{Gauge, GaugeType};
pub use judge::{JudgeLane, JudgeWindows, Window};
pub use lr2::Lr2Ruleset;
pub use lr2_play::{JudgeCounts, KeyState, Lr2Play, PlayEvent};
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

/// 描画の「計算」はドメイン：何を・どこに出すかを座標非依存で返す。
/// 実際のピクセル化はプレゼンテーション層（Bevy）の仕事。
#[derive(Debug, Clone)]
pub struct VisibleNote {
    pub lane: Lane,
    pub y: f32, /* 0=判定ライン, 1=レーン上端 */
    pub note_index: usize,
}

#[cfg(test)]
mod tests;
