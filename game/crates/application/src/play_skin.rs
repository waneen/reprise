//! LR2 スキンでプレイ画面を描くための状態（APP-SKN-*）。`PlaySession` の進み具合から
//! LR2 の番号のタイマー・条件・数値を作り、`domain::skin::lr2::SkinState` として渡す。
//! 番号の意味は OpenLR2 の `LR2_skinobject.cpp`・`Scene04_Play.cpp`・`LR2_gameloop.cpp` を仕様として参照した。
use crate::play::PlaySession;
use domain::chart::{Chart, KeyMode, NoteKind, SoundId, TimingEvent};
use domain::play::{GaugeType, Judge, KeyState, PlayEvent};
use domain::skin::lr2::{NoteView, PlayView, SkinState};
use domain::skin::SceneTiming;
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

/// 値を時間をかけて追いかける表示（LR2 の `HP_print`・`score_print`）
#[derive(Debug, Clone, Copy)]
struct Follow {
    from: f64,
    to: f64,
    t_from: i64,
    t_to: i64,
}

impl Follow {
    fn new(v: f64) -> Self {
        Self {
            from: v,
            to: v,
            t_from: 0,
            t_to: 0,
        }
    }
    /// 時刻 `now` の表示値（LR2 `ChangeValueByTime` の線形）
    fn at(&self, now: i64) -> f64 {
        if self.from == self.to {
            return self.to;
        }
        if self.t_from <= now && now <= self.t_to && self.t_from < self.t_to {
            let r = (now - self.t_from) as f64 / (self.t_to - self.t_from) as f64;
            return (1.0 - r) * self.from + r * self.to;
        }
        if self.t_from < now {
            self.to
        } else {
            self.from
        }
    }
    /// 目標が変わったら、今の表示値から `dur` ms かけて追いかけ直す
    fn retarget(&mut self, v: f64, now: i64, dur: i64) {
        if v != self.to {
            self.from = self.at(now);
            self.to = v;
            self.t_from = now;
            self.t_to = now + dur;
        }
    }
}

/// プレイ画面のスキン用の状態。時刻はすべて譜面の時刻（ms、譜面の先頭 = 0）
pub struct PlaySkin {
    /// 画面が始まってから譜面の先頭（タイマー 41）までの時間
    lead_in_ms: i64,
    timing: SceneTiming,
    /// 皿を回している向き（−1 / 0 / 1）
    scratch: i32,
    hp_print: Follow,
    score_print: Follow,
    now: i64,
    /// 動いているタイマーと、動き始めた時刻
    starts: BTreeMap<i32, i64>,
    /// プレイ中に変わらない条件
    fixed: BTreeSet<i32>,
    survival: bool,
    key_mode: KeyMode,
    /// 小節の始まりの拍
    measures: Vec<f64>,
    /// 曲の長さ（オブジェクトの時刻 + 音の長さ の最大）
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
    /// 文字列（APP-SKN-10）
    texts: BTreeMap<i32, String>,
}

