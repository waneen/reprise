//! LR2 スキンでプレイ画面を描くための状態（APP-SKN-*）。`PlaySession` の進み具合から
//! LR2 の番号のタイマー・条件・数値を作り、`domain::skin::lr2::SkinState` として渡す。
//! 番号の意味は OpenLR2 の `LR2_skinobject.cpp`・`Scene04_Play.cpp`・`LR2_gameloop.cpp` を仕様として参照した。
use crate::play::PlaySession;
use crate::play_options::{ControlInput, OptionState, PlayOptions};
use crate::result_skin::PlayResult;
use crate::usecase::SavedScore;
use domain::chart::{Beat, BgaLayer, Chart, ImageId, KeyMode, NoteKind, SoundId, TimingEvent};
use domain::play::scroll::{position_at, position_of, speed_multiplier, UNITS_PER_BEAT};
use domain::play::{GaugeGraph, GaugeType, Judge, KeyState, PlayEvent};
use domain::skin::lr2::{pixels_per_beat, BgaView, NoteView, PlayView, SkinState};
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

/// 途中終了を見る入力（APP-SKN-14。LR2 の入力の状態 2 = 押し続け）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuitInput {
    /// Esc を押している
    pub escape: bool,
    /// マウスの右ボタンの状態（LR2 `En_input.cpp`: 0 なし・1 押した・2 押し続け・3 離した）
    pub mouse_right: u8,
    pub start_held: bool,
    pub select_held: bool,
}

/// BD 以下の判定から POOR 層を出す時間（LR2 の設定 `poorbga` の既定）
const POOR_BGA_MS: i64 = 500;

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
    /// 踏んだ地雷の数（LR2 の `judgecount[0]`。数値 114 は POOR の数にこれを足す）
    mines: u32,
    /// 途中終了した（APP-SKN-14。以後は判定しない）
    quit: bool,
    /// 文字列（APP-SKN-10）
    texts: BTreeMap<i32, String>,
    /// BGA の切り替え（時刻順。APP-SKN-11）と次に見るもの
    bga_events: Vec<(i64, BgaLayer, ImageId)>,
    bga_next: usize,
    /// 読めた BGA 画像と描画側の番号
    bga_images: BTreeMap<ImageId, u32>,
    /// 1 層目・2 層目・POOR 層
    bga: [Option<ImageId>; 3],
    /// プレイオプション（プレイ中に変わる）
    opts: OptionState,
    /// HS-FIX の倍率
    hs_mult: f64,
    /// リザルトのゲージグラフ（APP-SKN-13）と、終わりまで埋めたか
    graph: GaugeGraph,
    graph_done: bool,
}

