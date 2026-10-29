//! LR2 のプレイ進行（DOM-LRP-*）。フレームごとの入力から判定・コンボ・ゲージ・ランプを進める。
//! OpenLR2 の `Scene04_Play.cpp`（`ProcGame` のフレーム処理・`ProcNoteOnTiming`・`ProcSinglenote`・
//! `ProcLongnote`・`ApplyJudgeNote`・`ApplyJudgeMine`）、`LR2_bmsload.cpp`（レーンへの配置）、
//! `LR2_statplay.cpp`（`CheckClear`）を仕様として参照した（コードは転載していない）。
//! BPM 変化直後の時刻補正（`bpmChangedRealtime`）は未対応。
use super::{
    lr2::{Lr2Gauge, Lr2Ruleset},
    GaugeType, Judge, JudgeLane, Ruleset,
};
use super::{RulesetId, TimingMap};
use crate::chart::{Chart, ChartHash, Lane, NoteKind};
use crate::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Normal,
    /// 終端の時刻（ms）
    Long(f64),
    /// 地雷の生の値
    Mine(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LaneNote {
    /// 時刻（ms。LR2 と同じく小数を持つ）
    time: f64,
    kind: Kind,
}

#[derive(Debug, Default)]
struct LaneState {
    notes: VecDeque<LaneNote>,
    /// 押下中の LN の始点判定と差（ms）
    active: Option<(Judge, i64)>,
}

/// そのフレームでの鍵盤の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyState {
    #[default]
    Off,
    /// このフレームで押した
    Pressed,
    /// 押し続けている
    Held,
    /// このフレームで離した
    Released,
}

/// プレイ中に起きたこと（表示・音のため）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayEvent {
    /// 判定。`delta_ms` はフレーム時刻 − ノーツ時刻（見逃しは `None`。LN は始点の差）
    Judge {
        lane: Lane,
        judge: Judge,
        delta_ms: Option<i64>,
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
    /// PG, GR, GD, BD（ms）
    windows: [i64; 4],
    gauge_type: GaugeType,
    gauge: Lr2Gauge,
    total: u32,
    processed: u32,
    combo: u32,
    max_combo: u32,
    counts: JudgeCounts,
    dead: bool,
}

/// 空POOR になる早押しの上限（ms 未満）
const EMPTY_POOR_MS: i64 = 1000;

