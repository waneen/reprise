//! LR2 スキンでプレイ画面を描くための状態（APP-SKN-*）。`PlaySession` の進み具合から
//! LR2 の番号のタイマー・条件・数値を作り、`domain::skin::lr2::SkinState` として渡す。
//! 番号の意味は OpenLR2 の `LR2_skinobject.cpp`・`Scene04_Play.cpp`・`LR2_gameloop.cpp` を仕様として参照した。
use crate::play::PlaySession;
use domain::chart::{Chart, KeyMode, Lane, NoteKind, TimingEvent};
use domain::play::{GaugeType, Judge, KeyState, PlayEvent};
use domain::skin::lr2::{NoteView, PlayView, SkinState};
use std::collections::{BTreeMap, BTreeSet};

/// NOWJUDGE の添字（5=PG … 1=POOR, 0=空POOR）
pub fn judge_index(j: Judge) -> usize {
    match j {
        Judge::PGreat => 5,
        Judge::Great => 4,
        Judge::Good => 3,
        Judge::Bad => 2,
        Judge::Poor => 1,
        Judge::EmptyPoor => 0,
    }
}

/// プレイ画面のスキン用の状態。時刻はすべて譜面の時刻（ms、譜面の先頭 = 0）
pub struct PlaySkin {
    /// 画面が始まってから譜面の先頭までの時間
    lead_in_ms: i64,
    now: i64,
    /// 動いているタイマーと、動き始めた時刻
    starts: BTreeMap<i32, i64>,
    /// プレイ中に変わらない条件
    fixed: BTreeSet<i32>,
    survival: bool,
    key_mode: KeyMode,
    /// 小節の始まりの拍
    measures: Vec<f64>,
    /// 最後のオブジェクトの時刻（曲の長さ）
    runtime_ms: i64,
    judge: Option<usize>,
    last_miss: Option<i64>,
    /// 前のフレームの表示 HP（偶数）
    shown_hp: i64,
    angle: f32,
    numbers: BTreeMap<i32, i64>,
    hp: f64,
    beat: f64,
    combo: i64,
    /// 片付いたノーツの数（空POOR・地雷を除く判定の数）
    processed: i64,
}

impl PlaySkin {
    /// `skin_options` はスキンの読み込みで真になった条件（スキン設定の選択など）
    pub fn new(
        chart: &Chart,
        session: &PlaySession,
        gauge: GaugeType,
        lead_in_ms: i64,
        skin_options: &[i32],
    ) -> Self {
        let timing = session.timing();
        let survival = !matches!(gauge, GaugeType::Groove | GaugeType::Easy);
        let has_ln = chart
            .notes
            .iter()
            .any(|n| matches!(n.kind, NoteKind::Long { .. }));
        let mut fixed: BTreeSet<i32> = skin_options.iter().copied().collect();
        fixed.extend([
            32, // オートプレイでない
            40, // BGA を描かない
            if survival { 43 } else { 42 },
            81, // 読み込み済み
            if chart.bga.is_empty() { 170 } else { 171 },
            if has_ln { 173 } else { 172 },
        ]);
        fixed.extend(match chart.key_mode {
            KeyMode::K7 => Some(160),
            KeyMode::K5 => Some(161),
            KeyMode::K14 => Some(162),
            KeyMode::K10 => Some(163),
            KeyMode::K9 => Some(164),
        });
        // 曲の長さ = 最後のノーツ（LN は終端）か BGM の時刻
        let last_beat = chart
            .notes
            .iter()
            .map(|n| match n.kind {
                NoteKind::Long { end, .. } => end.0,
                _ => n.beat.0,
            })
            .chain(chart.bgm.iter().map(|b| b.0 .0))
            .fold(0.0, f64::max);
        let runtime_ms = timing.time_of(domain::chart::Beat(last_beat)) / 1000;
        // 小節の始まり（小節の長さ × 4 拍）
        let lengths: BTreeMap<u32, f64> = chart
            .timing
            .iter()
            .filter_map(|e| match *e {
                TimingEvent::Measure { index, length } => Some((index, length)),
                _ => None,
            })
            .collect();
        let mut measures = vec![0.0];
        let mut b = 0.0;
        let mut i = 0;
        while b <= last_beat {
            b += 4.0 * lengths.get(&i).copied().unwrap_or(1.0);
            measures.push(b);
            i += 1;
        }
        let mut s = Self {
            lead_in_ms,
            now: -lead_in_ms,
            starts: BTreeMap::new(),
            fixed,
            survival,
            key_mode: chart.key_mode,
            measures,
            runtime_ms,
            judge: None,
            last_miss: None,
            shown_hp: 0,
            angle: 0.0,
            numbers: BTreeMap::new(),
            hp: 0.0,
            beat: 0.0,
            combo: 0,
            processed: 0,
        };
        // 画面の開始・入力開始・準備完了
        for t in [0, 1, 40] {
            s.starts.insert(t, -lead_in_ms);
        }
        s.update(-lead_in_ms, session, &[]);
        s
    }

