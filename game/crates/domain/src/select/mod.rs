//! 選曲（LR2 の曲一覧）。フォルダと譜面の木から、開いたフォルダのバーの並びを作り、カーソルを動かす。
//! 譜面の走査は外側（application）で行い、ここには結果だけが入る。
use crate::chart::{ChartHash, KeyMode};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// 選曲に出す譜面の情報
#[derive(Debug, Clone, PartialEq)]
pub struct SongInfo {
    pub path: PathBuf,
    pub hash: ChartHash,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    /// `#PLAYLEVEL`（無ければ 0）
    pub level: i32,
    /// `#DIFFICULTY`（無ければ 0）
    pub difficulty: i32,
    pub key_mode: KeyMode,
}

/// 譜面の置き場所（登録したフォルダ・その中のすべてのフォルダ・譜面）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    roots: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    songs: Vec<SongInfo>,
}

/// 一覧の 1 行
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bar {
    Folder {
        path: PathBuf,
        title: String,
    },
    /// `Library::songs` の添字
    Song(usize),
}

/// LR2 の文字列比較（大文字にしてバイト順）
fn upper(p: &Path) -> String {
    p.to_string_lossy().to_ascii_uppercase()
}

fn folder_bar(path: &Path) -> Bar {
    Bar::Folder {
        path: path.to_path_buf(),
        title: path
            .file_name()
            .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
            .into_owned(),
    }
}

/// フォルダをパスの大文字比較順に並べる（LR2 の `CMP_SongDataByPath`）
fn folder_bars<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> Vec<Bar> {
    let mut v: Vec<&PathBuf> = paths.collect();
    v.sort_by_key(|p| upper(p));
    v.dedup();
    v.into_iter().map(|p| folder_bar(p)).collect()
}

impl Library {
    /// `folders` は登録したフォルダの中にあるすべてのフォルダ（登録したフォルダ自身は含めなくてよい）
    pub fn new(roots: Vec<PathBuf>, folders: Vec<PathBuf>, songs: Vec<SongInfo>) -> Self {
        Self {
            roots,
            folders,
            songs,
        }
    }
    pub fn songs(&self) -> &[SongInfo] {
        &self.songs
    }

    /// 最初の一覧 = 登録したフォルダ（DOM-SEL-01）
    pub fn root_bars(&self) -> Vec<Bar> {
        folder_bars(self.roots.iter())
    }

    /// フォルダを開いた一覧（DOM-SEL-02, 03）。開けなければ None
    pub fn open(&self, folder: &Path) -> Option<Vec<Bar>> {
        let song_dir = |s: &SongInfo| s.path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut songs: Vec<usize> = (0..self.songs.len())
            .filter(|&i| song_dir(&self.songs[i]).parent() == Some(folder))
            .collect();
        if !songs.is_empty() {
            // 同じ譜面（MD5）はパス順で最初の 1 つ
            songs.sort_by_key(|&i| upper(&self.songs[i].path));
            let mut seen = std::collections::HashSet::new();
            songs.retain(|&i| seen.insert(self.songs[i].hash));
            songs.sort_by(|&a, &b| self.cmp_by_directory(a, b));
            return Some(songs.into_iter().map(Bar::Song).collect());
        }
        let subs = folder_bars(self.folders.iter().filter(|p| p.parent() == Some(folder)));
        (!subs.is_empty()).then_some(subs)
    }

    /// LR2 の `CMP_SongDataByDirectory`: 譜面のフォルダ → 鍵盤数 → レベル（`#DIFFICULTY` 5 は +100）。同点はパス順
    fn cmp_by_directory(&self, a: usize, b: usize) -> Ordering {
        let key = |s: &SongInfo| {
            (
                upper(s.path.parent().unwrap_or(Path::new(""))),
                s.key_mode,
                s.level + if s.difficulty == 5 { 100 } else { 0 },
            )
        };
        let (sa, sb) = (&self.songs[a], &self.songs[b]);
        key(sa)
            .cmp(&key(sb))
            .then_with(|| upper(&sa.path).cmp(&upper(&sb.path)))
    }
}

/// 決定したときに起きること
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// フォルダを開いた
    Opened,
    /// 譜面を選んだ（`Library::songs` の添字）
    Song(usize),
    /// 何もしない（開けないフォルダ・空の一覧）
    Nothing,
}

/// 選曲の状態（DOM-SEL-04）
#[derive(Debug, Clone, PartialEq)]
pub struct SelectState {
    bars: Vec<Bar>,
    cursor: usize,
    /// 開く前の一覧とカーソル
    stack: Vec<(Vec<Bar>, usize)>,
}

