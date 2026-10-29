//! LR2 のプレイ進行（DOM-LRP-*）。押下・離し・時間経過から判定・コンボ・ゲージ・ランプを進める。
//! OpenLR2 の `Scene04_Play.cpp`（`ProcSinglenote`・`ProcLongnote`・`ApplyJudgeNote`・`ApplyJudgeMine`・
//! 生存判定）、`LR2_bmsload.cpp`（レーンへの配置）、`LR2_statplay.cpp`（`CheckClear`）を仕様として参照した
//! （コードは転載していない）。LR2 はフレームごとに処理するが、ここでは期限の早い順に処理する。
use super::{
    lr2::{Lr2Gauge, Lr2Ruleset},
    GaugeType, Judge, JudgeLane, JudgeWindows, Ruleset,
};
use super::{RulesetId, TimingMap};
use crate::chart::{Chart, ChartHash, Lane, NoteKind};
use crate::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Normal,
    /// 終端の時刻（µs）
    Long(i64),
    /// 地雷の生の値
    Mine(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaneNote {
    time: i64,
    kind: Kind,
}

#[derive(Debug, Default)]
struct LaneState {
    notes: VecDeque<LaneNote>,
    held: bool,
    /// 押下中の LN の始点判定と差（µs）
    active: Option<(Judge, i64)>,
}

/// プレイ中に起きたこと（表示・音のため）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayEvent {
    /// 判定。`delta_us` は押下 − ノーツ（見逃しは `None`。LN は始点の差）
    Judge {
        lane: Lane,
        judge: Judge,
        delta_us: Option<i64>,
    },
    /// 地雷の発動
    Mine { lane: Lane, damage: u32 },
}

/// 判定の数。空POOR と地雷も POOR に数える
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JudgeCounts {
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
}

impl JudgeCounts {
    pub fn ex_score(&self) -> u32 {
        self.pgreat * 2 + self.great
    }
}

pub struct Lr2Play {
    lanes: BTreeMap<Lane, LaneState>,
    windows: JudgeWindows,
    gauge_type: GaugeType,
    gauge: Lr2Gauge,
    total: u32,
    processed: u32,
    combo: u32,
    max_combo: u32,
    counts: JudgeCounts,
    dead: bool,
}

/// 譜面の音符を LR2 の規則でレーンに並べる（DOM-LRP-01）
fn build_lanes(chart: &Chart, timing: &TimingMap) -> BTreeMap<Lane, LaneState> {
    let mut sorted: Vec<(i64, usize)> = chart
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| !matches!(n.kind, NoteKind::Invisible))
        .map(|(i, n)| (timing.time_of(n.beat), i))
        .collect();
    sorted.sort_by_key(|&(t, i)| (t, i));
    let mut lanes: BTreeMap<Lane, LaneState> = BTreeMap::new();
    for (time, i) in sorted {
        let n = &chart.notes[i];
        let kind = match &n.kind {
            NoteKind::Normal => Kind::Normal,
            NoteKind::Long { end, .. } => Kind::Long(timing.time_of(*end)),
            NoteKind::Mine { damage_raw } => Kind::Mine(*damage_raw),
            NoteKind::Invisible => unreachable!(),
        };
        let q = &mut lanes.entry(n.lane).or_default().notes;
        let note = LaneNote { time, kind };
        match q.back() {
            None => q.push_back(note),
            Some(prev) => {
                let prev_end = match prev.kind {
                    Kind::Long(end) => end,
                    _ => -1,
                };
                if time > prev_end {
                    if time != prev.time {
                        q.push_back(note);
                    } else if matches!(kind, Kind::Long(_)) {
                        // 同時刻の通常ノーツは LN が置き換える
                        q.pop_back();
                        q.push_back(note);
                    }
                }
            }
        }
    }
    lanes
}

