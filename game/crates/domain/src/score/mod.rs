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
    /// クリアした回数（FAILED 以外のランプで終えた回数）
    pub clear_count: u32,
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
        self.clear_count += other.clear_count;
    }

    /// まだ遊んでいない記録（ランプ NO PLAY、EX スコア 0、最小 BP 不明）
    pub fn empty(chart: ChartHash, ruleset: RulesetId) -> Self {
        Self {
            chart,
            ruleset,
            lamp: ClearLamp::NoPlay,
            ex_score: 0,
            pgreat: 0,
            great: 0,
            good: 0,
            bad: 0,
            poor: 0,
            max_combo: 0,
            min_bp: None,
            play_count: 0,
            clear_count: 0,
            origin: ScoreOrigin::Native,
        }
    }

    /// LR2 のプレイ後の更新（DOM-SCR-06, 07）。`play` は 1 プレイ分の記録
    pub fn update_lr2(&mut self, play: &ScoreRecord, mode: SaveMode) {
        debug_assert_eq!(self.chart, play.chart);
        let lamp = match mode {
            SaveMode::Full => play.lamp,
            SaveMode::LampOnly => play.lamp.min(ClearLamp::Easy),
        };
        self.lamp = self.lamp.max(lamp);
        self.play_count += 1;
        if lamp > ClearLamp::Failed {
            self.clear_count += 1;
        }
        if mode == SaveMode::LampOnly {
            return;
        }
        if play.ex_score > self.ex_score {
            self.ex_score = play.ex_score;
            (self.pgreat, self.great, self.good, self.bad, self.poor) =
                (play.pgreat, play.great, play.good, play.bad, play.poor);
        }
        self.max_combo = self.max_combo.max(play.max_combo);
        self.min_bp = match (self.min_bp, play.min_bp) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
}

/// プレイ後に何を保存するか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// すべて
    Full,
    /// ランプ（EASY まで）と回数だけ（LR2 の HS-FIX CONSTANT など）
    LampOnly,
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
            clear_count: 6,
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

    /// DOM-SCR-05: プレイ回数・クリア回数は合算
    #[rstest]
    #[case::sum((10, 6), (5, 2), (15, 8))]
    #[case::zero((10, 6), (0, 0), (10, 6))]
    fn dom_scr_05_counts_summed(
        base: ScoreRecord,
        #[case] current: (u32, u32),
        #[case] other: (u32, u32),
        #[case] expected: (u32, u32),
    ) {
        let mut cur = ScoreRecord {
            play_count: current.0,
            clear_count: current.1,
            ..base.clone()
        };
        cur.merge_best(&ScoreRecord {
            play_count: other.0,
            clear_count: other.1,
            ..base
        });
        assert_eq!((cur.play_count, cur.clear_count), expected);
    }

    /// 1 プレイ分（EX = PG×2 + GR）
    fn play(lamp: ClearLamp, pg: u32, gr: u32, combo: u32, bp: u32) -> ScoreRecord {
        ScoreRecord {
            lamp,
            ex_score: pg * 2 + gr,
            pgreat: pg,
            great: gr,
            good: 0,
            bad: bp,
            poor: 0,
            max_combo: combo,
            min_bp: Some(bp),
            play_count: 1,
            clear_count: 0,
            ..base()
        }
    }

    /// (ランプ, EX, PG, 最大コンボ, 最小 BP, プレイ回数, クリア回数)
    type Summary = (ClearLamp, u32, u32, u32, Option<u32>, u32, u32);
    fn summary(r: &ScoreRecord) -> Summary {
        (
            r.lamp,
            r.ex_score,
            r.pgreat,
            r.max_combo,
            r.min_bp,
            r.play_count,
            r.clear_count,
        )
    }

    /// DOM-SCR-06: LR2 のプレイ後の更新。ランプは良い方、EX スコアが上回ったときだけ判定内訳を替える（同点は残す）、
    /// 最大コンボは大きい方、最小 BP は小さい方、プレイ回数 +1、クリア回数は FAILED 以外で +1
    #[rstest]
    #[case::first_play(None, play(ClearLamp::Normal, 300, 10, 200, 4), (ClearLamp::Normal, 610, 300, 200, Some(4), 1, 1))]
    #[case::first_play_zero_ex(None, play(ClearLamp::Failed, 0, 0, 0, 9), (ClearLamp::Failed, 0, 0, 0, Some(9), 1, 0))]
    #[case::better_ex(Some(base()), play(ClearLamp::Failed, 500, 10, 100, 20), (ClearLamp::Easy, 1010, 500, 300, Some(8), 11, 6))]
    #[case::tie_keeps_judges(Some(base()), play(ClearLamp::Hard, 450, 100, 400, 2), (ClearLamp::Hard, 1000, 400, 400, Some(2), 11, 7))]
    #[case::worse(Some(base()), play(ClearLamp::Easy, 100, 0, 50, 30), (ClearLamp::Easy, 1000, 400, 300, Some(8), 11, 7))]
    fn dom_scr_06_update_lr2(
        #[case] best: Option<ScoreRecord>,
        #[case] p: ScoreRecord,
        #[case] want: Summary,
    ) {
        let mut cur = best.unwrap_or_else(|| ScoreRecord::empty(p.chart, RulesetId::Lr2));
        cur.update_lr2(&p, SaveMode::Full);
        assert_eq!(summary(&cur), want);
    }

    /// DOM-SCR-07: LR2 のランプだけの保存。ランプは EASY を上限に良い方、回数は DOM-SCR-06 と同じ、ほかは変えない
    #[rstest]
    #[case::capped_to_easy(None, play(ClearLamp::FullCombo, 500, 0, 500, 0), (ClearLamp::Easy, 0, 0, 0, None, 1, 1))]
    #[case::failed(None, play(ClearLamp::Failed, 500, 0, 500, 0), (ClearLamp::Failed, 0, 0, 0, None, 1, 0))]
    #[case::keeps_better(Some(ScoreRecord { lamp: ClearLamp::Hard, ..base() }), play(ClearLamp::Normal, 900, 0, 900, 0), (ClearLamp::Hard, 1000, 400, 300, Some(8), 11, 7))]
    fn dom_scr_07_lamp_only(
        #[case] best: Option<ScoreRecord>,
        #[case] p: ScoreRecord,
        #[case] want: Summary,
    ) {
        let mut cur = best.unwrap_or_else(|| ScoreRecord::empty(p.chart, RulesetId::Lr2));
        cur.update_lr2(&p, SaveMode::LampOnly);
        assert_eq!(summary(&cur), want);
    }
}