    fn set(&mut self, timer: i32) {
        self.starts.insert(timer, self.now);
    }
    fn set_once(&mut self, timer: i32) {
        self.starts.entry(timer).or_insert(self.now);
    }
    fn reset(&mut self, timer: i32) {
        self.starts.remove(&timer);
    }

    /// 1 フレーム進める。`events` はそのフレームの `PlaySession::frame` の結果
    pub fn update(&mut self, t_ms: i64, session: &PlaySession, events: &[PlayEvent]) {
        let dt = (t_ms - self.now).max(0);
        self.now = t_ms;
        let play = session.play();
        // プレイ開始 = 譜面の先頭
        if t_ms >= 0 {
            self.starts.entry(41).or_insert(0);
        }
        // 判定（APP-SKN-02）
        for e in events {
            match *e {
                PlayEvent::Judge { lane, judge, .. } => {
                    if judge != Judge::EmptyPoor {
                        self.processed += 1;
                    }
                    let j = judge_index(judge);
                    self.judge = Some(j);
                    if j < 3 {
                        self.last_miss = Some(t_ms);
                    }
                    if j >= 4 && lane.0 < 10 {
                        self.set(50 + lane.0 as i32);
                    }
                    self.set(46);
                }
                PlayEvent::Mine { .. } => self.set(46),
                _ => {}
            }
        }
        // キー（APP-SKN-03）
        for (lane, k) in session.keys() {
            let l = lane.0 as i32;
            match k {
                KeyState::Pressed => {
                    self.set(100 + l);
                    self.reset(120 + l);
                }
                KeyState::Released => {
                    self.set(120 + l);
                    self.reset(100 + l);
                }
                _ => {}
            }
        }
        // 押下中の LN
        let held: BTreeSet<u8> = play
            .pending_notes()
            .iter()
            .filter(|n| n.held)
            .map(|n| n.lane.0)
            .collect();
        for l in 0..10u8 {
            if held.contains(&l) {
                self.set_once(70 + l as i32);
            } else {
                self.reset(70 + l as i32);
            }
        }
        // ゲージ（APP-SKN-04）
        let hp = play.gauge();
        let even = (hp as i64) / 2 * 2;
        if even >= 100 {
            self.set_once(44);
            self.reset(42);
        } else {
            if even > self.shown_hp {
                self.set(42);
            }
            self.reset(44);
        }
        self.shown_hp = even;
        if play.is_dead() {
            self.set_once(3);
            self.reset(44);
        }
        let total = play.total_notes();
        if total > 0 && play.combo() == total {
            self.set_once(48);
        }
        if play.is_finished() && !play.is_dead() {
            self.set_once(143);
        }
        // 皿の角度: 皿を押している間は −720°/s、プレイ開始後は 360°/s
        let held_scratch = matches!(
            session.keys().get(&Lane(0)),
            Some(KeyState::Pressed | KeyState::Held)
        );
        let rate = if held_scratch {
            -720.0
        } else if t_ms >= 0 {
            360.0
        } else {
            0.0
        };
        self.angle = (self.angle + rate * dt as f32 / 1000.0).rem_euclid(360.0);
        // 数値（APP-SKN-05）
        let c = play.counts();
        let ex = c.ex_score() as i64;
        let total = total as i64;
        let processed = self.processed;
        let mut n = BTreeMap::new();
        if total > 0 {
            let score = (c.good as i64 + c.great as i64 * 2 + c.pgreat as i64 * 4) * 50000 / total;
            n.insert(
                100,
                if matches!(self.key_mode, KeyMode::K7 | KeyMode::K14) {
                    score
                } else {
                    score / 20 * 10
                },
            );
            n.insert(115, ex * 100 / (total * 2));
        }
        n.insert(101, ex);
        if processed > 0 {
            let rate = ex as f64 * 100.0 / (processed * 2) as f64;
            n.insert(102, rate as i64);
            n.insert(103, (rate * 100.0) as i64 % 100);
        }
        n.insert(104, play.combo() as i64);
        n.insert(105, play.max_combo() as i64);
        n.insert(106, total);
        n.insert(107, even);
        n.insert(110, c.pgreat as i64);
        n.insert(111, c.great as i64);
        n.insert(112, c.good as i64);
        n.insert(113, c.bad as i64);
        n.insert(114, c.poor as i64);
        n.insert(160, session.timing().bpm_at(t_ms * 1000) as i64);
        if t_ms >= 1000 {
            n.insert(161, t_ms / 1000 / 60);
            n.insert(162, t_ms / 1000 % 60);
        }
        let rest = self.runtime_ms - t_ms;
        if t_ms >= 0 && rest >= 1000 {
            n.insert(163, rest / 1000 / 60);
            n.insert(164, rest / 1000 % 60);
        }
        self.numbers = n;
        self.hp = hp;
        self.beat = session.timing().beat_at(t_ms * 1000).0;
        self.combo = play.combo() as i64;
    }