impl Lr2Play {
    /// 譜面と時刻表からプレイを始める。LR2 に無いゲージ種類なら `None`
    pub fn new(chart: &Chart, timing: &TimingMap, gauge_type: GaugeType) -> Option<Self> {
        let lanes = build_lanes(chart, timing);
        let total = lanes
            .values()
            .flat_map(|l| &l.notes)
            .filter(|n| !matches!(n.kind, Kind::Mine(_)))
            .count();
        let gauge = Lr2Gauge::new(gauge_type, chart.meta.total, total)?;
        Some(Self {
            lanes,
            windows: Lr2Ruleset.judge_windows(&chart.meta, JudgeLane::Key),
            gauge_type,
            gauge,
            total: total as u32,
            processed: 0,
            combo: 0,
            max_combo: 0,
            counts: JudgeCounts::default(),
            dead: false,
        })
    }

    pub fn total_notes(&self) -> u32 {
        self.total
    }
    pub fn counts(&self) -> JudgeCounts {
        self.counts
    }
    pub fn combo(&self) -> u32 {
        self.combo
    }
    pub fn max_combo(&self) -> u32 {
        self.max_combo
    }
    pub fn gauge(&self) -> f64 {
        self.gauge.hp
    }
    /// ゲージが 2 未満になって途中で終わったか
    pub fn is_dead(&self) -> bool {
        self.dead
    }
    /// 全ノーツを処理したか、途中で終わったか
    pub fn is_finished(&self) -> bool {
        self.dead || self.processed == self.total
    }

    /// クリアランプ（DOM-LRP-09）
    pub fn clear_lamp(&self) -> ClearLamp {
        if self.total > 0 && self.max_combo == self.total {
            return ClearLamp::FullCombo;
        }
        if self.processed != self.total || !self.gauge.qualified() {
            return ClearLamp::Failed;
        }
        match self.gauge_type {
            GaugeType::Hard | GaugeType::GAttack | GaugeType::PAttack => ClearLamp::Hard,
            GaugeType::Easy => ClearLamp::Easy,
            _ => ClearLamp::Normal,
        }
    }

    /// 1 プレイ分の記録
    pub fn record(&self, chart: ChartHash) -> ScoreRecord {
        let c = self.counts;
        ScoreRecord {
            chart,
            ruleset: RulesetId::Lr2,
            lamp: self.clear_lamp(),
            ex_score: c.ex_score(),
            pgreat: c.pgreat,
            great: c.great,
            good: c.good,
            bad: c.bad,
            poor: c.poor,
            max_combo: self.max_combo,
            min_bp: Some(c.bad + c.poor),
            play_count: 1,
            origin: ScoreOrigin::Native,
        }
    }

    /// 判定を記録に反映する（DOM-LRP-07）
    fn score(&mut self, lane: Lane, judge: Judge, delta_us: Option<i64>, out: &mut Vec<PlayEvent>) {
        let c = &mut self.counts;
        match judge {
            Judge::PGreat => c.pgreat += 1,
            Judge::Great => c.great += 1,
            Judge::Good => c.good += 1,
            Judge::Bad => c.bad += 1,
            Judge::Poor | Judge::EmptyPoor => c.poor += 1,
        }
        match judge {
            Judge::PGreat | Judge::Great | Judge::Good => {
                self.combo += 1;
                self.max_combo = self.max_combo.max(self.combo);
            }
            Judge::Bad | Judge::Poor => self.combo = 0,
            Judge::EmptyPoor => {}
        }
        if judge != Judge::EmptyPoor {
            self.processed += 1;
        }
        self.gauge.apply(judge);
        out.push(PlayEvent::Judge {
            lane,
            judge,
            delta_us,
        });
        self.check_alive();
    }

    fn mine(&mut self, lane: Lane, damage: u32, out: &mut Vec<PlayEvent>) {
        self.counts.poor += 1;
        self.gauge.apply_mine(damage as f64);
        out.push(PlayEvent::Mine { lane, damage });
        self.check_alive();
    }

