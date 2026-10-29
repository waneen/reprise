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

#[derive(Debug, Clone, PartialEq)]
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
    /// 取り込み時のマージ規則：項目ごとに良い方を残す（DOM-SCR-01..05）
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};

    #[fixture]
    fn base() -> ScoreRecord {
        ScoreRecord {
            chart: ChartHash {
                md5: [1; 16],
                sha256: None,
            },
            ruleset: RulesetId::Lr2,
            lamp: ClearLamp::Easy,
            ex_score: 1000,
            pgreat: 400,
            great: 200,
            good: 10,
            bad: 5,
            poor: 3,
            max_combo: 300,
            min_bp: Some(8),
            play_count: 10,
            origin: ScoreOrigin::Native,
        }
    }

    /// DOM-SCR-01: ランプは良い方
    #[rstest]
    #[case::imported_better(ClearLamp::Hard, ClearLamp::Hard)]
    #[case::imported_worse(ClearLamp::Failed, ClearLamp::Easy)]
    #[case::same(ClearLamp::Easy, ClearLamp::Easy)]
    fn dom_scr_01_lamp_takes_best(
        base: ScoreRecord,
        #[case] other: ClearLamp,
        #[case] expected: ClearLamp,
    ) {
        let mut cur = base.clone();
        cur.merge_best(&ScoreRecord {
            lamp: other,
            ..base
        });
        assert_eq!(cur.lamp, expected);
    }

    /// DOM-SCR-02: EX スコアが高い方の内訳
    #[rstest]
    #[case::imported_higher(1100, (1100, 450))]
    #[case::imported_lower(900, (1000, 400))]
    #[case::tie_keeps_current(1000, (1000, 400))]
    fn dom_scr_02_judges_follow_best_ex_score(
        base: ScoreRecord,
        #[case] ex: u32,
        #[case] expected: (u32, u32),
    ) {
        let mut cur = base.clone();
        cur.merge_best(&ScoreRecord {
            ex_score: ex,
            pgreat: 450,
            ..base
        });
        assert_eq!((cur.ex_score, cur.pgreat), expected);
    }

    /// DOM-SCR-03: 最大コンボは大きい方
    #[rstest]
    #[case::imported_bigger(500, 500)]
    #[case::imported_smaller(100, 300)]
    fn dom_scr_03_max_combo_takes_max(
        base: ScoreRecord,
        #[case] other: u32,
        #[case] expected: u32,
    ) {
        let mut cur = base.clone();
        cur.merge_best(&ScoreRecord {
            max_combo: other,
            ..base
        });
        assert_eq!(cur.max_combo, expected);
    }

    /// DOM-SCR-04: 最小 BP は小さい方、片方だけならそれ
    #[rstest]
    #[case::both_smaller_wins(Some(8), Some(3), Some(3))]
    #[case::both_current_smaller(Some(2), Some(3), Some(2))]
    #[case::only_current(Some(8), None, Some(8))]
    #[case::only_imported(None, Some(5), Some(5))]
    #[case::neither(None, None, None)]
    fn dom_scr_04_min_bp_takes_min(
        base: ScoreRecord,
        #[case] current: Option<u32>,
        #[case] other: Option<u32>,
        #[case] expected: Option<u32>,
    ) {
        let mut cur = ScoreRecord {
            min_bp: current,
            ..base.clone()
        };
        cur.merge_best(&ScoreRecord {
            min_bp: other,
            ..base
        });
        assert_eq!(cur.min_bp, expected);
    }

    /// DOM-SCR-05: プレイ回数は合算
    #[rstest]
    #[case::sum(10, 5, 15)]
    #[case::zero(10, 0, 10)]
    fn dom_scr_05_play_count_summed(
        base: ScoreRecord,
        #[case] current: u32,
        #[case] other: u32,
        #[case] expected: u32,
    ) {
        let mut cur = ScoreRecord {
            play_count: current,
            ..base.clone()
        };
        cur.merge_best(&ScoreRecord {
            play_count: other,
            ..base
        });
        assert_eq!(cur.play_count, expected);
    }
}
