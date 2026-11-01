//! LR2 の選曲スキンで描くための状態（APP-SEL-*）。選んでいるバーと一覧の動きから
//! LR2 の番号のタイマー・条件・数値・文字列を作り、`domain::skin::lr2::SkinState` として渡す。
//! 番号の意味は OpenLR2 の `LR2_skinobject.cpp`（`GetOptionFlag_dst`・`SetObjectValue_Num`・`SetObjectValue_Bargraph`・
//! `SetObjectValue_Slider`・`SetObjectValue_Button`）・`Scene02_Songselect.cpp`（`ProcS_Select`）・`Main.cpp` を仕様として参照した。
//! 今あるデータで作れないもの（IR・ライバル・コース・バナー・プレビュー音・BPM・難易度ごとのレベル・追加日時など）は作らない。
use crate::play_options::PlayOptions;
use domain::chart::{ChartHash, KeyMode};
use domain::score::{ClearLamp, ScoreRecord};
use domain::select::{Bar, Library};
use domain::skin::lr2::{BarView, SkinState};
use domain::skin::SceneTiming;
use std::collections::{BTreeMap, BTreeSet};

/// 曲のバーの種類（LR2 の `folderType`。song.db の `type` 列）
pub const BAR_SONG: i32 = 0;
/// フォルダのバーの種類
pub const BAR_FOLDER: i32 = 1;

/// 選曲の一覧の 1 本（LR2 の `SONGDATA` のうち作れるもの）
#[derive(Debug, Clone, PartialEq)]
pub struct SelectBar {
    /// LR2 の `folderType`
    pub kind: i32,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    /// 鍵盤数（フォルダは None）
    pub key_mode: Option<KeyMode>,
    pub level: i32,
    pub difficulty: i32,
    /// 自分の記録（LR2 のルール。無ければ None）
    pub record: Option<ScoreRecord>,
}

/// LR2 の鍵盤数の番号
fn key_number(k: KeyMode) -> i32 {
    match k {
        KeyMode::K5 => 5,
        KeyMode::K7 => 7,
        KeyMode::K9 => 9,
        KeyMode::K10 => 10,
        KeyMode::K14 => 14,
    }
}

/// LR2 のランプの番号（0 = NO PLAY、1 = FAILED、2 = EASY、3 = NORMAL、4 = HARD、5 = FULL COMBO）。
/// LR2 のルールの記録に出てこないランプ（beatoraja のもの）は 0
pub fn lr2_clear(lamp: ClearLamp) -> i32 {
    match lamp {
        ClearLamp::Failed => 1,
        ClearLamp::Easy => 2,
        ClearLamp::Normal => 3,
        ClearLamp::Hard => 4,
        ClearLamp::FullCombo => 5,
        _ => 0,
    }
}

impl SelectBar {
    /// APP-SEL-01: 一覧の 1 本。曲は種類 0、フォルダは種類 1（OpenLR2 が song.db に書く `type`）。`record` は曲の記録を引く
    pub fn new(
        library: &Library,
        bar: &Bar,
        record: &dyn Fn(&ChartHash) -> Option<ScoreRecord>,
    ) -> Self {
        match bar {
            Bar::Folder { title, .. } => Self {
                kind: BAR_FOLDER,
                title: title.clone(),
                subtitle: String::new(),
                artist: String::new(),
                genre: String::new(),
                key_mode: None,
                level: 0,
                difficulty: 0,
                record: None,
            },
            Bar::Song(i) => {
                let s = &library.songs()[*i];
                Self {
                    kind: BAR_SONG,
                    title: s.title.clone(),
                    subtitle: s.subtitle.clone(),
                    artist: s.artist.clone(),
                    genre: s.genre.clone(),
                    key_mode: Some(s.key_mode),
                    level: s.level,
                    difficulty: s.difficulty,
                    record: record(&s.hash),
                }
            }
        }
    }