    /// ゲージが 2 未満なら途中終了（DOM-LRP-08）
    fn check_alive(&mut self) {
        if self.gauge.hp < 2.0 {
            self.dead = true;
        }
    }

    /// 先頭ノーツが自動で片付く期限（この時刻を過ぎたら処理）
    fn due(&self, st: &LaneState) -> Option<i64> {
        let n = st.notes.front()?;
        Some(match n.kind {
            Kind::Normal => n.time + self.windows.grades[3].late,
            Kind::Long(end) if st.active.is_some() => end,
            Kind::Long(_) => n.time + self.windows.grades[2].late,
            Kind::Mine(_) => n.time,
        })
    }

    /// 時刻 `t` までに期限の来たノーツを、期限の早い順に処理する（DOM-LRP-04..06）
    pub fn advance(&mut self, t: i64) -> Vec<PlayEvent> {
        let mut out = vec![];
        while !self.dead {
            let next = self
                .lanes
                .iter()
                .filter_map(|(l, st)| self.due(st).map(|d| (d, *l)))
                .filter(|(d, _)| t > *d)
                .min();
            let Some((_, lane)) = next else { break };
            let st = self.lanes.get_mut(&lane).unwrap();
            let n = st.notes.pop_front().unwrap();
            match n.kind {
                Kind::Normal => self.score(lane, Judge::Poor, None, &mut out),
                Kind::Long(_) => match st.active.take() {
                    Some((j, d)) => self.score(lane, j, Some(d), &mut out),
                    None => self.score(lane, Judge::Poor, None, &mut out),
                },
                Kind::Mine(dmg) => {
                    if st.held {
                        self.mine(lane, dmg, &mut out);
                    }
                }
            }
        }
        out
    }

    /// 鍵盤を押した（DOM-LRP-02, 03, 05, 06）
    pub fn press(&mut self, lane: Lane, t: i64) -> Vec<PlayEvent> {
        let mut out = self.advance(t);
        if self.dead {
            return out;
        }
        let Some(st) = self.lanes.get_mut(&lane) else {
            return out;
        };
        st.held = true;
        let Some(n) = st.notes.front().copied() else {
            return out;
        };
        if let Kind::Long(_) = n.kind {
            let d = t - n.time;
            match self.windows.judge(d) {
                Some(Judge::EmptyPoor) => self.score(lane, Judge::EmptyPoor, Some(d), &mut out),
                Some(j) => self.lanes.get_mut(&lane).unwrap().active = Some((j, d)),
                None => {}
            }
        } else {
            self.press_single(lane, t, &mut out);
        }
        out
    }

    /// 先頭ノーツを単ノーツとして押下判定する。BD なら次のノーツも続けて見る
    fn press_single(&mut self, lane: Lane, t: i64, out: &mut Vec<PlayEvent>) {
        let st = self.lanes.get_mut(&lane).unwrap();
        let Some(n) = st.notes.front().copied() else {
            return;
        };
        if let Kind::Mine(dmg) = n.kind {
            if n.time - t < self.windows.grades[1].early {
                st.notes.pop_front();
                self.mine(lane, dmg, out);
            }
            return;
        }
        let d = t - n.time;
        match self.windows.judge(d) {
            Some(Judge::EmptyPoor) => self.score(lane, Judge::EmptyPoor, Some(d), out),
            Some(j) => {
                st.notes.pop_front();
                self.score(lane, j, Some(d), out);
                let bd = self.windows.grades[3];
                let chain = self.lanes[&lane]
                    .notes
                    .front()
                    .is_some_and(|m| (t - m.time).abs() <= bd.late);
                if j == Judge::Bad && chain && !self.dead {
                    self.press_single(lane, t, out);
                }
            }
            None => {}
        }
    }

