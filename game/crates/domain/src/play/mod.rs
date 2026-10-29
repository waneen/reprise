//! プレイ中のルール：判定・ゲージ・ノーツ位置計算。
//! LR2 / beatoraja でルールが違うので Ruleset をドメインの戦略として持つ。
use crate::chart::Lane;

mod timing;
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

/// 判定・ゲージのルール。値の正しさはOpenLR2/実機との差分テストで担保する。
pub trait Ruleset {
    fn id(&self) -> RulesetId;
    /// 押下時刻とノーツ時刻の差(µs, 正=遅い)から判定
    fn judge(&self, delta_us: i64, judge_rank: i32) -> Option<Judge>;
    fn gauge_delta(&self, gauge: GaugeType, judge: Judge, total: f64, notes: usize) -> f64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaugeType {
    AssistEasy,
    Easy,
    Groove,
    Hard,
    ExHard,
    Hazard,
}

/// 描画の「計算」はドメイン：何を・どこに出すかを座標非依存で返す。
/// 実際のピクセル化はプレゼンテーション層（Bevy）の仕事。
#[derive(Debug, Clone)]
pub struct VisibleNote {
    pub lane: Lane,
    pub y: f32, /* 0=判定ライン, 1=レーン上端 */
    pub note_index: usize,
}

pub struct PlayState<R: Ruleset> {
    pub ruleset: R,
    pub combo: u32,
    pub gauge: f64,
}
