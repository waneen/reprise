//! リザルトのゲージグラフ用の HP の記録（LR2 `LogGraphPlayData`・`LogGraphPlayerDataToEnd`。OpenLR2 `LR2_statplay.cpp`）

/// 記録する区間の数（曲の長さを 1000 に分ける）
pub const GRAPH_POINTS: usize = 1000;

/// 区間ごとの HP（整数）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GaugeGraph {
    hp: Vec<i32>,
    cursor: usize,
}

impl GaugeGraph {
    /// DOM-GRF-01: 区間 0 は始めの HP（ほかは 0）で、まだ何も記録していない
    pub fn new(initial_hp: f64) -> Self {
        let mut hp = vec![0; GRAPH_POINTS];
        hp[0] = initial_hp as i32;
        Self { hp, cursor: 0 }
    }

    /// DOM-GRF-01: 譜面の時刻 `t_ms` の HP を記録する。曲の長さ `end_ms` が正で `t_ms` ≤ `end_ms` のときだけ、
    /// 区間 ⌊t × 1000 / 長さ⌋（0 の方へ切り捨て）まで、まだ記録していない区間に HP（切り捨て）を書く
    pub fn log(&mut self, t_ms: i64, end_ms: i64, hp: f64) {
        if self.cursor >= GRAPH_POINTS || t_ms > end_ms || end_ms <= 0 {
            return;
        }
        while self.cursor < GRAPH_POINTS {
            if t_ms * 1000 / end_ms < self.cursor as i64 {
                return;
            }
            self.hp[self.cursor] = hp as i32;
            self.cursor += 1;
        }
    }

    /// DOM-GRF-02: 終わり。まだ記録していない区間を、1 つでも記録していれば `hp`（切り捨て）で、
    /// 何も記録していなければ区間 0 の値で埋める
    pub fn finish(&mut self, hp: f64) {
        let fill = if self.cursor >= 1 {
            hp as i32
        } else {
            self.hp[0]
        };
        for v in &mut self.hp[self.cursor.min(GRAPH_POINTS)..] {
            *v = fill;
        }
        self.cursor = GRAPH_POINTS;
    }

    /// 区間ごとの HP
    pub fn points(&self) -> &[i32] {
        &self.hp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// DOM-GRF-01: 記録（(時刻, HP) を順に、曲の長さ 1000ms）→ 区間 0..5 の値と記録した区間の数
    #[rstest]
    #[case::initial(vec![], [20, 0, 0, 0, 0], 0)]
    #[case::hp_truncated(vec![(0, 22.9)], [22, 0, 0, 0, 0], 1)]
    #[case::before_start(vec![(-1, 30.0)], [20, 0, 0, 0, 0], 0)]
    #[case::fills_up_to_slot(vec![(0, 20.0), (3, 40.5)], [20, 40, 40, 40, 0], 4)]
    #[case::same_slot_keeps_first(vec![(2, 50.0), (2, 60.0)], [50, 50, 50, 0, 0], 3)]
    #[case::after_end_ignored(vec![(1001, 90.0)], [20, 0, 0, 0, 0], 0)]
    #[case::end_fills_all(vec![(1000, 90.0)], [90, 90, 90, 90, 90], 1000)]
    fn dom_grf_01_log(
        #[case] frames: Vec<(i64, f64)>,
        #[case] want: [i32; 5],
        #[case] cursor: usize,
    ) {
        let mut g = GaugeGraph::new(20.0);
        for (t, hp) in frames {
            g.log(t, 1000, hp);
        }
        assert_eq!(&g.points()[..5], &want);
        assert_eq!(g.cursor, cursor);
        // 長さ 0 は記録しない
        let mut z = GaugeGraph::new(20.0);
        z.log(0, 0, 50.0);
        assert_eq!(z.cursor, 0);
    }

    /// DOM-GRF-02: 終わりの埋め方（記録した区間の後ろ / 何も記録していなければ区間 0 の値）
    #[rstest]
    #[case::after_logged(Some(2), 0.0, [20, 20, 20, 0, 0], 0)]
    #[case::hp_truncated(Some(0), 77.7, [20, 77, 77, 77, 77], 77)]
    #[case::nothing_logged(None, 50.0, [20, 20, 20, 20, 20], 20)]
    fn dom_grf_02_finish(
        #[case] logged_until: Option<i64>,
        #[case] hp: f64,
        #[case] head: [i32; 5],
        #[case] last: i32,
    ) {
        let mut g = GaugeGraph::new(20.0);
        if let Some(t) = logged_until {
            g.log(t, 1000, 20.0);
        }
        g.finish(hp);
        assert_eq!(&g.points()[..5], &head);
        assert_eq!(g.points()[GRAPH_POINTS - 1], last);
    }
}