    /// 鍵盤を離した（DOM-LRP-05）
    pub fn release(&mut self, lane: Lane, t: i64) -> Vec<PlayEvent> {
        let mut out = self.advance(t);
        if self.dead {
            return out;
        }
        let gd = self.windows.grades[2].late;
        let Some(st) = self.lanes.get_mut(&lane) else {
            return out;
        };
        st.held = false;
        if let (
            Some((j, d)),
            Some(LaneNote {
                kind: Kind::Long(end),
                ..
            }),
        ) = (st.active.take(), st.notes.front().copied())
        {
            st.notes.pop_front();
            if t + gd < end {
                self.score(lane, Judge::Bad, Some(d), &mut out);
            } else {
                self.score(lane, j, Some(d), &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{Beat, ChartMeta, KeyMode, Note};
    use rstest::rstest;

    // BPM 60: 1 拍 = 1 秒。ノーツの位置は ms で書く。判定幅は #RANK 2（18/40/100/200ms）
    fn note(lane: u8, ms: i64, kind: NoteKind) -> Note {
        Note {
            beat: Beat(ms as f64 / 1000.0),
            lane: Lane(lane),
            kind,
            sound: None,
        }
    }
    fn n(ms: i64) -> Note {
        note(1, ms, NoteKind::Normal)
    }
    fn ln(s: i64, e: i64) -> Note {
        note(
            1,
            s,
            NoteKind::Long {
                end: Beat(e as f64 / 1000.0),
                end_sound: None,
            },
        )
    }
    fn mine(ms: i64, damage_raw: u32) -> Note {
        note(1, ms, NoteKind::Mine { damage_raw })
    }

    fn chart(notes: Vec<Note>, total: Option<f64>) -> Chart {
        Chart {
            hash: ChartHash {
                md5: [7; 16],
                sha256: None,
            },
            meta: ChartMeta {
                initial_bpm: Some(60.0),
                total,
                ..Default::default()
            },
            key_mode: KeyMode::K7,
            notes,
            bgm: vec![],
            bga: vec![],
            timing: vec![],
            sounds: vec![],
            images: vec![],
        }
    }

    #[derive(Clone, Copy)]
    enum Act {
        /// レーン 1 を押す（ms）
        P(i64),
        /// レーン 1 を離す（ms）
        R(i64),
        /// 時間を進める（ms）
        A(i64),
    }
    use Act::*;

    fn run_with(
        notes: Vec<Note>,
        total: Option<f64>,
        g: GaugeType,
        acts: &[Act],
    ) -> (Lr2Play, Vec<PlayEvent>) {
        let c = chart(notes, total);
        let mut p = Lr2Play::new(&c, &TimingMap::from_chart(&c), g).unwrap();
        let mut ev = vec![];
        for a in acts {
            ev.extend(match *a {
                P(t) => p.press(Lane(1), t * 1000),
                R(t) => p.release(Lane(1), t * 1000),
                A(t) => p.advance(t * 1000),
            });
        }
        (p, ev)
    }
    fn run(notes: Vec<Note>, acts: &[Act]) -> (Lr2Play, Vec<PlayEvent>) {
        run_with(notes, None, GaugeType::Groove, acts)
    }
    fn judges(ev: &[PlayEvent]) -> Vec<Judge> {
        ev.iter()
            .filter_map(|e| match e {
                PlayEvent::Judge { judge, .. } => Some(*judge),
                _ => None,
            })
            .collect()
    }

    /// DOM-LRP-01: レーンへの配置。同時刻の 2 つ目は捨て（LN は通常ノーツを置き換える）、
    /// LN の終端以前に始まるものは捨てる。地雷は並ぶがノーツ数に入らない。不可視は入れない
    #[rstest]
    #[case::distinct(vec![n(1000), n(2000)], 2, vec![1000, 2000])]
    #[case::same_time_first_wins(vec![n(1000), n(1000)], 1, vec![1000])]
    #[case::ln_replaces_normal(vec![n(1000), ln(1000, 2000)], 1, vec![1000])]
    #[case::normal_after_ln_same_time(vec![ln(1000, 2000), n(1000)], 1, vec![1000])]
    #[case::inside_ln_dropped(vec![ln(1000, 3000), n(2000), n(3000), n(4000)], 2, vec![1000, 4000])]
    #[case::mine_not_counted(vec![mine(1000, 5), n(2000)], 1, vec![1000, 2000])]
    #[case::invisible_ignored(vec![note(1, 1000, NoteKind::Invisible)], 0, vec![])]
    fn dom_lrp_01_lanes(#[case] notes: Vec<Note>, #[case] total: u32, #[case] times_ms: Vec<i64>) {
        let (p, _) = run(notes, &[]);
        assert_eq!(p.total_notes(), total);
        let got: Vec<i64> = p
            .lanes
            .get(&Lane(1))
            .map(|l| l.notes.iter().map(|x| x.time / 1000).collect())
            .unwrap_or_default();
        assert_eq!(got, times_ms);
    }

    /// DOM-LRP-02: 押下は先頭ノーツを判定して片付ける。BD のときは次のノーツが ±BD 内なら同じ押下で続けて判定する
    #[rstest]
    #[case::pg(vec![n(1000)], 1000, vec![Judge::PGreat])]
    #[case::pg_edge(vec![n(1000)], 1018, vec![Judge::PGreat])]
    #[case::gr_early(vec![n(1000)], 960, vec![Judge::Great])]
    #[case::gd(vec![n(1000)], 1100, vec![Judge::Good])]
    #[case::bd_early(vec![n(1000)], 800, vec![Judge::Bad])]
    #[case::bd_chains_next(vec![n(1000), n(1150)], 1200, vec![Judge::Bad, Judge::Good])]
    #[case::bd_next_out_of_range(vec![n(1000), n(1450)], 1200, vec![Judge::Bad])]
    #[case::gd_does_not_chain(vec![n(1000), n(1050)], 1090, vec![Judge::Good])]
    fn dom_lrp_02_press(#[case] notes: Vec<Note>, #[case] at: i64, #[case] want: Vec<Judge>) {
        let (_, ev) = run(notes, &[P(at)]);
        assert_eq!(judges(&ev), want);
    }

    /// DOM-LRP-03: 先頭ノーツに当たらず早 1000ms 未満なら空POOR（ノーツは残る）。レーンが空なら何もしない
    #[rstest]
    #[case::just_outside_bd(vec![n(1000)], &[P(799)], vec![Judge::EmptyPoor], 1)]
    #[case::edge_999(vec![n(1000)], &[P(1)], vec![Judge::EmptyPoor], 1)]
    #[case::too_early(vec![n(1000)], &[P(0)], vec![], 1)]
    #[case::note_remains(vec![n(1000)], &[P(500), R(600), P(1000)], vec![Judge::EmptyPoor, Judge::PGreat], 0)]
    #[case::ln_head(vec![ln(1000, 2000)], &[P(500)], vec![Judge::EmptyPoor], 1)]
    #[case::empty_lane(vec![n(1000)], &[P(1000), R(1100), P(1500)], vec![Judge::PGreat], 0)]
    fn dom_lrp_03_empty_poor(
        #[case] notes: Vec<Note>,
        #[case] acts: &[Act],
        #[case] want: Vec<Judge>,
        #[case] remaining: usize,
    ) {
        let (p, ev) = run(notes, acts);
        assert_eq!(judges(&ev), want);
        assert_eq!(p.lanes[&Lane(1)].notes.len(), remaining);
    }

    /// DOM-LRP-04: 見逃し。通常ノーツは BD の遅い側、押していない LN は GD の遅い側を超えたら POOR
    #[rstest]
    #[case::normal_at_edge(vec![n(1000)], 1200, vec![])]
    #[case::normal_past(vec![n(1000)], 1201, vec![Judge::Poor])]
    #[case::ln_at_edge(vec![ln(1000, 2000)], 1100, vec![])]
    #[case::ln_past(vec![ln(1000, 2000)], 1101, vec![Judge::Poor])]
    #[case::in_time_order(vec![n(1000), n(1100)], 5000, vec![Judge::Poor, Judge::Poor])]
    fn dom_lrp_04_miss(#[case] notes: Vec<Note>, #[case] at: i64, #[case] want: Vec<Judge>) {
        let (_, ev) = run(notes, &[A(at)]);
        assert_eq!(judges(&ev), want);
    }

    /// DOM-LRP-05: LN は始点の判定を保留し、押し続けて終端を過ぎたらその判定。終端の GD 幅より前に離したら BAD。
    /// 始点を GD の遅い側より後に押しても見逃し（DOM-LRP-04）が先
    #[rstest]
    #[case::held_to_end(&[P(1000), A(2000)], vec![])]
    #[case::held_past_end(&[P(1000), A(2001)], vec![Judge::PGreat])]
    #[case::head_kept(&[P(1050), R(1950)], vec![Judge::Good])]
    #[case::released_early(&[P(1000), R(1899)], vec![Judge::Bad])]
    #[case::release_at_gd_edge(&[P(1000), R(1900)], vec![Judge::PGreat])]
    #[case::head_bad_early(&[P(850), A(2001)], vec![Judge::Bad])]
    #[case::late_head_past_gd_is_poor(&[P(1150)], vec![Judge::Poor])]
    #[case::released_after_end(&[P(1000), R(2500)], vec![Judge::PGreat])]
    fn dom_lrp_05_long_note(#[case] acts: &[Act], #[case] want: Vec<Judge>) {
        let (_, ev) = run(vec![ln(1000, 2000)], acts);
        assert_eq!(judges(&ev), want);
    }

    /// DOM-LRP-06: 地雷。押下時に地雷まで GR 幅未満（過ぎていても）なら発動、押しながら通過しても発動、
    /// 押していなければ消える。発動で HP − 生の値（GROOVE/EASY は 2 で止まる）、POOR +1
    #[rstest]
    #[case::press_near(GaugeType::Hard, &[P(970)], 90.0, 1)]
    #[case::press_too_early(GaugeType::Hard, &[P(959), R(980), A(1001)], 100.0, 0)]
    #[case::held_through(GaugeType::Hard, &[P(500), A(1001)], 90.0, 1)]
    #[case::passed_unheld(GaugeType::Hard, &[P(1100)], 100.0, 0)]
    #[case::groove_floor(GaugeType::Groove, &[P(1000)], 20.0 - 10.0, 1)]
    fn dom_lrp_06_mine(
        #[case] g: GaugeType,
        #[case] acts: &[Act],
        #[case] hp: f64,
        #[case] hits: u32,
    ) {
        let (p, ev) = run_with(vec![mine(1000, 10)], None, g, acts);
        assert!((p.gauge() - hp).abs() < 1e-9, "{}", p.gauge());
        assert_eq!(p.counts().poor, hits);
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, PlayEvent::Mine { .. }))
                .count() as u32,
            hits
        );
    }

    /// DOM-LRP-07: コンボは PG/GR/GD で +1、BD/POOR で 0、空POOR・地雷では変わらない。最大コンボを記録
    #[rstest]
    #[case::good_continues(vec![n(1000)], &[P(1100)], 1, 1)]
    #[case::bad_resets(vec![n(1000), n(2000)], &[P(1000), R(1001), P(2200)], 0, 1)]
    #[case::poor_resets(vec![n(1000), n(2000)], &[P(1000), A(2201)], 0, 1)]
    #[case::empty_poor_keeps(vec![n(1000), n(3000)], &[P(1000), R(1001), P(2500)], 1, 1)]
    #[case::mine_keeps(vec![n(1000), mine(2000, 1)], &[P(1000), R(1001), P(2000)], 1, 1)]
    #[case::max_kept(vec![n(1000), n(2000), n(3000)], &[P(1000), R(1001), P(2000), R(2001), A(3201)], 0, 2)]
    fn dom_lrp_07_combo(
        #[case] notes: Vec<Note>,
        #[case] acts: &[Act],
        #[case] combo: u32,
        #[case] max: u32,
    ) {
        let (p, _) = run(notes, acts);
        assert_eq!((p.combo(), p.max_combo()), (combo, max));
    }

    /// DOM-LRP-08: 全ノーツを処理したら終了。ゲージが 2 未満になったら途中終了し、以後の入力は無視
    #[rstest]
    #[case::all_done(GaugeType::Groove, vec![n(1000)], &[P(1000)], true, false, 1)]
    #[case::not_yet(GaugeType::Groove, vec![n(1000), n(2000)], &[P(1000)], false, false, 1)]
    #[case::dead_ignores_input(GaugeType::Hard, vec![mine(1000, 99), n(2000)], &[P(1000), R(1001), P(2000)], true, true, 0)]
    #[case::groove_never_dies(GaugeType::Groove, vec![mine(1000, 99), n(2000)], &[P(1000), R(1001), P(2000)], true, false, 1)]
    fn dom_lrp_08_finish(
        #[case] g: GaugeType,
        #[case] notes: Vec<Note>,
        #[case] acts: &[Act],
        #[case] finished: bool,
        #[case] dead: bool,
        #[case] judged: usize,
    ) {
        let (p, ev) = run_with(notes, None, g, acts);
        assert_eq!(
            (p.is_finished(), p.is_dead(), judges(&ev).len()),
            (finished, dead, judged)
        );
    }

    /// 10 ノーツ（1..=10 秒）。`bad` 番目だけ +150ms で押す（0 なら全 PG）
    fn ten(g: GaugeType, bad: usize, miss_all: bool) -> Lr2Play {
        let notes = (1..=10).map(|i| n(i * 1000)).collect();
        let mut acts = vec![];
        if miss_all {
            acts.push(A(20_000));
        } else {
            for i in 1..=10i64 {
                let t = i * 1000 + if i as usize == bad { 150 } else { 0 };
                acts.extend([P(t), R(t + 1)]);
            }
        }
        run_with(notes, Some(1000.0), g, &acts).0
    }

    /// DOM-LRP-09: ランプ。最大コンボ = 総ノーツ数なら FULL COMBO、全ノーツ処理済みでゲージがクリア条件を満たせば
    /// HARD・G-ATTACK・P-ATTACK は HARD、EASY は EASY、他は NORMAL。それ以外は FAILED
    #[rstest]
    #[case::full_combo(GaugeType::Groove, 0, false, ClearLamp::FullCombo)]
    #[case::normal(GaugeType::Groove, 5, false, ClearLamp::Normal)]
    #[case::easy(GaugeType::Easy, 5, false, ClearLamp::Easy)]
    #[case::hard(GaugeType::Hard, 5, false, ClearLamp::Hard)]
    #[case::death_bad_dies(GaugeType::Hazard, 5, false, ClearLamp::Failed)]
    #[case::failed_gauge(GaugeType::Groove, 0, true, ClearLamp::Failed)]
    fn dom_lrp_09_lamp(
        #[case] g: GaugeType,
        #[case] bad: usize,
        #[case] miss: bool,
        #[case] want: ClearLamp,
    ) {
        assert_eq!(ten(g, bad, miss).clear_lamp(), want);
    }

    /// DOM-LRP-10: 記録は EX スコア = PG×2 + GR、最小 BP = BD + POOR、プレイ回数 1、ルール LR2
    #[rstest]
    #[case::all_pg(0, 20, 0)]
    #[case::one_bad(5, 18, 1)]
    fn dom_lrp_10_record(#[case] bad: usize, #[case] ex: u32, #[case] bp: u32) {
        let r = ten(GaugeType::Groove, bad, false).record(ChartHash {
            md5: [7; 16],
            sha256: None,
        });
        assert_eq!(
            (r.ex_score, r.min_bp, r.play_count, r.ruleset, r.origin),
            (ex, Some(bp), 1, RulesetId::Lr2, ScoreOrigin::Native)
        );
    }
}
