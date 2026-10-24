//! 記録。プレイヤー間移行の共通語彙。
use crate::chart::ChartHash;
use crate::play::RulesetId;

/// 各プレイヤーのランプを写像する共通語彙（細分はアダプタ側で対応付け）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClearLamp {
    NoPlay,
    Failed,
    AssistEasy,
    LightAssistEasy,
    Easy,
    Normal,
    Hard,
    ExHard,
    FullCombo,
    Perfect,
    Max,
}

#[derive(Debug, Clone)]
pub struct ScoreRecord {
    pub chart: ChartHash,
    /// どのルールで出した記録か（LR2判定とbeatoraja判定のスコアは同列にしない）
    pub ruleset: RulesetId,
    pub lamp: ClearLamp,
    pub ex_score: u32,
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub max_combo: u32,
    pub min_bp: Option<u32>,
    pub play_count: u32,
    pub origin: ScoreOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreOrigin {
    Native,
    ImportedFrom(String),
}

impl ScoreRecord {
    /// 取り込み時のマージ規則：項目ごとに良い方を残す（ドメインのルール）
    pub fn merge_best(&mut self, other: &ScoreRecord) {
        debug_assert_eq!(self.chart, other.chart);
        self.lamp = self.lamp.max(other.lamp);
        if other.ex_score > self.ex_score {
            self.ex_score = other.ex_score;
            (self.pgreat, self.great, self.good, self.bad, self.poor) =
                (other.pgreat, other.great, other.good, other.bad, other.poor);
        }
        self.max_combo = self.max_combo.max(other.max_combo);
        self.min_bp = match (self.min_bp, other.min_bp) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.play_count += other.play_count;
    }
}