    /// 曲の進み具合（0..=1）。プレイ開始前は 0
    fn progress(&self) -> f64 {
        if self.now < 0 || self.runtime_ms <= 0 {
            return 0.0;
        }
        (self.now as f64 / self.runtime_ms as f64).min(1.0)
    }

    /// 描画材料（APP-SKN-06）。`visible_beats` 拍先まで、`tremble` はゲージのちらつき用の乱数（0..=2）
    pub fn view(
        &self,
        session: &PlaySession,
        hi_speed: f64,
        visible_beats: f64,
        tremble: i32,
    ) -> PlayView {
        let notes = session
            .visible_notes(self.now, visible_beats)
            .into_iter()
            .map(|v| NoteView {
                lane: v.lane.0 as i32,
                head: v.y * visible_beats,
                tail: v.y_end.map(|y| y * visible_beats),
                mine: v.mine,
                held: v.held,
            })
            .collect();
        PlayView {
            judge: self.judge,
            combo: self.combo,
            gauge: (self.hp as i32) / 2,
            survival: self.survival,
            tremble,
            hi_speed,
            notes,
        }
    }

    /// 画面の開始からの時間
    pub fn scene_ms(&self) -> i64 {
        self.now + self.lead_in_ms
    }
}

impl SkinState for PlaySkin {
    fn timer(&self, id: i32) -> Option<i64> {
        if id == 140 {
            // 1 拍 = 1000、小節線で 0 に戻る
            if self.now < 0 {
                return None;
            }
            let i = self.measures.partition_point(|m| *m <= self.beat);
            let start = self.measures[i.saturating_sub(1)];
            return Some(((self.beat - start) * 1000.0) as i64);
        }
        self.starts.get(&id).map(|s| self.now - s)
    }

    fn option(&self, id: i32) -> bool {
        if self.fixed.contains(&id) {
            return true;
        }
        match id {
            230..=239 => {
                let lo = (id - 230) as f64 * 10.0;
                self.hp >= lo && self.hp < lo + 10.0
            }
            240 => self.hp >= 100.0,
            241..=246 => self.judge == Some((246 - id) as usize),
            247 => self.last_miss.is_none_or(|m| self.now - m >= 1000),
            248 => self.last_miss.is_some_and(|m| self.now - m < 1000),
            _ => false,
        }
    }

    fn number(&self, id: i32) -> Option<i64> {
        self.numbers.get(&id).copied()
    }

    fn bar(&self, kind: i32) -> f64 {
        match kind {
            1 => self.progress(),
            _ => 0.0,
        }
    }

    fn slider(&self, kind: i32) -> f64 {
        match kind {
            6 => self.progress(),
            _ => 0.0,
        }
    }

    fn turntable(&self, side: i32) -> f32 {
        if side == 1 {
            self.angle
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests;
