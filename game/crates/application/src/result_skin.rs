//! LR2 のリザルト画面（APP-RES-*）。リザルトスキン（`#INFORMATION` 種別 7）で描くための状態と、1 フレームの処理。
//! OpenLR2 の `Scene05_Result.cpp`（`ProcI_Result`・`Proc_Result`。LR2 の書き起こしである `LR2Beta3-v100201` の枝とも同じ）・
//! `Main.cpp`（場面の開始・`#STARTINPUT`・プレイを閉じたときの行き先）・`LR2_skinobject.cpp`（条件・数値・BARGRAPH）・
//! `LR2_gameloop.cpp`（`ReactInput` のタイマー 100..127）を仕様として参照し、lunaticvibesf の `scene_result.cpp` でも確かめた。
use crate::key_timers::KeyTimers;
use crate::play_options::PlayOptions;
use crate::usecase::SavedScore;
use domain::chart::KeyMode;
use domain::input::{key_slot, SLOTS};
use domain::play::JudgeCounts;
use domain::score::{ClearLamp, ScoreRecord};
use domain::skin::lr2::SkinState;
use domain::skin::SceneTiming;
use std::collections::{BTreeMap, BTreeSet};

/// 遊び終えたプレイの結果（リザルト画面の材料）
#[derive(Debug, Clone, PartialEq)]
pub struct PlayResult {
    /// 文字列 10..15（プレイ画面と同じ。APP-SKN-10）
    pub texts: BTreeMap<i32, String>,
    pub key_mode: KeyMode,
    /// GROOVE・EASY ゲージ（それ以外は HARD 系）
    pub groove: bool,
    /// プレイオプション（プレイ中の変更を含む）
    pub options: PlayOptions,
    pub counts: JudgeCounts,
    /// 総ノーツ数
    pub total: u32,
    /// 片付いたノーツの数（LR2 の `note_current`）
    pub processed: u32,
    pub combo: u32,
    pub max_combo: u32,
    /// 終わりの HP
    pub hp: f64,
    pub lamp: ClearLamp,
    /// ゲージグラフの区間ごとの HP（`domain::play::GaugeGraph`）
    pub graph: Vec<i32>,
    /// 保存した記録の前後（保存しなかったら None）
    pub saved: Option<SavedScore>,
}

impl PlayResult {
    /// EX スコア
    fn ex(&self) -> i64 {
        self.counts.ex_score() as i64
    }
    /// このプレイの BP（記録の最小 BP と同じ数え方: BD + POOR + 片付いていないノーツ）
    fn bp(&self) -> i64 {
        self.counts.bad as i64 + self.counts.poor as i64 + self.total as i64 - self.processed as i64
    }
    /// LR2 のスコア（PG 4・GR 2・GD 1 を 50000 / 総ノーツ数 倍）
    fn score(&self) -> Option<i64> {
        let c = self.counts;
        (self.total > 0).then(|| {
            (c.good as i64 + c.great as i64 * 2 + c.pgreat as i64 * 4) * 50000 / self.total as i64
        })
    }
    /// 保存前の記録（LR2 の `sSelect.old`）
    fn old(&self) -> Option<&ScoreRecord> {
        self.saved.as_ref().and_then(|s| s.before.as_ref())
    }
}

/// プレイの場面を閉じた後の行き先
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterPlay {
    Result,
    Select,
}

/// APP-RES-09: プレイの場面を閉じたら（LR2 `Main.cpp` の場面 4 の閉じ方）、オートプレイ・片付けたノーツが 0 は選曲、
/// PG・GR・GD のどれかがあればリザルト、それ以外は選曲
pub fn after_play(auto: bool, processed: u32, counts: &JudgeCounts) -> AfterPlay {
    if auto || processed == 0 {
        return AfterPlay::Select;
    }
    if counts.pgreat + counts.great + counts.good != 0 {
        AfterPlay::Result
    } else {
        AfterPlay::Select
    }
}