impl SelectState {
    pub fn new(library: &Library) -> Self {
        Self {
            bars: library.root_bars(),
            cursor: 0,
            stack: vec![],
        }
    }
    pub fn bars(&self) -> &[Bar] {
        &self.bars
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn current(&self) -> Option<&Bar> {
        self.bars.get(self.cursor)
    }
    /// カーソルを置く（一覧の本数で回す。選曲スキンの一覧の動き [`BarScroll`] から決める）
    pub fn set_cursor(&mut self, cursor: usize) {
        let n = self.bars.len();
        self.cursor = if n == 0 { 0 } else { cursor % n };
    }
    /// 開いているフォルダの深さ（最初の一覧は 0）
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
    /// カーソルを動かす（端で反対側へ回る）
    pub fn scroll(&mut self, delta: i32) {
        let n = self.bars.len() as i64;
        if n > 0 {
            self.cursor = (self.cursor as i64 + delta as i64).rem_euclid(n) as usize;
        }
    }
    pub fn decide(&mut self, library: &Library) -> Decided {
        match self.current() {
            Some(Bar::Song(i)) => Decided::Song(*i),
            Some(Bar::Folder { path, .. }) => match library.open(path) {
                Some(bars) => {
                    let prev = std::mem::replace(&mut self.bars, bars);
                    self.stack.push((prev, self.cursor));
                    self.cursor = 0;
                    Decided::Opened
                }
                None => Decided::Nothing,
            },
            None => Decided::Nothing,
        }
    }
    /// 1 つ上の一覧へ戻る（最初の一覧なら false）
    pub fn back(&mut self) -> bool {
        match self.stack.pop() {
            Some((bars, cursor)) => {
                self.bars = bars;
                self.cursor = cursor;
                true
            }
            None => false,
        }
    }
}

/// スクロールのキーの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyState {
    #[default]
    Off,
    /// このフレームで押した
    Pressed,
    /// 押し続けている
    Held,
}

/// スクロールのくり返し（LR2 の `speedfirst` 300ms・`speednext` 70ms。DOM-SEL-05）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollRepeat {
    /// 次に動けるようになる時刻（ms）
    end_ms: i64,
    pub first_ms: i64,
    pub next_ms: i64,
}

impl Default for ScrollRepeat {
    fn default() -> Self {
        Self {
            end_ms: i64::MIN,
            first_ms: 300,
            next_ms: 70,
        }
    }
}

impl ScrollRepeat {
    /// このフレームで動かす量（−1 = 上、1 = 下、0 = 動かない）
    pub fn update(&mut self, now_ms: i64, up: KeyState, down: KeyState) -> i32 {
        let (delta, wait) = if up == KeyState::Pressed && now_ms > self.end_ms {
            (-1, self.first_ms)
        } else if down == KeyState::Pressed && now_ms > self.end_ms {
            (1, self.first_ms)
        } else if up == KeyState::Held && now_ms > self.end_ms.saturating_sub(20) {
            (-1, self.next_ms)
        } else if down == KeyState::Held && now_ms > self.end_ms.saturating_sub(20) {
            (1, self.next_ms)
        } else {
            return 0;
        };
        self.end_ms = now_ms + wait;
        delta
    }
}

/// LR2 の `ChangeValueByTime`（線形）: v1 = v2 なら v1、t1 ≤ now ≤ t2 かつ t1 < t2 なら直線、それ以外は now > t1 なら v2、でなければ v1
pub fn change_value_by_time(v1: f64, v2: f64, t1: f64, t2: f64, now: f64) -> f64 {
    if v1 == v2 {
        return v1;
    }
    if now <= t2 && t1 <= now && t1 < t2 {
        let r = (now - t1) / (t2 - t1);
        return (1.0 - r) * v1 + r * v2;
    }
    if t1 < now {
        v2
    } else {
        v1
    }
}

/// ホイールで動かしたときの待ち（ms）
pub const WHEEL_MS: i64 = 200;
/// マウスで範囲の外のバーを押したときの待ち（ms）
pub const MOUSE_MS: i64 = 200;

/// 選曲スキンの一覧の動き（LR2 の `nowBar`・`oldBar`・`barMoveStartTime`・`barMoveEndTime`・`listTopbar`・
/// `listCalculatedBar`・`listSelectedBarFromScreenTop`・`scrollDirection`。DOM-SEL-06..10）。位置は 1 本 = 1000。
/// 動きの終わる時刻はキーのくり返しの待ち（[`ScrollRepeat`]）と同じもの
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarScroll {
    /// 動き始めの位置
    old: i64,
    /// 目標の位置
    target: i64,
    start_ms: i64,
    /// 今の位置（回さない）
    top: i64,
    /// 今の位置を 0..本数 × 1000 に回したもの
    position: i64,
    /// 1 = 上へ、2 = 下へ（0 = まだ動かしていない）
    direction: i32,
    /// 選んでいるバーを置くスロット
    slot: i32,
    center: i32,
    available: (i32, i32),
    pub repeat: ScrollRepeat,
}

impl BarScroll {
    /// 画面に入ったとき: 選ぶスロットは `center`（`#BAR_CENTER`）、カーソルは `cursor`
    pub fn new(center: i32, available: (i32, i32), cursor: usize, n: usize, now_ms: i64) -> Self {
        let mut s = Self {
            old: 0,
            target: 0,
            start_ms: now_ms,
            top: 0,
            position: 0,
            direction: 0,
            slot: center,
            center,
            available,
            repeat: ScrollRepeat::default(),
        };
        s.jump(cursor, n, now_ms);
        s
    }