impl PlaySkin {
    /// `skin_options` はスキンの読み込みで真になった条件（スキン設定の選択など）
    /// `timing` はスキンの場面の時間、`sound_ms` は音の長さ（曲の長さに使う）
    pub fn new(
        chart: &Chart,
        session: &PlaySession,
        gauge: GaugeType,
        timing: SceneTiming,
        skin_options: &[i32],
        sound_ms: &BTreeMap<SoundId, i64>,
    ) -> Self {
        let lead_in_ms = (timing.load_start + timing.load_end + timing.play_start).max(0) as i64;
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
        // 曲の長さ = 各オブジェクト（ノーツ・LN 終端・BGM）の時刻 + その音の長さ の最大
        let tm = session.timing();
        let ends = chart
            .notes
            .iter()
            .flat_map(|n| {
                let end = match n.kind {
                    NoteKind::Long { end, end_sound } => Some((end, end_sound)),
                    _ => None,
                };
                std::iter::once((n.beat, n.sound)).chain(end)
            })
            .chain(chart.bgm.iter().map(|(b, s)| (*b, Some(*s))));
        let mut last_beat: f64 = 0.0;
        let mut runtime_ms: i64 = 0;
        for (b, snd) in ends {
            last_beat = last_beat.max(b.0);
            let len = snd.and_then(|s| sound_ms.get(&s).copied()).unwrap_or(0);
            runtime_ms = runtime_ms.max(tm.time_of(b) / 1000 + len);
        }
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
        // 文字列（APP-SKN-10）: 10 題名、11 副題、12 題名 + 空白 + 副題、13 ジャンル、14 アーティスト、15 副アーティスト
        let m = &chart.meta;
        let full = if m.subtitle.is_empty() {
            m.title.clone()
        } else {
            format!("{} {}", m.title, m.subtitle)
        };
        let texts = [
            (10, m.title.clone()),
            (11, m.subtitle.clone()),
            (12, full),
            (13, m.genre.clone()),
            (14, m.artist.clone()),
            (15, m.subartist.clone()),
        ]
        .into_iter()
        .filter(|(_, s)| !s.is_empty())
        .collect();
        let hp = session.play().gauge();
        let mut s = Self {
            texts,
            lead_in_ms,
            timing,
            scratch: 0,
            hp_print: Follow::new(hp),
            score_print: Follow::new(0.0),
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
        // 画面の開始
        s.starts.insert(0, -lead_in_ms);
        s.update(-lead_in_ms, session, &[]);
        s
    }

    /// 画面の開始から譜面の先頭までの時間
    pub fn lead_in_ms(&self) -> i64 {
        self.lead_in_ms
    }

    /// 皿を回している向き（−1 = 上、0 = なし、1 = 下）。次の `update` から角度に効く
    pub fn set_scratch(&mut self, dir: i32) {
        self.scratch = dir.signum();
    }

    /// 場面を閉じてよいか（APP-SKN-08）: 曲の終わり（タイマー 2）から `#FADEOUT`、途中終了（タイマー 3）から `#CLOSE` を過ぎた
    pub fn scene_over(&self) -> bool {
        let over = |timer: i32, limit: i32| {
            self.timer(timer)
                .is_some_and(|t| limit == 0 || t > limit as i64)
        };
        over(3, self.timing.close) || over(2, self.timing.fade_out)
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
        // 場面の時間（APP-SKN-01）: 入力開始・準備完了・プレイ開始 = 譜面の先頭
        let scene = t_ms + self.lead_in_ms;
        if scene > self.timing.start_input as i64 {
            self.set_once(1);
        }
        let ready = (self.timing.load_start + self.timing.load_end) as i64 - self.lead_in_ms;
        if t_ms >= ready {
            self.starts.entry(40).or_insert(ready);
        }
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
        } else if t_ms > self.runtime_ms {
            // 曲の終わり（APP-SKN-08）
            self.set_once(2);
        }
        let total = play.total_notes();
        if total > 0 && play.combo() == total {
            self.set_once(48);
        }
        if play.is_finished() && !play.is_dead() {
            self.set_once(143);
        }
        // 皿の角度（APP-SKN-09）: 回している間はその向きに 720°/s、それ以外はプレイ開始後 360°/s
        let rate = if self.scratch != 0 {
            720.0 * self.scratch as f32
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
        // 表示の追従（APP-SKN-07）: HP は差 × 10ms、スコアは 500ms で追いかける
        let shown = self.hp_print.at(t_ms);
        self.hp_print
            .retarget(hp, t_ms, ((shown - hp) as i64).abs() * 10);
        if total > 0 {
            let score = (c.good as i64 + c.great as i64 * 2 + c.pgreat as i64 * 4) * 50000 / total;
            self.score_print.retarget(score as f64, t_ms, 500);
            let score = self.score_print.at(t_ms) as i64;
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
        n.insert(107, (self.hp_print.at(t_ms) as i64) / 2 * 2);
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
        let lines = self
            .measures
            .iter()
            .map(|m| m - self.beat)
            .filter(|b| (0.0..=visible_beats).contains(b))
            .collect();
        PlayView {
            lines,
            judge: self.judge,
            combo: self.combo,
            gauge: (self.hp_print.at(self.now) as i32) / 2,
            survival: self.survival,
            tremble,
            hi_speed,
            notes,
        }
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

    fn text(&self, id: i32) -> Option<&str> {
        self.texts.get(&id).map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
