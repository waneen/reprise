//! LR2 の判定・ゲージ（DOM-LR2-*）。OpenLR2 の `LR2_bmsload.cpp`（判定幅・TOTAL・増減表・初期値）、
//! `LR2_replay.cpp`（ゲージ更新）、`LR2_statplay.cpp`（`CheckClear`）、`Scene04_Play.cpp`（判定）を
//! 仕様として参照した（コードは転載していない）。段位（コース）用の表と NONSTOP の幅は未対応。
use super::{
    gauge::{index, Inner},
    Gauge, GaugeType, Judge, JudgeLane, JudgeWindows, Ruleset, RulesetId, Window,
};
use crate::chart::ChartMeta;

pub struct Lr2Ruleset;

/// `#RANK` ごとの PG/GR/GD（ms）。0/1/3 以外（既定 2 を含む）は NORMAL
fn grade_ms(rank: Option<i32>) -> [i64; 3] {
    match rank {
        Some(0) => [8, 24, 40],
        Some(1) => [15, 30, 60],
        Some(3) => [21, 60, 120],
        _ => [18, 40, 100],
    }
}

/// LR2 が使う TOTAL（整数）。`#TOTAL` は整数に切り捨て、1 未満・無しなら既定式 × 0.8 を切り捨て
pub fn effective_total(total: Option<f64>, notes: usize) -> i64 {
    let t = total.map_or(0, |t| t.trunc() as i64);
    if t >= 1 {
        return t;
    }
    let n = notes as f64;
    let calc = if notes < 400 {
        n / 5.0 + 200.0
    } else if notes < 600 {
        (n - 400.0) / 2.5 + 280.0
    } else {
        (n - 600.0) / 5.0 + 360.0
    };
    (calc * 0.8) as i64
}

/// HARD 系のダメージ係数（OpenLR2 自身が「要確認」としている式）
pub fn damage_factor(total: i64, notes: usize) -> f64 {
    let n = notes as f64;
    let base = match notes {
        0..=19 => 10.0,
        20..=29 => 10.0 - (n - 20.0) / 10.0 * 2.0,
        30..=44 => 7.0 - (n - 30.0) / 15.0,
        45..=59 => 6.0 - (n - 45.0) / 15.0,
        60..=124 => 5.0 - (n - 60.0) / 65.0,
        125..=249 => 4.0 - (n - 125.0) / 125.0,
        250..=499 => 3.0 - (n - 250.0) / 250.0,
        500..=999 => 2.0 - (n - 500.0) / 500.0,
        _ => 1.0,
    };
    let recover = (((total as f64) - 80.0) * 0.125 / 2.0) as i64;
    let recover = if recover <= 0 { 1 } else { recover };
    f64::max(base * 10.0, 100.0 / recover as f64) / 10.0
}

impl Ruleset for Lr2Ruleset {
    fn id(&self) -> RulesetId {
        RulesetId::Lr2
    }
    /// bmson（百分率）は LR2 が読まないので `#RANK` 既定（2）扱い。皿も鍵盤と同じ
    fn judge_windows(&self, meta: &ChartMeta, _lane: JudgeLane) -> JudgeWindows {
        let rank = if meta.judge_rank_percent.is_some() {
            None
        } else {
            meta.judge_rank
        };
        let [pg, gr, gd] = grade_ms(rank);
        JudgeWindows {
            grades: [
                Window::ms(pg, pg),
                Window::ms(gr, gr),
                Window::ms(gd, gd),
                Window::ms(200, 200),
            ],
            // 1000ms「未満」の早押し
            empty_poor: Window {
                early: 999_999,
                late: 0,
            },
        }
    }
    fn gauge(&self, ty: GaugeType, meta: &ChartMeta, notes: usize) -> Option<Gauge> {
        Lr2Gauge::new(ty, meta.total, notes).map(|g| Gauge(Inner::Lr2(g)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lr2Gauge {
    pub hp: f64,
    ty: GaugeType,
    /// PG, GR, GD, BD, PR, 空POOR
    pub delta: [f64; 6],
}

impl Lr2Gauge {
    pub(crate) fn new(ty: GaugeType, total: Option<f64>, notes: usize) -> Option<Self> {
        let total = effective_total(total, notes);
        let n = notes.max(1);
        let dmg = damage_factor(total, notes);
        // OpenLR2 は int / float で割る
        let per = (total as f32 / n as f32) as f64;
        let half = (total as f32 / (2 * n) as f32) as f64;
        let delta = match ty {
            GaugeType::Groove => [per, per, half, -4.0, -6.0, -2.0],
            GaugeType::Hard => [0.1, 0.1, 0.05, -6.0 * dmg, -10.0 * dmg, -2.0 * dmg],
            GaugeType::Hazard => [0.0, 0.0, 0.0, -100.0, -100.0, 0.0],
            GaugeType::Easy => [
                per * 1.2,
                per * 1.2,
                half * 1.2,
                -3.2,
                -4.800000000000001,
                -1.6,
            ],
            GaugeType::PAttack => [0.1, -1.0, -100.0, -100.0, -100.0, -100.0],
            GaugeType::GAttack => [-10.0 * dmg, -1.0, 0.1, -6.0, -10.0 * dmg, -2.0 * dmg],
            GaugeType::AssistEasy | GaugeType::ExHard => return None,
        };
        let hp = if Self::recovering(ty) { 20.0 } else { 100.0 };
        Some(Self { hp, ty, delta })
    }

    /// GROOVE・EASY（減りにくく 2% で止まる側）
    fn recovering(ty: GaugeType) -> bool {
        matches!(ty, GaugeType::Groove | GaugeType::Easy)
    }

    pub(crate) fn apply(&mut self, j: Judge) {
        let before = (self.hp as i64 / 2) * 2;
        let mut d = self.delta[index(j)];
        let damage_judge = matches!(j, Judge::Bad | Judge::Poor | Judge::EmptyPoor);
        if before <= 30 && !Self::recovering(self.ty) && damage_judge {
            d *= 0.6;
        }
        self.hp += d;
        if Self::recovering(self.ty) && self.hp <= 2.0 {
            self.hp = 2.0;
        }
        self.hp = self.hp.clamp(0.0, 100.0);
        if before <= 0 {
            self.hp = 0.0;
        }
    }

    /// 地雷: HP から生の値を引く（GROOVE/EASY は 2 で止まる）
    pub(crate) fn apply_mine(&mut self, damage: f64) {
        self.hp -= damage;
        if Self::recovering(self.ty) && self.hp <= 2.0 {
            self.hp = 2.0;
        }
        self.hp = self.hp.clamp(0.0, 100.0);
    }

    pub(crate) fn qualified(&self) -> bool {
        match self.ty {
            GaugeType::Hard | GaugeType::GAttack | GaugeType::PAttack => self.hp >= 2.0,
            _ => self.hp >= 80.0,
        }
    }
}