    /// DOM-SEL-09: カーソルを `cursor` に置いて止める（フォルダを開いた・戻った）
    pub fn jump(&mut self, cursor: usize, n: usize, now_ms: i64) {
        self.top = (self.center - self.slot) as i64 * 1000 + cursor as i64 * 1000;
        self.old = self.top;
        self.target = self.top;
        self.start_ms = now_ms;
        self.repeat.end_ms = now_ms;
        self.position = wrap(self.top, n);
    }

    fn start(&mut self, now_ms: i64, end_ms: i64) {
        self.old = self.top;
        self.start_ms = now_ms;
        self.repeat.end_ms = end_ms;
    }

    /// DOM-SEL-06: キー（DOM-SEL-05 の待ち）。動かした量（−1 / 0 / 1）を返す
    pub fn keys(&mut self, now_ms: i64, up: KeyState, down: KeyState) -> i32 {
        let d = self.repeat.update(now_ms, up, down);
        if d != 0 {
            let end = self.repeat.end_ms;
            self.target += d as i64 * 1000;
            self.start(now_ms, end);
            self.direction = if d < 0 { 1 } else { 2 };
        }
        d
    }

    /// DOM-SEL-06: ホイール（正 = 上へ回した量）。待ちは 200ms。キーより先に呼ぶ（LR2 と同じ順）
    pub fn wheel(&mut self, now_ms: i64, amount: i32) {
        if amount == 0 {
            return;
        }
        self.target -= amount as i64 * 1000;
        self.start(now_ms, now_ms + WHEEL_MS);
        self.direction = if amount > 0 { 1 } else { 2 };
    }

    /// DOM-SEL-10: マウスで押し続けているスロット `i`。`#BAR_AVAILABLE` の範囲なら選ぶスロットにし、
    /// 範囲より前・後ろなら止まっているときだけ (i − 端) × 2000 動かす
    pub fn mouse_hold(&mut self, now_ms: i64, i: i32) {
        let (from, to) = self.available;
        let edge = if i < from {
            from
        } else if i <= to {
            self.slot = i;
            return;
        } else {
            to
        };
        if self.target == self.top {
            self.target += (i - edge) as i64 * 2000;
            self.start(now_ms, now_ms + MOUSE_MS);
        }
    }

    /// DOM-SEL-07: 時刻 `now_ms` の位置にする。位置（回したもの）が変わったら true
    pub fn update(&mut self, now_ms: i64, n: usize) -> bool {
        let p = change_value_by_time(
            self.old as f64,
            self.target as f64,
            self.start_ms as f64,
            self.repeat.end_ms as f64,
            now_ms as f64,
        ) as i64;
        let before = self.position;
        self.top = p;
        self.position = wrap(p, n);
        before != self.position
    }

    /// DOM-SEL-08: カーソル（LR2 `GetSongCursor`）
    pub fn cursor(&self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        let n = n as i64;
        let step = (self.position % 1000 != 0 && self.direction == 2) as i64;
        ((self.slot - self.center) as i64 + (self.position / 1000 + step) + n * 30) as usize
            % n as usize
    }

    /// 一覧の位置（0..本数 × 1000）
    pub fn position(&self) -> i64 {
        self.position
    }
    /// 選んでいるバーを置いたスロット
    pub fn slot(&self) -> i32 {
        self.slot
    }
    /// 止まっているか（目標 = 今の位置）
    pub fn still(&self) -> bool {
        self.target == self.top
    }
}

/// 位置を 0..本数 × 1000 に回す（LR2 は足し引きをくり返す）
fn wrap(p: i64, n: usize) -> i64 {
    let m = n as i64 * 1000;
    if m == 0 {
        return 0;
    }
    p.rem_euclid(m)
}

/// LR2 のマウスの左ボタンの状態（`mouse_buttonL`。DOM-SEL-11）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseButton {
    /// 0 = 押していない、1 = 押した、2 = 押し続け、3 = 離した
    pub state: u8,
    press_ms: i64,
}

impl Default for MouseButton {
    fn default() -> Self {
        Self {
            state: 0,
            press_ms: i64::MIN / 2,
        }
    }
}

impl MouseButton {
    /// このフレームの状態にする（`down` は押しているか）
    pub fn update(&mut self, now_ms: i64, down: bool) -> u8 {
        self.state = if !down {
            if self.state == 0 || self.state == 3 {
                0
            } else if now_ms - self.press_ms > 32 {
                3
            } else {
                2
            }
        } else if self.state != 1 && self.state != 2 {
            if now_ms - self.press_ms >= 300 {
                self.press_ms = now_ms;
            }
            1
        } else {
            2
        };
        self.state
    }
}

#[cfg(test)]
mod tests;
