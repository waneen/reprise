//! ゲージ。増減の規則は Ruleset ごとに違うので中身は各実装が持つ。
use super::{beatoraja::BrGauge, lr2::Lr2Gauge, Judge};

/// ゲージの種類。LR2 の DEATH は `Hazard`。Ruleset に無い種類は `Ruleset::gauge` が `None` を返す
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaugeType {
    AssistEasy,
    Easy,
    Groove,
    Hard,
    ExHard,
    Hazard,
    /// LR2 の P-ATTACK
    PAttack,
    /// LR2 の G-ATTACK
    GAttack,
}

/// 判定ごとの増減表の添字（PG, GR, GD, BD, PR, 空POOR）
pub(crate) fn index(j: Judge) -> usize {
    match j {
        Judge::PGreat => 0,
        Judge::Great => 1,
        Judge::Good => 2,
        Judge::Bad => 3,
        Judge::Poor => 4,
        Judge::EmptyPoor => 5,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inner {
    Lr2(Lr2Gauge),
    Beatoraja(BrGauge),
}

/// プレイ中のゲージ（値は 0..=100 の %）
#[derive(Debug, Clone, PartialEq)]
pub struct Gauge(pub(crate) Inner);

impl Gauge {
    pub fn value(&self) -> f64 {
        match &self.0 {
            Inner::Lr2(g) => g.hp,
            Inner::Beatoraja(g) => g.value as f64,
        }
    }
    /// 1 回の判定を反映する
    pub fn apply(&mut self, j: Judge) {
        match &mut self.0 {
            Inner::Lr2(g) => g.apply(j),
            Inner::Beatoraja(g) => g.apply(j),
        }
    }
    /// 全ノーツを処理し終えた時点でこの値ならクリア
    pub fn qualified(&self) -> bool {
        match &self.0 {
            Inner::Lr2(g) => g.qualified(),
            Inner::Beatoraja(g) => g.qualified(),
        }
    }
}
