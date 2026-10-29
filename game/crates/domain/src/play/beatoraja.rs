//! beatoraja の 7 鍵ルール（`BMSPlayerRule.Beatoraja_7`）の判定・ゲージ。
//! beatoraja の `JudgeProperty.SEVENKEYS`・`GaugeProperty.SEVENKEYS`・`GrooveGauge`・`BMSPlayerRule.validate`
//! を仕様として参照した（コードは転載していない）。判定幅の利用者補正（judgeWindowRate）は 100% 固定。
//! 5 鍵・PMS・24 鍵・段位ゲージ・`#DEFEXRANK`・ロングノーツ終端の幅は未対応。
use super::{
    gauge::{index, Inner},
    Gauge, GaugeType, Judge, JudgeLane, JudgeWindows, Ruleset, RulesetId, Window,
};
use crate::chart::ChartMeta;

pub struct BeatorajaRuleset;

/// 基準の幅（µs, (早, 遅)）。PG, GR, GD, BD, 空POOR の順
const KEY: [(i64, i64); 5] = [
    (20_000, 20_000),
    (60_000, 60_000),
    (150_000, 150_000),
    (220_000, 280_000),
    (500_000, 150_000),
];
const SCRATCH: [(i64, i64); 5] = [
    (30_000, 30_000),
    (70_000, 70_000),
    (160_000, 160_000),
    (230_000, 290_000),
    (500_000, 160_000),
];

/// `#RANK` 0..=4 の倍率（%）
const RANK_RATE: [i64; 5] = [25, 50, 75, 100, 125];

/// 判定幅の倍率（%）。`#RANK` は表で引き、範囲外・無しは NORMAL。
/// bmson の `judge_rank` は 0 未満なら NORMAL、5 未満なら `#RANK` とみなし、それ以外はそのまま %
pub fn judge_rate(meta: &ChartMeta) -> i64 {
    let step = |r: i64| RANK_RATE.get(r as usize).copied().unwrap_or(RANK_RATE[2]);
    match meta.judge_rank_percent {
        Some(p) => {
            let p = p.trunc() as i64;
            if p < 0 {
                RANK_RATE[2]
            } else if p < 5 {
                step(p)
            } else {
                p
            }
        }
        None => match meta.judge_rank {
            Some(r) if (0..5).contains(&r) => step(r as i64),
            _ => RANK_RATE[2],
        },
    }
}

/// TOTAL の既定値（7 鍵・5 鍵・PMS 系）
pub fn default_total(notes: usize) -> f64 {
    let n = notes as f64;
    f64::max(260.0, 7.605 * n / (0.01 * n + 6.5))
}

/// ゲージに使う TOTAL。`#TOTAL` が正ならそのまま、bmson の `total` が正なら % として既定値に掛け、
/// それ以外は既定値
pub fn effective_total(meta: &ChartMeta, notes: usize) -> f64 {
    let def = default_total(notes);
    match (meta.total_percent, meta.total) {
        (Some(p), _) if p > 0.0 => p / 100.0 * def,
        (None, Some(t)) if t > 0.0 => t,
        _ => def,
    }
}

impl Ruleset for BeatorajaRuleset {
    fn id(&self) -> RulesetId {
        RulesetId::Beatoraja
    }
    fn judge_windows(&self, meta: &ChartMeta, lane: JudgeLane) -> JudgeWindows {
        let base = match lane {
            JudgeLane::Key => KEY,
            JudgeLane::Scratch => SCRATCH,
        };
        let rate = judge_rate(meta);
        // PG..BD は倍率を掛け、空POOR は固定。同じ倍率なので幅の大小関係は崩れない
        // （beatoraja の幅の詰め直しは利用者補正が 100% なら何もしない）
        let win = |(e, l): (i64, i64), r: i64| Window {
            early: e * r / 100,
            late: l * r / 100,
        };
        JudgeWindows {
            grades: [0, 1, 2, 3].map(|i| win(base[i], rate)),
            empty_poor: win(base[4], 100),
        }
    }
    fn gauge(&self, ty: GaugeType, meta: &ChartMeta, notes: usize) -> Option<Gauge> {
        BrGauge::new(ty, effective_total(meta, notes), notes).map(|g| Gauge(Inner::Beatoraja(g)))
    }
}