/// 1 フレームの入力
#[derive(Debug, Clone, Copy, Default)]
pub struct ResultInput<'a> {
    /// 1P の操作の押下（true）・離し（false）。起きた順
    pub slots: &'a [(usize, bool)],
    /// 1P の操作を押しているか
    pub held: [bool; SLOTS],
    /// Esc・Enter を押した
    pub escape: bool,
    pub enter: bool,
    /// LR2 のマウスの左・右ボタンの状態（0 なし・1 押した・2 押し続け・3 離した）
    pub mouse_left: u8,
    pub mouse_right: u8,
}

/// 画面を閉じた後の行き先
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultExit {
    Select,
    /// 同じ譜面をもう一度（LR2 の `flag_retry`）
    Retry,
}

/// リザルト画面の状態。時刻は画面の開始からの ms
pub struct ResultSkin {
    timing: SceneTiming,
    skin_options: BTreeSet<i32>,
    /// GAUGECHART_1P\[0\] の伸び始め・伸び終わり（SRC 引数 13・14）
    chart: (i64, i64),
    /// 入力で早めた伸び終わり
    chart_end: Option<i64>,
    now: i64,
    starts: BTreeMap<i32, i64>,
    keys: KeyTimers,
    /// 閉じ始めた（タイマー 2。LR2 の `procPhase == 2`）
    closing: bool,
    result: PlayResult,
}

/// ランク（EX スコア × 9 / (総ノーツ数 × 2)。総ノーツ数が 0 なら None）
fn rank(ex: i64, total: i64) -> Option<i64> {
    (total > 0).then(|| ex * 9 / (total * 2))
}

impl ResultSkin {
    /// APP-RES-01: 画面の開始（タイマー 0）。`chart_times` は GAUGECHART_1P\[0\] の SRC 引数 13・14
    pub fn new(
        timing: SceneTiming,
        skin_options: &[i32],
        chart_times: (i64, i64),
        result: PlayResult,
    ) -> Self {
        let mut s = Self {
            timing,
            skin_options: skin_options.iter().copied().collect(),
            chart: chart_times,
            chart_end: None,
            now: 0,
            starts: BTreeMap::new(),
            keys: KeyTimers::default(),
            closing: false,
            result,
        };
        s.starts.insert(0, 0);
        s
    }

    pub fn result(&self) -> &PlayResult {
        &self.result
    }

    /// 入力で早めたゲージグラフの伸び終わり（DOM-SKD-22）
    pub fn chart_end(&self) -> Option<i64> {
        self.chart_end
    }

    fn set(&mut self, id: i32) {
        self.starts.insert(id, self.now);
    }
    fn has(&self, id: i32) -> bool {
        self.starts.contains_key(&id)
    }

    /// APP-RES-01..04: 1 フレーム進める（LR2 の順: 入力開始 → 入力 → タイマー 150・151）。閉じたら行き先を返す
    pub fn frame(&mut self, now_ms: i64, input: ResultInput<'_>) -> Option<ResultExit> {
        self.now = now_ms;
        let mut out = None;
        // APP-RES-01: `#STARTINPUT` を過ぎたらタイマー 1 と 11
        if now_ms > self.timing.start_input as i64 && !self.has(1) {
            self.set(1);
            self.set(11);
        }
        // APP-RES-04: タイマー 1 の後は押した・離した鍵盤のタイマー
        if self.has(1) {
            for &(slot, down) in input.slots {
                self.keys.apply(&mut self.starts, now_ms, slot, down);
            }
        }
        // APP-RES-02: 鍵盤 1〜7 の押下・Esc・Enter・マウスの左右の押し続け
        let keys = (1..=7).map(key_slot).collect::<Vec<_>>();
        let decide = input
            .slots
            .iter()
            .any(|&(s, down)| down && keys.contains(&s))
            || input.escape
            || input.enter
            || input.mouse_left == 2
            || input.mouse_right == 2;
        if decide && !self.closing && now_ms > self.timing.start_input as i64 {
            self.advance();
        } else if self.closing {
            // APP-RES-03: タイマー 2 から `#FADEOUT` を過ぎたら（0 なら次のフレームで）閉じる
            let t = self.timer(2).unwrap_or(0);
            if self.timing.fade_out == 0 || t > self.timing.fade_out as i64 {
                let held = |n: &[usize]| n.iter().any(|&k| input.held[key_slot(k)]);
                out = Some(if held(&[1, 3, 5, 7]) && held(&[2, 4, 6]) {
                    ResultExit::Retry
                } else {
                    ResultExit::Select
                });
            }
        }
        // APP-RES-01: ゲージグラフの伸び始めでタイマー 150、伸び終わりで 151
        let end = self.chart_end.unwrap_or(self.chart.1);
        if now_ms > self.chart.0 && !self.has(150) {
            self.set(150);
        }
        if now_ms > end && !self.has(151) {
            self.set(151);
        }
        out
    }

