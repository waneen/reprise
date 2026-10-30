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
    Folder { path: PathBuf, title: String },
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

#[cfg(test)]
mod tests;