impl PlaySkin {
    /// `skin_options` はスキンの読み込みで真になった条件（スキン設定の選択など）
    /// `timing` はスキンの場面の時間、`sound_ms` は音の長さ（曲の長さに使う）、
    /// `bga_images` は読めた BGA 画像と描画側の番号（[`BgaView::image`] になる）、`options` はプレイオプション
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chart: &Chart,
        session: &PlaySession,
        gauge: GaugeType,
        timing: SceneTiming,
        skin_options: &[i32],
        sound_ms: &BTreeMap<SoundId, i64>,
        bga_images: &BTreeMap<ImageId, u32>,
        options: PlayOptions,
    ) -> Self {
        let lead_in_ms = (timing.load_start + timing.load_end + timing.play_start).max(0) as i64;
        let survival = !matches!(gauge, GaugeType::Groove | GaugeType::Easy);
        let has_ln = chart
            .notes
            .iter()
            .any(|n| matches!(n.kind, NoteKind::Long { .. }));
        let mut fixed: BTreeSet<i32> = skin_options.iter().copied().collect();
        fixed.extend([
            41, // BGA を描く
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
        // BGA（APP-SKN-11）: POOR 層の初期値は 0 番の画像（読めていれば）
        let mut bga_events: Vec<(i64, BgaLayer, ImageId)> = chart
            .bga
            .iter()
            .map(|(b, l, i)| (tm.time_of(*b) / 1000, *l, *i))
            .collect();
        bga_events.sort_by_key(|e| e.0);
        let poor = bga_images.contains_key(&ImageId(0)).then_some(ImageId(0));
        let hp = session.play().gauge();
        let hs_mult = speed_multiplier(chart, tm, options.hs_fix);
        let mut s = Self {
            graph: GaugeGraph::new(hp),
            graph_done: false,
            opts: OptionState::new(options),
            hs_mult,
            texts,
            bga_events,
            bga_next: 0,
            bga_images: bga_images.clone(),
            bga: [None, None, poor],
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
            mines: 0,
            quit: false,
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

    /// プレイオプション（プレイ中の変更を含む）
    pub fn options(&self) -> &PlayOptions {
        &self.opts.options
    }

    /// プレイ中の操作（APP-OPT-01, 02）。`update` の後に呼ぶ
    pub fn control(&mut self, input: &ControlInput) {
        self.opts.control(self.now, input);
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

    /// APP-SKN-14: 途中終了（LR2 `ProcI_Play`）。閉じ始める前（タイマー 2・3 が止まっている）に、Esc かマウスの右の状態 2、
    /// START と SELECT の押し続け、全ノーツを片付けた後は START か SELECT の押し続けで、タイマー 2 を始めて
    /// ゲージグラフを終わりまで埋め（片付いていないノーツがあれば 0）、以後は判定しない。`#FADEOUT` の後に場面を閉じる
    pub fn quit(&mut self, input: &QuitInput, total_notes: u32) {
        if self.quit || self.starts.contains_key(&2) || self.starts.contains_key(&3) {
            return;
        }
        let all_done = self.processed >= total_notes as i64;
        let start_select = input.start_held && input.select_held;
        let either = input.start_held || input.select_held;
        if input.escape || input.mouse_right == 2 || start_select || (all_done && either) {
            self.quit = true;
            self.set(2);
            if !self.graph_done {
                let unfinished = self.processed < total_notes as i64;
                self.graph.finish(if unfinished { 0.0 } else { self.hp });
                self.graph_done = true;
            }
        }
    }

    /// 途中終了した（以後は `PlaySession::frame` を呼ばない）
    pub fn quitting(&self) -> bool {
        self.quit
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
    /// 鍵盤を押していなければ押す（タイマー 100+レーン を始めて 120+レーン を止める）
    fn press_once(&mut self, lane: i32) {
        if !self.starts.contains_key(&(100 + lane)) {
            self.set(100 + lane);
            self.reset(120 + lane);
        }
    }

    /// 1 フレーム進める。`events` はそのフレームの `PlaySession::frame` の結果
    pub fn update(&mut self, t_ms: i64, session: &PlaySession, events: &[PlayEvent]) {
        let dt = (t_ms - self.now).max(0);
        self.now = t_ms;
        // 途中終了した後は時刻だけ進める（APP-SKN-14）
        if self.quit {
            return;
        }
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
                    // オートは判定で鍵盤を押す（APP-SKN-12）
                    if self.opts.options.auto && judge != Judge::EmptyPoor {
                        self.press_once(lane.0 as i32);
                    }
                    self.set(46);
                }
                PlayEvent::Mine { .. } => {
                    self.mines += 1;
                    self.set(46);
                }
                _ => {}
            }
        }
        // BGA の切り替え（APP-SKN-11）
        while let Some(&(t, layer, image)) = self.bga_events.get(self.bga_next) {
            if t > t_ms {
                break;
            }
            let i = match layer {
                BgaLayer::Base => 0,
                BgaLayer::Layer => 1,
                BgaLayer::Poor => 2,
            };
            self.bga[i] = Some(image);
            self.bga_next += 1;
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
            let l = l as i32;
            if held.contains(&(l as u8)) {
                if self.opts.options.auto && !self.starts.contains_key(&(70 + l)) {
                    // オートは LN の間 鍵盤を押し続ける（APP-SKN-12）
                    self.reset(50 + l);
                    self.press_once(l);
                }
                self.set_once(70 + l);
            } else {
                self.reset(70 + l);
            }
            // オートは判定から 80ms で鍵盤を離す（APP-SKN-12）
            let after_judge = self.timer(50 + l).is_some_and(|t| t >= 80);
            if self.opts.options.auto && after_judge && self.timer(100 + l).is_some_and(|t| t > 0) {
                self.reset(100 + l);
                self.set(120 + l);
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
        // ゲージグラフ（APP-SKN-13）: 記録し、途中終了か曲の終わりで残りを埋める（片付いていないノーツがあれば 0）
        if !self.graph_done {
            self.graph.log(t_ms, self.runtime_ms, hp);
            if play.is_dead() || t_ms > self.runtime_ms {
                let unfinished = self.processed < play.total_notes() as i64;
                self.graph.finish(if unfinished { 0.0 } else { hp });
                self.graph_done = true;
            }
        }
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
        // LR2 の 114 は judgecount[0] + judgecount[1]（地雷は両方に数える）
        n.insert(114, c.poor as i64 + self.mines as i64);
        n.insert(160, session.timing().bpm_at(t_ms * 1000) as i64);
        // オプション（APP-OPT-04）: ハイスピード・判定タイミング・レーンカバー
        let o = self.opts.options;
        n.insert(10, o.hi_speed as i64);
        n.insert(12, o.judge_offset as i64);
        n.insert(14, o.cover as i64);
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

    /// APP-SKN-13: リザルト画面の材料（文字列 10..15・判定数・HP・ゲージグラフなど）。`saved` は保存した記録の前後
    pub fn result(&self, session: &PlaySession, saved: Option<SavedScore>) -> PlayResult {
        let play = session.play();
        PlayResult {
            texts: self.texts.clone(),
            key_mode: self.key_mode,
            groove: !self.survival,
            options: self.opts.options,
            counts: play.counts(),
            total: play.total_notes(),
            processed: self.processed as u32,
            combo: play.combo(),
            max_combo: play.max_combo(),
            mines: self.mines,
            // LR2 は片付いていないノーツが残ったまま終わると HP を 0 にする
            hp: if self.processed < play.total_notes() as i64 {
                0.0
            } else {
                play.gauge()
            },
            lamp: play.clear_lamp(),
            graph: self.graph.points().to_vec(),
            saved,
        }
    }

    /// 曲の進み具合（0..=1）。プレイ開始前は 0
    fn progress(&self) -> f64 {
        if self.now < 0 || self.runtime_ms <= 0 {
            return 0.0;
        }
        (self.now as f64 / self.runtime_ms as f64).min(1.0)
    }

    /// 描画材料（APP-SKN-06, APP-OPT-03）。`visible_px` ピクセル先まで、`tremble` はゲージのちらつき用の乱数（0..=2）。
    /// ハイスピードは 設定 × HS-FIX の倍率、ノーツと小節線は (位置 − 今の位置) / 480（今 = 時刻 + 判定タイミング）
    pub fn view(&self, session: &PlaySession, visible_px: f64, tremble: i32) -> PlayView {
        let o = self.opts.options;
        let hi_speed = o.hi_speed as f64 * self.hs_mult;
        let visible = visible_px / pixels_per_beat(hi_speed);
        let tm = session.timing();
        let now = position_at(tm, o.hs_fix, (self.now + o.judge_offset as i64) * 1000);
        let dist = |b: Beat| (position_of(tm, o.hs_fix, b) - now) / UNITS_PER_BEAT;
        let notes = session
            .play()
            .pending_notes()
            .into_iter()
            .filter(|n| dist(n.beat) <= visible)
            .map(|n| {
                let head = dist(n.beat);
                NoteView {
                    lane: n.lane.0 as i32,
                    head: if n.held { head.max(0.0) } else { head },
                    tail: n.end_beat.map(dist),
                    mine: n.mine,
                    held: n.held,
                }
            })
            .collect();
        let lines = self
            .measures
            .iter()
            .map(|m| dist(Beat(*m)))
            .filter(|b| (0.0..=visible).contains(b))
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
            bga: self.bga_view(),
        }
    }

    /// 描く BGA の層（APP-SKN-11）。BD 以下の判定から 500ms 未満は POOR 層だけ（後ろの黒なし）、
    /// それ以外は 1 層目（後ろに黒）と 2 層目。読めていない画像は描かない
    fn bga_view(&self) -> Vec<BgaView> {
        let [base, layer, poor] = self.bga;
        let poor_time = self.last_miss.is_some_and(|m| self.now - m < POOR_BGA_MS);
        let layers = match (poor_time, poor) {
            (true, Some(p)) => vec![(p, false)],
            _ => vec![(base, true), (layer, false)]
                .into_iter()
                .filter_map(|(i, b)| Some((i?, b)))
                .collect(),
        };
        layers
            .into_iter()
            .filter_map(|(i, backbox)| {
                Some(BgaView {
                    image: *self.bga_images.get(&i)?,
                    backbox,
                })
            })
            .collect()
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
        let auto = self.opts.options.auto;
        match id {
            // オプション（APP-OPT-04）: 32 オートでない、33 オート、270 START/SELECT を押しながらでレーンカバーが効いている
            32 => !auto,
            33 => auto,
            270 => self.opts.holding && self.opts.cover_active(),
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
        let o = &self.opts;
        match kind {
            // ハイスピード（10..=990 の範囲）・レーンカバー（APP-OPT-04）
            2 => (o.options.hi_speed - 10) as f64 / 980.0,
            4 if o.cover_active() => o.options.cover as f64 / 100.0,
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