    /// APP-RES-02: 入力で 1 段進める（LR2 `ProcI_Result`）
    fn advance(&mut self) {
        let Some(t151) = self.timer(151) else {
            // グラフを伸び終わらせてランクを出す
            self.set(151);
            self.chart_end = Some(self.now);
            return;
        };
        if t151 <= self.timing.start_input_rank as i64 || self.has(152) {
            if self
                .timer(152)
                .is_some_and(|t| t > self.timing.start_input_update as i64)
                && !self.has(2)
            {
                self.close();
            }
        } else if self.result.saved.is_some() {
            // 記録更新の表示
            self.set(152);
        } else {
            self.close();
        }
    }

    fn close(&mut self) {
        self.set(2);
        self.closing = true;
    }
}

impl SkinState for ResultSkin {
    fn timer(&self, id: i32) -> Option<i64> {
        self.starts.get(&id).map(|s| self.now - s)
    }

    /// APP-RES-05: 条件
    fn option(&self, id: i32) -> bool {
        if self.skin_options.contains(&id) {
            return true;
        }
        let r = &self.result;
        let total = r.total as i64;
        let now = rank(r.ex(), total);
        let old = r.old();
        // 保存前の記録が無ければ LR2 の空の記録（総ノーツ数 0）と同じ
        let old_rank = old.and_then(|o| rank(o.ex_score as i64, total));
        let is = |rank: Option<i64>, f: &dyn Fn(i64) -> bool| rank.is_some_and(f);
        match id {
            // オートプレイでない（オートはリザルトに来ない）
            32 => true,
            42 => r.groove,
            43 => !r.groove,
            // クリア・失敗
            90 => r.lamp > ClearLamp::Failed,
            91 => r.lamp <= ClearLamp::Failed,
            160 => r.key_mode == KeyMode::K7,
            161 => r.key_mode == KeyMode::K5,
            162 => r.key_mode == KeyMode::K14,
            163 => r.key_mode == KeyMode::K10,
            164 => r.key_mode == KeyMode::K9,
            // ランク AAA..F（F は EX スコア 0）
            300 => is(now, &|k| k >= 8),
            301..=306 => is(now, &|k| k == (308 - id) as i64),
            307 => r.ex() != 0 && is(now, &|k| k <= 1),
            308 => r.ex() == 0 || total == 0,
            // 保存前のランク
            320 => is(old_rank, &|k| k >= 8),
            321..=326 => is(old_rank, &|k| k == (328 - id) as i64),
            327 => is(old_rank, &|k| k <= 1),
            // 更新した: EX スコア・最大コンボ・最小 BP・ランク
            330 => r.ex() > old.map_or(0, |o| o.ex_score as i64),
            331 => r.max_combo > old.map_or(0, |o| o.max_combo),
            332 => old
                .and_then(|o| o.min_bp)
                .is_some_and(|b| r.bp() < b as i64),
            335 => matches!((old_rank, now), (Some(a), Some(b)) if a < b),
            // 更新後のランク（LR2 はこのプレイの EX スコアで決める）
            340 => is(now, &|k| k >= 8),
            341..=346 => is(now, &|k| k == (348 - id) as i64),
            347 => is(now, &|k| k <= 1),
            // 1P 側で描く（`#FLIP_RESULT` は無い）
            350 => true,
            _ => false,
        }
    }