/// 譜面の音符を LR2 の規則でレーンに並べる（DOM-LRP-01）
fn build_lanes(chart: &Chart, timing: &TimingMap) -> BTreeMap<Lane, LaneState> {
    let ms = |b| timing.time_of(b) as f64 / 1000.0;
    let mut sorted: Vec<(f64, usize)> = chart
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| !matches!(n.kind, NoteKind::Invisible))
        .map(|(i, n)| (ms(n.beat), i))
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut lanes: BTreeMap<Lane, LaneState> = BTreeMap::new();
    for (time, i) in sorted {
        let n = &chart.notes[i];
        let kind = match &n.kind {
            NoteKind::Normal => Kind::Normal,
            NoteKind::Long { end, .. } => Kind::Long(ms(*end)),
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
                    _ => -1.0,
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
        let w = Lr2Ruleset.judge_windows(&chart.meta, JudgeLane::Key);
        Some(Self {
            lanes,
            windows: w.grades.map(|g| g.late / 1000),
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

    /// 1 フレーム分を処理する（DOM-LRP-11）。`t_ms` はフレームの時刻、`keys` は各レーンの鍵盤の状態。
    /// レーンを番号順に 1 回ずつ処理し、最後にゲージが 2 未満なら途中終了（DOM-LRP-08）
    pub fn tick(&mut self, t_ms: i64, keys: impl Fn(Lane) -> KeyState) -> Vec<PlayEvent> {
        let mut out = vec![];
        if self.dead {
            return out;
        }
        let lanes: Vec<Lane> = self.lanes.keys().copied().collect();
        for lane in lanes {
            let key = keys(lane);
            match self.lanes[&lane].notes.front().map(|n| n.kind) {
                None => {}
                Some(Kind::Long(end)) => self.proc_long(lane, key, t_ms, end, &mut out),
                Some(_) => self.proc_single(lane, key, t_ms, &mut out),
            }
        }
        if self.gauge.hp < 2.0 {
            self.dead = true;
        }
        out
    }

    /// 判定を記録に反映する（DOM-LRP-07）
    fn score(&mut self, lane: Lane, judge: Judge, delta_ms: Option<i64>, out: &mut Vec<PlayEvent>) {
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
            delta_ms,
        });
    }

    /// PG..BD の判定（差の絶対値が幅以下）
    fn grade(&self, gap: i64) -> Option<Judge> {
        const G: [Judge; 4] = [Judge::PGreat, Judge::Great, Judge::Good, Judge::Bad];
        self.windows.iter().position(|w| gap <= *w).map(|i| G[i])
    }

    fn pop(&mut self, lane: Lane) {
        self.lanes.get_mut(&lane).unwrap().notes.pop_front();
    }

    /// 通常ノーツ・地雷（DOM-LRP-02..04, 06）
    fn proc_single(&mut self, lane: Lane, key: KeyState, t: i64, out: &mut Vec<PlayEvent>) {
        let Some(n) = self.lanes[&lane].notes.front().copied() else {
            return;
        };
        let tf = t as f64;
        if let Kind::Mine(dmg) = n.kind {
            let fire = if key == KeyState::Pressed && n.time - tf < self.windows[1] as f64 {
                true
            } else if tf <= n.time {
                return;
            } else {
                matches!(key, KeyState::Pressed | KeyState::Held)
            };
            if fire {
                self.counts.poor += 1;
                self.gauge.apply_mine(dmg as f64);
                out.push(PlayEvent::Mine { lane, damage: dmg });
            }
            self.pop(lane);
            return;
        }
        if tf - n.time > self.windows[3] as f64 {
            self.pop(lane);
            self.score(lane, Judge::Poor, None, out);
            return;
        }
        if key != KeyState::Pressed {
            return;
        }
        let offset = t - n.time as i64;
        match self.grade(offset.abs()) {
            Some(j) => {
                self.pop(lane);
                self.score(lane, j, Some(offset), out);
                let bd = self.windows[3];
                let chain = self.lanes[&lane]
                    .notes
                    .front()
                    .is_some_and(|m| (t - m.time as i64).abs() <= bd);
                if j == Judge::Bad && chain {
                    self.proc_single(lane, KeyState::Pressed, t, out);
                }
            }
            None if (n.time as i64) - t < EMPTY_POOR_MS => {
                self.score(lane, Judge::EmptyPoor, Some(offset), out)
            }
            None => {}
        }
    }

    /// LN（DOM-LRP-03..05）
    fn proc_long(&mut self, lane: Lane, key: KeyState, t: i64, end: f64, out: &mut Vec<PlayEvent>) {
        let n = self.lanes[&lane].notes[0];
        let active = self.lanes[&lane].active;
        let gd = self.windows[2];
        if t as f64 - n.time > gd as f64 && active.is_none() {
            self.pop(lane);
            self.score(lane, Judge::Poor, None, out);
            return;
        }
        match (key, active) {
            (KeyState::Pressed, _) => {
                let offset = t - n.time as i64;
                match self.grade(offset.abs()) {
                    Some(j) => self.lanes.get_mut(&lane).unwrap().active = Some((j, offset)),
                    None if (n.time as i64) - t < EMPTY_POOR_MS => {
                        self.score(lane, Judge::EmptyPoor, Some(offset), out)
                    }
                    None => {}
                }
            }
            (KeyState::Held, Some((j, d))) if (end as i64) < t => {
                self.lanes.get_mut(&lane).unwrap().active = None;
                self.pop(lane);
                self.score(lane, j, Some(d), out);
            }
            (KeyState::Released, Some((j, d))) => {
                self.lanes.get_mut(&lane).unwrap().active = None;
                self.pop(lane);
                let j = if gd + t < end as i64 { Judge::Bad } else { j };
                self.score(lane, j, Some(d), out);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{Beat, ChartMeta, KeyMode, Note};
    use rstest::rstest;
    use KeyState::*;

    // BPM 60: 1 拍 = 1 秒。ノーツの位置は ms で書く。判定幅は #RANK 2（18/40/100/200ms）
    fn note_at(lane: u8, ms: f64, kind: NoteKind) -> Note {
        Note {
            beat: Beat(ms / 1000.0),
            lane: Lane(lane),
            kind,
            sound: None,
        }
    }
    fn n(ms: i64) -> Note {
        note_at(1, ms as f64, NoteKind::Normal)
    }
    fn n2(ms: i64) -> Note {
        note_at(2, ms as f64, NoteKind::Normal)
    }
    fn ln(s: i64, e: i64) -> Note {
        let end = Beat(e as f64 / 1000.0);
        note_at(
            1,
            s as f64,
            NoteKind::Long {
                end,
                end_sound: None,
            },
        )
    }
    fn mine(ms: i64, damage_raw: u32) -> Note {
        note_at(1, ms as f64, NoteKind::Mine { damage_raw })
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

    /// 1 フレーム: (時刻 ms, レーン 1 の状態, レーン 2 の状態)
    type Frame = (i64, KeyState, KeyState);
    fn p(t: i64) -> Frame {
        (t, Pressed, Off)
    }
    fn h(t: i64) -> Frame {
        (t, Held, Off)
    }
    fn r(t: i64) -> Frame {
        (t, Released, Off)
    }
    fn o(t: i64) -> Frame {
        (t, Off, Off)
    }

    fn run_with(
        notes: Vec<Note>,
        total: Option<f64>,
        g: GaugeType,
        frames: &[Frame],
    ) -> (Lr2Play, Vec<PlayEvent>) {
        let c = chart(notes, total);
        let mut play = Lr2Play::new(&c, &TimingMap::from_chart(&c), g).unwrap();
        let mut ev = vec![];
        for &(t, k1, k2) in frames {
            ev.extend(play.tick(t, |l| match l.0 {
                1 => k1,
                2 => k2,
                _ => Off,
            }));
        }
        (play, ev)
    }
    fn run(notes: Vec<Note>, frames: &[Frame]) -> (Lr2Play, Vec<PlayEvent>) {
        run_with(notes, None, GaugeType::Groove, frames)
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
    #[case::invisible_ignored(vec![note_at(1, 1000.0, NoteKind::Invisible)], 0, vec![])]
    fn dom_lrp_01_lanes(#[case] notes: Vec<Note>, #[case] total: u32, #[case] times_ms: Vec<i64>) {
        let (play, _) = run(notes, &[]);
        assert_eq!(play.total_notes(), total);
        let got: Vec<i64> = play
            .lanes
            .get(&Lane(1))
            .map(|l| l.notes.iter().map(|x| x.time as i64).collect())
            .unwrap_or_default();
        assert_eq!(got, times_ms);
    }

    /// DOM-LRP-02: 押したフレームで先頭ノーツを判定して片付ける。差 = フレーム時刻 − ⌊ノーツ時刻⌋（ms）。
    /// BD のときは次のノーツが ±BD 内なら同じ押下で続けて判定する
    #[rstest]
    #[case::pg(vec![n(1000)], 1000, vec![Judge::PGreat])]
    #[case::pg_edge(vec![n(1000)], 1018, vec![Judge::PGreat])]
    #[case::gr_early(vec![n(1000)], 960, vec![Judge::Great])]
    #[case::gd(vec![n(1000)], 1100, vec![Judge::Good])]
    #[case::bd_early(vec![n(1000)], 800, vec![Judge::Bad])]
    #[case::truncated_note_time(vec![note_at(1, 1000.5, NoteKind::Normal)], 1018, vec![Judge::PGreat])]
    #[case::bd_chains_next(vec![n(1000), n(1150)], 1200, vec![Judge::Bad, Judge::Good])]
    #[case::bd_next_out_of_range(vec![n(1000), n(1450)], 1200, vec![Judge::Bad])]
    #[case::gd_does_not_chain(vec![n(1000), n(1050)], 1090, vec![Judge::Good])]
    fn dom_lrp_02_press(#[case] notes: Vec<Note>, #[case] at: i64, #[case] want: Vec<Judge>) {
        let (_, ev) = run(notes, &[p(at)]);
        assert_eq!(judges(&ev), want);
    }

    /// DOM-LRP-03: 先頭ノーツに当たらず早 1000ms 未満なら空POOR（ノーツは残る）。レーンが空なら何もしない
    #[rstest]
    #[case::just_outside_bd(vec![n(1000)], &[p(799)], vec![Judge::EmptyPoor], 1)]
    #[case::edge_999(vec![n(1000)], &[p(1)], vec![Judge::EmptyPoor], 1)]
    #[case::too_early(vec![n(1000)], &[p(0)], vec![], 1)]
    #[case::note_remains(vec![n(1000)], &[p(500), r(600), p(1000)], vec![Judge::EmptyPoor, Judge::PGreat], 0)]
    #[case::ln_head(vec![ln(1000, 2000)], &[p(500)], vec![Judge::EmptyPoor], 1)]
    #[case::empty_lane(vec![n(1000)], &[p(1000), r(1100), p(1500)], vec![Judge::PGreat], 0)]
    fn dom_lrp_03_empty_poor(
        #[case] notes: Vec<Note>,
        #[case] frames: &[Frame],
        #[case] want: Vec<Judge>,
        #[case] remaining: usize,
    ) {
        let (play, ev) = run(notes, frames);
        assert_eq!(judges(&ev), want);
        assert_eq!(play.lanes[&Lane(1)].notes.len(), remaining);
    }

    /// DOM-LRP-04: 見逃し。通常ノーツはフレーム時刻 − ノーツ時刻が BD を、押していない LN は GD を超えたら POOR
    #[rstest]
    #[case::normal_at_edge(vec![n(1000)], 1200, vec![])]
    #[case::normal_past(vec![n(1000)], 1201, vec![Judge::Poor])]
    #[case::ln_at_edge(vec![ln(1000, 2000)], 1100, vec![])]
    #[case::ln_past(vec![ln(1000, 2000)], 1101, vec![Judge::Poor])]
    fn dom_lrp_04_miss(#[case] notes: Vec<Note>, #[case] at: i64, #[case] want: Vec<Judge>) {
        let (_, ev) = run(notes, &[o(at)]);
        assert_eq!(judges(&ev), want);
    }

    /// DOM-LRP-05: LN は押したフレームで始点の判定を保留し、押し続けて ⌊終端⌋ を過ぎたフレームでその判定。
    /// 離したフレームで フレーム時刻 + GD < ⌊終端⌋ なら BAD、そうでなければ始点の判定。
    /// 始点を GD の遅い側より後に押しても見逃し（DOM-LRP-04）が先
    #[rstest]
    #[case::held_to_end(&[p(1000), h(2000)], vec![])]
    #[case::held_past_end(&[p(1000), h(2001)], vec![Judge::PGreat])]
    #[case::head_kept(&[p(1050), r(1950)], vec![Judge::Good])]
    #[case::released_early(&[p(1000), r(1899)], vec![Judge::Bad])]
    #[case::release_at_gd_edge(&[p(1000), r(1900)], vec![Judge::PGreat])]
    #[case::head_bad_early(&[p(850), h(2001)], vec![Judge::Bad])]
    #[case::late_head_past_gd_is_poor(&[p(1150)], vec![Judge::Poor])]
    #[case::released_after_end(&[p(1000), r(2500)], vec![Judge::PGreat])]
    #[case::off_frame_does_not_finish(&[p(1000), o(2500)], vec![])]
    fn dom_lrp_05_long_note(#[case] frames: &[Frame], #[case] want: Vec<Judge>) {
        let (_, ev) = run(vec![ln(1000, 2000)], frames);
        assert_eq!(judges(&ev), want);
    }

    /// DOM-LRP-06: 地雷。押したフレームで 地雷 − フレーム時刻 < GR（過ぎていても）なら発動。
    /// 地雷を過ぎたフレームで押下中なら発動、押していなければ消える。発動で HP − 生の値（GROOVE/EASY は 2 で止まる）、POOR +1
    #[rstest]
    #[case::press_near(GaugeType::Hard, &[p(970)], 90.0, 1)]
    #[case::press_too_early(GaugeType::Hard, &[p(959), r(980), o(1001)], 100.0, 0)]
    #[case::held_through(GaugeType::Hard, &[p(500), h(1001)], 90.0, 1)]
    #[case::passed_unheld(GaugeType::Hard, &[o(1001), p(1100)], 100.0, 0)]
    #[case::late_press_same_frame(GaugeType::Hard, &[p(1100)], 90.0, 1)]
    #[case::groove_floor(GaugeType::Groove, &[p(1000)], 20.0 - 10.0, 1)]
    fn dom_lrp_06_mine(
        #[case] g: GaugeType,
        #[case] frames: &[Frame],
        #[case] hp: f64,
        #[case] hits: u32,
    ) {
        let (play, ev) = run_with(vec![mine(1000, 10)], None, g, frames);
        assert!((play.gauge() - hp).abs() < 1e-9, "{}", play.gauge());
        assert_eq!(play.counts().poor, hits);
        let fired = ev
            .iter()
            .filter(|e| matches!(e, PlayEvent::Mine { .. }))
            .count();
        assert_eq!(fired as u32, hits);
    }

    /// DOM-LRP-07: コンボは PG/GR/GD で +1、BD/POOR で 0、空POOR・地雷では変わらない。最大コンボを記録
    #[rstest]
    #[case::good_continues(vec![n(1000)], &[p(1100)], 1, 1)]
    #[case::bad_resets(vec![n(1000), n(2000)], &[p(1000), r(1001), p(2200)], 0, 1)]
    #[case::poor_resets(vec![n(1000), n(2000)], &[p(1000), r(1001), o(2201)], 0, 1)]
    #[case::empty_poor_keeps(vec![n(1000), n(3000)], &[p(1000), r(1001), p(2500)], 1, 1)]
    #[case::mine_keeps(vec![n(1000), mine(2000, 1)], &[p(1000), r(1001), p(2000)], 1, 1)]
    #[case::max_kept(vec![n(1000), n(2000), n(3000)], &[p(1000), r(1001), p(2000), r(2001), o(3201)], 0, 2)]
    fn dom_lrp_07_combo(
        #[case] notes: Vec<Note>,
        #[case] frames: &[Frame],
        #[case] combo: u32,
        #[case] max: u32,
    ) {
        let (play, _) = run(notes, frames);
        assert_eq!((play.combo(), play.max_combo()), (combo, max));
    }

    /// DOM-LRP-08: 全ノーツを処理したら終了。フレームの終わりにゲージが 2 未満なら途中終了し、以後のフレームは無視
    /// （同じフレームの残りのレーンは処理する）
    #[rstest]
    #[case::all_done(GaugeType::Groove, vec![n(1000)], &[p(1000)], true, false, 1)]
    #[case::not_yet(GaugeType::Groove, vec![n(1000), n(2000)], &[p(1000)], false, false, 1)]
    #[case::dead_ignores_later_frames(GaugeType::Hard, vec![mine(1000, 99), n(2000)], &[p(1000), r(1001), p(2000)], true, true, 0)]
    #[case::rest_of_frame_processed(GaugeType::Hard, vec![mine(1000, 99), n2(1000)], &[(1000, Pressed, Pressed)], true, true, 1)]
    #[case::groove_never_dies(GaugeType::Groove, vec![mine(1000, 99), n(2000)], &[p(1000), r(1001), p(2000)], true, false, 1)]
    fn dom_lrp_08_finish(
        #[case] g: GaugeType,
        #[case] notes: Vec<Note>,
        #[case] frames: &[Frame],
        #[case] finished: bool,
        #[case] dead: bool,
        #[case] judged: usize,
    ) {
        let (play, ev) = run_with(notes, None, g, frames);
        assert_eq!(
            (play.is_finished(), play.is_dead(), judges(&ev).len()),
            (finished, dead, judged)
        );
    }

    /// 10 ノーツ（1..=10 秒）。`bad` 番目だけ +150ms で押す（0 なら全 PG）。`miss_all` なら毎秒見逃す
    fn ten(g: GaugeType, bad: usize, miss_all: bool) -> Lr2Play {
        let notes = (1..=10).map(|i| n(i * 1000)).collect();
        let mut frames = vec![];
        for i in 1..=10i64 {
            if miss_all {
                frames.push(o(i * 1000 + 201));
            } else {
                let t = i * 1000 + if i as usize == bad { 150 } else { 0 };
                frames.extend([p(t), r(t + 1)]);
            }
        }
        run_with(notes, Some(1000.0), g, &frames).0
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
        let rec = ten(GaugeType::Groove, bad, false).record(ChartHash {
            md5: [7; 16],
            sha256: None,
        });
        assert_eq!(
            (
                rec.ex_score,
                rec.min_bp,
                rec.play_count,
                rec.ruleset,
                rec.origin
            ),
            (ex, Some(bp), 1, RulesetId::Lr2, ScoreOrigin::Native)
        );
    }

    /// DOM-LRP-11: フレーム処理。各レーンを番号順に 1 回ずつ処理し、1 レーンで片付くのは先頭ノーツ 1 つ
    /// （BD の続け判定を除く）
    #[rstest]
    #[case::one_per_lane_per_frame(vec![n(1000), n(1100)], &[o(5000)], vec![(1, Judge::Poor)])]
    #[case::next_frame_next_note(vec![n(1000), n(1100)], &[o(5000), o(5001)], vec![(1, Judge::Poor), (1, Judge::Poor)])]
    #[case::lane_order_not_time_order(vec![n2(1000), n(1100)], &[o(5000)], vec![(1, Judge::Poor), (2, Judge::Poor)])]
    #[case::same_frame_both_pressed(vec![n2(1000), n(1000)], &[(1010, Pressed, Pressed)], vec![(1, Judge::PGreat), (2, Judge::PGreat)])]
    fn dom_lrp_11_frame(
        #[case] notes: Vec<Note>,
        #[case] frames: &[Frame],
        #[case] want: Vec<(u8, Judge)>,
    ) {
        let (_, ev) = run(notes, frames);
        let got: Vec<(u8, Judge)> = ev
            .iter()
            .filter_map(|e| match e {
                PlayEvent::Judge { lane, judge, .. } => Some((lane.0, *judge)),
                _ => None,
            })
            .collect();
        assert_eq!(got, want);
    }
}
