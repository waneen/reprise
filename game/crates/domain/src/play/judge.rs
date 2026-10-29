//! 判定幅と判定（DOM-JDG-01..02）。幅の値そのものは各 Ruleset が決める。
use super::Judge;

/// 判定幅を決めるノーツの種類（ロングノーツ終端は未対応）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JudgeLane {
    Key,
    Scratch,
}

/// 1 つの判定の幅（µs、両端を含む）。`early` は早押し側、`late` は遅押し側の大きさ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub early: i64,
    pub late: i64,
}

impl Window {
    pub const fn ms(early: i64, late: i64) -> Self {
        Self {
            early: early * 1000,
            late: late * 1000,
        }
    }
    fn contains(&self, delta_us: i64) -> bool {
        -self.early <= delta_us && delta_us <= self.late
    }
}

/// PG・GR・GD・BD と空 POOR の幅
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeWindows {
    /// PG, GR, GD, BD の順
    pub grades: [Window; 4],
    pub empty_poor: Window,
}

impl JudgeWindows {
    /// 押下の判定。`delta_us` = 押下時刻 − ノーツ時刻（正 = 遅い）。
    /// どの幅にも入らなければ `None`（その押下はこのノーツに作用しない）
    pub fn judge(&self, delta_us: i64) -> Option<Judge> {
        const GRADES: [Judge; 4] = [Judge::PGreat, Judge::Great, Judge::Good, Judge::Bad];
        if let Some(i) = self.grades.iter().position(|w| w.contains(delta_us)) {
            return Some(GRADES[i]);
        }
        self.empty_poor
            .contains(delta_us)
            .then_some(Judge::EmptyPoor)
    }

    /// 見逃し: 現在時刻 − ノーツ時刻が BD の遅い側を超えたら POOR
    pub fn is_missed(&self, elapsed_us: i64) -> bool {
        elapsed_us > self.grades[3].late
    }
}