#[derive(Debug, Clone, Copy)]
enum Modifier {
    /// 増加量に TOTAL / ノーツ数を掛ける
    Total,
    /// TOTAL が小さいほど増加量を減らす
    LimitIncrement,
}

const HARD_GUTS: &[(f32, f32)] = &[
    (10.0, 0.4),
    (20.0, 0.5),
    (30.0, 0.6),
    (40.0, 0.7),
    (50.0, 0.8),
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrGauge {
    pub value: f32,
    min: f32,
    max: f32,
    pub border: f32,
    /// PG, GR, GD, BD, PR, 空POOR（補正後）
    pub delta: [f32; 6],
    /// 減少時: 値 < 閾値 の最初の行の倍率を掛ける
    guts: &'static [(f32, f32)],
}

impl BrGauge {
    fn row(min: f32, init: f32, border: f32, delta: [f32; 6], guts: &'static [(f32, f32)]) -> Self {
        Self {
            value: init,
            min,
            max: 100.0,
            border,
            delta,
            guts,
        }
    }

    fn new(ty: GaugeType, total: f64, notes: usize) -> Option<Self> {
        use Modifier::*;
        // 行は (最小, 初期, ボーダー, 増減, guts)。最大はすべて 100
        let (m, mut g) = match ty {
            GaugeType::AssistEasy => (
                Some(Total),
                Self::row(2.0, 20.0, 60.0, [1.0, 1.0, 0.5, -1.5, -3.0, -0.5], &[]),
            ),
            GaugeType::Easy => (
                Some(Total),
                Self::row(2.0, 20.0, 80.0, [1.0, 1.0, 0.5, -1.5, -4.5, -1.0], &[]),
            ),
            GaugeType::Groove => (
                Some(Total),
                Self::row(2.0, 20.0, 80.0, [1.0, 1.0, 0.5, -3.0, -6.0, -2.0], &[]),
            ),
            GaugeType::Hard => (
                Some(LimitIncrement),
                Self::row(
                    0.0,
                    100.0,
                    0.0,
                    [0.15, 0.12, 0.03, -5.0, -10.0, -5.0],
                    HARD_GUTS,
                ),
            ),
            GaugeType::ExHard => (
                Some(LimitIncrement),
                Self::row(0.0, 100.0, 0.0, [0.15, 0.06, 0.0, -8.0, -16.0, -8.0], &[]),
            ),
            GaugeType::Hazard => (
                None,
                Self::row(
                    0.0,
                    100.0,
                    0.0,
                    [0.15, 0.06, 0.0, -100.0, -100.0, -10.0],
                    &[],
                ),
            ),
            GaugeType::PAttack | GaugeType::GAttack => return None,
        };
        let n = notes.max(1) as f64;
        g.delta = g.delta.map(|f| match m {
            Some(Total) if f > 0.0 => (f as f64 * total / n) as f32,
            Some(LimitIncrement) if f > 0.0 => {
                let pg = f64::max(f64::min(0.15f32 as f64, (2.0 * total - 320.0) / n), 0.0) as f32;
                f * (pg / 0.15)
            }
            _ => f,
        });
        Some(g)
    }

    pub(crate) fn apply(&mut self, j: Judge) {
        let mut inc = self.delta[index(j)];
        if inc < 0.0 {
            if let Some(&(_, rate)) = self.guts.iter().find(|(th, _)| self.value < *th) {
                inc *= rate;
            }
        }
        // 0 になったゲージは以後動かない
        if self.value > 0.0 {
            self.value = (self.value + inc).clamp(self.min, self.max);
        }
    }

    pub(crate) fn qualified(&self) -> bool {
        self.value > 0.0 && self.value >= self.border
    }
}