    /// APP-RES-06: 数値
    fn number(&self, id: i32) -> Option<i64> {
        let r = &self.result;
        let c = r.counts;
        let total = r.total as i64;
        let ex = r.ex();
        let old = r.old();
        let old_ex = old.map_or(0, |o| o.ex_score as i64);
        let old_bp = old.and_then(|o| o.min_bp).map_or(0, |b| b as i64);
        let o = r.options;
        Some(match id {
            10 => o.hi_speed as i64,
            12 => o.judge_offset as i64,
            14 => o.cover as i64,
            100 => {
                let s = r.score()?;
                if matches!(r.key_mode, KeyMode::K7 | KeyMode::K14) {
                    s
                } else {
                    s / 20 * 10
                }
            }
            101 | 171 => ex,
            102 | 103 if r.processed == 0 => return None,
            102 => (ex as f64 * 100.0 / (r.processed as f64 * 2.0)) as i64,
            103 => ((ex as f64 * 100.0 / (r.processed as f64 * 2.0)) * 100.0) as i64 % 100,
            104 => r.combo as i64,
            105 | 174 => r.max_combo as i64,
            106 => total,
            107 => (r.hp as i64) / 2 * 2,
            110 => c.pgreat as i64,
            111 => c.great as i64,
            112 => c.good as i64,
            113 => c.bad as i64,
            114 => c.poor as i64,
            115 | 116 if total == 0 => return None,
            115 => ex * 100 / (total * 2),
            116 => ex * 10000 / (total * 2) % 100,
            170 => old_ex,
            172 => ex - old_ex,
            173 => old.map_or(0, |o| o.max_combo as i64),
            175 => r.max_combo as i64 - old.map_or(0, |o| o.max_combo as i64),
            176 => old_bp,
            177 => r.bp(),
            178 => r.bp() - old_bp,
            183 | 184 if total == 0 => 0,
            183 => old_ex * 100 / (total * 2),
            184 => old_ex * 10000 / (total * 2) % 100,
            _ => return None,
        })
    }

    /// APP-RES-07: BARGRAPH
    fn bar(&self, kind: i32) -> f64 {
        let r = &self.result;
        let c = r.counts;
        let total = r.total as f64;
        let ex = r.ex() as f64;
        let (val, max) = match kind {
            10 | 27 => (ex, total * 2.0),
            11 if r.processed > 0 => (
                (r.ex() * r.total as i64 / r.processed as i64) as f64,
                total * 2.0,
            ),
            20 => (c.pgreat as f64, total),
            21 => (c.great as f64, total),
            22 => (c.good as f64, total),
            23 => (c.bad as f64, total),
            24 => (c.poor as f64, total),
            25 => (r.max_combo as f64, total),
            // LR2 は 7/14 鍵 20000・それ以外 10000 を最大にする（スコアの最大は 200000）
            26 => (
                r.score().unwrap_or(0) as f64,
                if matches!(r.key_mode, KeyMode::K7 | KeyMode::K14) {
                    20000.0
                } else {
                    10000.0
                },
            ),
            _ => return 0.0,
        };
        if val <= 0.0 || max <= 0.0 {
            return 0.0;
        }
        val.min(max) / max
    }

    fn slider(&self, _kind: i32) -> f64 {
        0.0
    }

    fn turntable(&self, _side: i32) -> f32 {
        0.0
    }

    /// APP-RES-08: 文字列 10..15（題名など。プレイ画面と同じ）
    fn text(&self, id: i32) -> Option<&str> {
        self.result.texts.get(&id).map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