    /// 題名 + 空白 + 副題（副題が無ければ題名。LR2 の `fulltitle`）
    pub fn full_title(&self) -> String {
        if self.subtitle.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.title, self.subtitle)
        }
    }

    /// 曲バーの描画材料（ランプは記録が無ければ 0）
    pub fn view(&self) -> BarView {
        BarView {
            kind: self.kind,
            title: self.full_title(),
            key_mode: self.key_mode.map_or(0, key_number),
            level: self.level,
            difficulty: self.difficulty,
            lamp: self.record.as_ref().map_or(0, |r| lr2_clear(r.lamp)),
        }
    }
}

/// 選曲画面のスキン用の状態。時刻は画面の開始からの ms
pub struct SelectSkin {
    timing: SceneTiming,
    skin_options: BTreeSet<i32>,
    options: PlayOptions,
    now: i64,
    starts: BTreeMap<i32, i64>,
    /// 選んでいるバーで決まる条件・数値・文字列
    ops: BTreeSet<i32>,
    numbers: BTreeMap<i32, i64>,
    texts: BTreeMap<i32, String>,
    /// BARGRAPH 3（レベル）
    level_bar: f64,
    /// SLIDER 1（一覧の位置）
    list_slider: f64,
}

impl SelectSkin {
    /// `skin_options` はスキンの読み込みで真になった条件、`options` はプレイオプション（数値 10/12/14）
    pub fn new(timing: SceneTiming, skin_options: &[i32], options: PlayOptions) -> Self {
        let mut s = Self {
            timing,
            skin_options: skin_options.iter().copied().collect(),
            options,
            now: 0,
            starts: BTreeMap::new(),
            ops: BTreeSet::new(),
            numbers: BTreeMap::new(),
            texts: BTreeMap::new(),
            level_bar: 0.0,
            list_slider: 0.0,
        };
        // APP-SEL-02: 画面の開始でタイマー 0
        s.starts.insert(0, 0);
        s
    }

    /// 入力を受け付けるか（`#STARTINPUT` を過ぎた）
    pub fn input_ready(&self) -> bool {
        self.now > self.timing.start_input as i64
    }

    /// APP-SEL-02: 1 フレーム進める。`moved` は一覧の位置が変わったか、`changed` は選んでいるバーかスロットが変わったか
    pub fn update(&mut self, now_ms: i64, moved: bool, changed: bool) {
        self.now = now_ms;
        if now_ms > self.timing.start_input as i64 && !self.starts.contains_key(&1) {
            self.starts.insert(1, now_ms);
            self.starts.insert(11, now_ms);
        }
        if changed && self.timer(1).is_some_and(|t| t > 0) {
            self.starts.insert(11, now_ms);
        }
        if !moved {
            self.starts.remove(&10);
        } else if self.timer(10) != Some(0) {
            // LR2 は経過が 0 でなければ（止まっていても）始め直す
            self.starts.insert(10, now_ms);
        }
    }

    /// 選んでいるバーを替えた（フォルダを開いた・戻った）ときのタイマー 11
    pub fn reset_bar_timer(&mut self) {
        self.starts.insert(11, self.now);
    }

    /// APP-SEL-05: 一覧の位置（SLIDER 1 = 位置 / (本数 × 1000 − 1)）
    pub fn set_list(&mut self, position: i64, n: usize) {
        let max = n as i64 * 1000 - 1;
        self.list_slider = if max > 0 {
            position as f64 / max as f64
        } else {
            0.0
        };
    }

    /// APP-SEL-03, 04: 選んでいるバー（LR2 `ProcS_Select`）。条件・数値・文字列を作り直す
    pub fn select(&mut self, bar: Option<&SelectBar>) {
        self.ops.clear();
        self.numbers.clear();
        self.texts.clear();
        self.level_bar = 0.0;
        let Some(bar) = bar else {
            return;
        };
        let k = bar.kind;
        // バーの種類（GetOptionFlag_dst 1..5）
        for (op, kinds) in [
            (1, &[1, 2, 3, 4, 6][..]),
            (2, &[0, 5][..]),
            (3, &[8][..]),
            (4, &[7, 9][..]),
            (5, &[0, 5, 8, 9][..]),
        ] {
            if kinds.contains(&k) {
                self.ops.insert(op);
            }
        }
        // 難易度（難易度の絞り込みが使える LR2 の既定の設定での 150..155）
        if (0..=5).contains(&bar.difficulty) {
            self.ops.insert(150 + bar.difficulty);
        }
        let none = ChartHash {
            md5: [0; 16],
            sha256: None,
        };
        let empty = ScoreRecord::empty(none, domain::play::RulesetId::Lr2);
        let rec = bar.record.as_ref().unwrap_or(&empty);
        if let Some(km) = bar.key_mode {
            let key = key_number(km);
            // ランプ 100..105（曲だけ）
            self.ops.insert(100 + lr2_clear(rec.lamp));
            // 鍵盤数 160..164
            self.ops.insert(match km {
                KeyMode::K7 => 160,
                KeyMode::K5 => 161,
                KeyMode::K14 => 162,
                KeyMode::K10 => 163,
                KeyMode::K9 => 164,
            });
            // レベルの高さ 185/186（BARGRAPH 3 の最大を超えるか）
            let max = match key {
                5 | 10 => 9,
                7 | 14 => 12,
                _ => 42,
            };
            self.ops.insert(if bar.level <= max { 185 } else { 186 });
            self.level_bar = (bar.level.max(0) as f64 / max as f64).min(1.0);
            self.numbers.insert(42, bar.level as i64);
            self.numbers.insert(96, bar.level as i64);
            for base in [17, 27] {
                self.texts.insert(base, bar.level.to_string());
                self.texts.insert(base + 1, bar.difficulty.to_string());
            }
        }
        // 自分の記録 71, 75..84（記録が無い・フォルダは 0）
        let n = &mut self.numbers;
        n.insert(71, rec.ex_score as i64);
        n.insert(75, rec.max_combo as i64);
        n.insert(76, rec.min_bp.unwrap_or(0) as i64);
        n.insert(77, rec.play_count as i64);
        n.insert(78, rec.clear_count as i64);
        n.insert(79, rec.play_count as i64 - rec.clear_count as i64);
        n.insert(80, rec.pgreat as i64);
        n.insert(81, rec.great as i64);
        n.insert(82, rec.good as i64);
        n.insert(83, rec.bad as i64);
        n.insert(84, rec.poor as i64);
        // 文字列 10..14・20..24（空は無し）
        let full = bar.full_title();
        for base in [10, 20] {
            for (i, s) in [&bar.title, &bar.subtitle, &full, &bar.genre, &bar.artist]
                .into_iter()
                .enumerate()
            {
                if !s.is_empty() {
                    self.texts.insert(base + i as i32, s.clone());
                }
            }
        }
    }
}

impl SkinState for SelectSkin {
    fn timer(&self, id: i32) -> Option<i64> {
        self.starts.get(&id).map(|s| self.now - s)
    }

    /// APP-SEL-03
    fn option(&self, id: i32) -> bool {
        if self.skin_options.contains(&id) || self.ops.contains(&id) {
            return true;
        }
        // 20 パネルを開いていない（パネルは無い）、50 IR につないでいない、52 EXTRA モードでない
        matches!(id, 20 | 50 | 52)
    }

    /// APP-SEL-04
    fn number(&self, id: i32) -> Option<i64> {
        let o = &self.options;
        match id {
            10 => Some(o.hi_speed as i64),
            12 => Some(o.judge_offset as i64),
            14 => Some(o.cover as i64),
            _ => self.numbers.get(&id).copied(),
        }
    }

    fn bar(&self, kind: i32) -> f64 {
        match kind {
            3 => self.level_bar,
            _ => 0.0,
        }
    }

    fn slider(&self, kind: i32) -> f64 {
        match kind {
            1 => self.list_slider,
            _ => 0.0,
        }
    }

    fn turntable(&self, _side: i32) -> f32 {
        0.0
    }

    fn text(&self, id: i32) -> Option<&str> {
        self.texts.get(&id).map(String::as_str)
    }

    /// APP-SEL-06: パネル 1..8 は 0（パネルを開いていない）、絞り込み・並べ替え 10..12 は 0（絞り込まない・ディレクトリ順）、
    /// 13..19 は 0（LR2 も 0 で描く）
    fn button(&self, action: i32) -> Option<i32> {
        match action {
            1..=8 | 10..=19 => Some(0),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
