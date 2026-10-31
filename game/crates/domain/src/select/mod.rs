//! 選曲（LR2 の曲一覧）。フォルダと譜面の木から、開いたフォルダのバーの並びを作り、カーソルを動かす。
//! 譜面の走査は外側（application）で行い、ここには結果だけが入る。
pub mod filter;
pub mod preview;
pub mod table;

use crate::chart::{ChartHash, KeyMode};
pub use filter::{FilterKind, Group, ListFilter, SortRecord};
pub use preview::{Panel, PanelChange, Preview, PreviewAction};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
pub use table::{DifficultyTable, TableChart, TableLevel};

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
    /// `#DIFFICULTY`（無ければ −1。[`Library::new`] で LR2 と同じく付ける）
    pub difficulty: i32,
    pub key_mode: KeyMode,
    /// ノーツ数（`#DIFFICULTY` の無い譜面の難易度の付け方・スコア順に使う）
    pub notes: u32,
}

/// カスタムフォルダ（LR2 の `.lr2folder`。song.db の `folder` の `type` 2 の行）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomFolder {
    /// `.lr2folder` のパス
    pub path: PathBuf,
    /// `#TITLE`
    pub title: String,
    /// `#GENRE`・`#CATEGORY`（LR2 の `category`）
    pub category: String,
    /// `#ARTIST`・`#INFORMATION_A`（LR2 の `info_a`）
    pub info_a: String,
    /// `#SUBARTIST`・`#INFORMATION_B`（LR2 の `info_b`）
    pub info_b: String,
    /// `#TAG`・`#COMMAND`（SQL の `WHERE` の後ろ。LR2 の `command`）
    pub command: String,
    /// `#PLAYLEVEL`・`#MAXTRACKS`（出す譜面の数の上限。1 なら無作為に選ぶ。LR2 の `max`）
    pub max: i32,
    /// LR2 の `folder` の `type`（2 カスタムフォルダ・3 新着（登録したフォルダの `__NEWSONG__`）など。バーの種類）
    pub kind: i32,
}

impl CustomFolder {
    /// 開くと SQL で譜面を引くか（LR2: `tag` が 3 バイト以上で `(null)` でない）
    pub fn has_command(&self) -> bool {
        self.command.len() > 2 && self.command != "(null)"
    }
}

/// カスタムフォルダの SQL で引いた譜面
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomResult {
    /// 引いた順の譜面のパス
    pub paths: Vec<PathBuf>,
    /// 並びが決まっている（`ORDER BY` を書いた・無作為に選んだ）。絞り込み・並べ替えをしない
    pub ordered: bool,
}

/// 一覧を作るときに引くもの（記録とカスタムフォルダの SQL）
pub trait Lookup {
    /// 並べ替えに使う記録
    fn record(&self, hash: &ChartHash) -> SortRecord;
    /// カスタムフォルダの SQL の結果（引けなければ None）
    fn custom(&self, _folder: &CustomFolder) -> Option<CustomResult> {
        None
    }
}

/// 記録もカスタムフォルダの SQL も引かない
pub struct NoLookup;

impl Lookup for NoLookup {
    fn record(&self, _: &ChartHash) -> SortRecord {
        SortRecord::default()
    }
}

impl<F: Fn(&ChartHash) -> SortRecord> Lookup for F {
    fn record(&self, hash: &ChartHash) -> SortRecord {
        self(hash)
    }
}

/// 譜面の置き場所（登録したフォルダ・その中のすべてのフォルダ・譜面・カスタムフォルダ・難易度表）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    roots: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    songs: Vec<SongInfo>,
    custom: Vec<CustomFolder>,
    tables: Vec<DifficultyTable>,
    /// 譜面のパス → 添字
    by_path: HashMap<PathBuf, usize>,
    /// MD5・SHA-256（小文字の 16 進）→ 最初の譜面の添字
    by_md5: HashMap<String, usize>,
    by_sha256: HashMap<String, usize>,
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
    /// カスタムフォルダ（`Library::custom` の添字）
    Custom(usize),
    /// 難易度表（`Library::tables` の添字。reprise の拡張）
    Table(usize),
    /// 難易度表のレベル（表・レベルの添字）
    Level(usize, usize),
    /// 難易度表にあって曲の一覧に無い譜面（表・レベル・何番目）
    Missing(usize, usize, usize),
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

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 難易度表とそのレベルを一覧の段として開き直すためのパス（ファイルのパスにならない）
const TABLE_KEY: &str = "\0table\0";

impl Library {
    /// `folders` は登録したフォルダの中にあるすべてのフォルダ（登録したフォルダ自身は含めなくてよい）。
    /// `#DIFFICULTY` の無い譜面には難易度を付ける（DOM-SEL-17）
    pub fn new(roots: Vec<PathBuf>, folders: Vec<PathBuf>, mut songs: Vec<SongInfo>) -> Self {
        filter::assign_undefined_difficulty(&mut songs);
        let mut by_path = HashMap::new();
        let mut by_md5 = HashMap::new();
        let mut by_sha256 = HashMap::new();
        for (i, s) in songs.iter().enumerate() {
            by_path.entry(s.path.clone()).or_insert(i);
            by_md5.entry(hex(&s.hash.md5)).or_insert(i);
            if let Some(sha) = &s.hash.sha256 {
                by_sha256.entry(hex(sha)).or_insert(i);
            }
        }
        Self {
            roots,
            folders,
            songs,
            custom: vec![],
            tables: vec![],
            by_path,
            by_md5,
            by_sha256,
        }
    }
    /// カスタムフォルダ（登録したフォルダが `.lr2folder` なら最初の一覧に、それ以外は置いてあるフォルダの一覧に出る）
    pub fn with_custom(mut self, custom: Vec<CustomFolder>) -> Self {
        self.custom = custom;
        self
    }
    /// 難易度表（最初の一覧の最後にこの順で出る。reprise の拡張）
    pub fn with_tables(mut self, tables: Vec<DifficultyTable>) -> Self {
        self.tables = tables;
        self
    }
    pub fn songs(&self) -> &[SongInfo] {
        &self.songs
    }
    pub fn custom(&self) -> &[CustomFolder] {
        &self.custom
    }
    pub fn tables(&self) -> &[DifficultyTable] {
        &self.tables
    }

    /// フォルダとカスタムフォルダをパスの大文字比較順に並べる（LR2 の `CMP_SongDataByPath`）
    fn folder_bars<'a>(&self, paths: impl Iterator<Item = &'a PathBuf>) -> Vec<Bar> {
        let mut v: Vec<&PathBuf> = paths.collect();
        v.sort_by_key(|p| upper(p));
        v.dedup();
        v.into_iter()
            .map(|p| match self.custom.iter().position(|c| &c.path == p) {
                Some(c) => Bar::Custom(c),
                None => folder_bar(p),
            })
            .collect()
    }

    /// 最初の一覧 = 登録したフォルダ（DOM-SEL-01, 23）と難易度表（DOM-SEL-25）
    pub fn root_bars(&self) -> Vec<Bar> {
        let mut bars = self.folder_bars(self.roots.iter());
        bars.extend((0..self.tables.len()).map(Bar::Table));
        bars
    }

    /// 譜面の添字（パスで引く）
    pub fn song_by_path(&self, path: &Path) -> Option<usize> {
        self.by_path.get(path).copied()
    }

    /// 難易度表の譜面に当たる曲（MD5、無ければ SHA-256 で引く。beatoraja の `SongBar.matches`）
    pub fn table_song(&self, chart: &TableChart) -> Option<usize> {
        let md5 = (!chart.md5.is_empty())
            .then(|| self.by_md5.get(&chart.md5))
            .flatten();
        let sha = || {
            (!chart.sha256.is_empty())
                .then(|| self.by_sha256.get(&chart.sha256))
                .flatten()
        };
        md5.or_else(sha).copied()
    }

    fn table_key(t: usize, level: Option<usize>) -> PathBuf {
        match level {
            None => PathBuf::from(format!("{TABLE_KEY}{t}")),
            Some(l) => PathBuf::from(format!("{TABLE_KEY}{t}\0{l}")),
        }
    }

    /// 難易度表・レベルを開き直すためのパス → (表, レベル)
    fn parse_table_key(&self, p: &Path) -> Option<(usize, Option<usize>)> {
        let rest = p.to_str()?.strip_prefix(TABLE_KEY)?;
        let mut it = rest.split('\0');
        let t: usize = it.next()?.parse().ok()?;
        let l = match it.next() {
            Some(x) => Some(x.parse().ok()?),
            None => None,
        };
        Some((t, l))
    }

    /// フォルダを開いた一覧（DOM-SEL-02, 03, 12..15, 24, 25）。開けなければ None。
    /// 譜面は `filter` で絞り込んで並べ（絞り込みを先へ回したら `filter` を書き換える）、`look` は並べ替えに使う記録と
    /// カスタムフォルダの SQL を引く
    pub fn open(
        &self,
        folder: &Path,
        filter: &mut ListFilter,
        look: &dyn Lookup,
    ) -> Option<Listing> {
        if let Some((t, level)) = self.parse_table_key(folder) {
            return self.open_table(t, level);
        }
        if let Some(c) = self.custom.iter().find(|c| c.path == folder) {
            if c.has_command() {
                if let Some(r) = look.custom(c) {
                    let ids: Vec<usize> = r
                        .paths
                        .iter()
                        .filter_map(|p| self.song_by_path(p))
                        .collect();
                    let listed =
                        filter::list_custom(&self.songs, &ids, filter, c.max, r.ordered, look);
                    if !listed.is_empty() {
                        return Some(Listing {
                            bars: listed.iter().map(|(i, _)| Bar::Song(*i)).collect(),
                            groups: listed.iter().map(|(_, g)| Some(*g)).collect(),
                        });
                    }
                }
            }
        }
        let song_dir = |s: &SongInfo| s.path.parent().map(Path::to_path_buf).unwrap_or_default();
        let songs: Vec<usize> = (0..self.songs.len())
            .filter(|&i| song_dir(&self.songs[i]).parent() == Some(folder))
            .collect();
        if !songs.is_empty() {
            let listed = filter::list_songs(&self.songs, &songs, filter, look);
            return (!listed.is_empty()).then(|| Listing {
                bars: listed.iter().map(|(i, _)| Bar::Song(*i)).collect(),
                groups: listed.iter().map(|(_, g)| Some(*g)).collect(),
            });
        }
        let subs = self.folder_bars(
            self.folders
                .iter()
                .chain(self.custom.iter().map(|c| &c.path))
                .filter(|p| p.parent() == Some(folder) && !self.roots.contains(p)),
        );
        (!subs.is_empty()).then(|| Listing::folders(subs))
    }

    /// DOM-SEL-25: 難易度表はレベルを表の順に、レベルは譜面を表の順に（曲の一覧に無ければ `Missing`）並べる。
    /// 絞り込み・並べ替えはしない
    fn open_table(&self, t: usize, level: Option<usize>) -> Option<Listing> {
        let table = self.tables.get(t)?;
        let bars: Vec<Bar> = match level {
            None => (0..table.levels.len()).map(|l| Bar::Level(t, l)).collect(),
            Some(l) => table
                .levels
                .get(l)?
                .charts
                .iter()
                .enumerate()
                .map(|(k, c)| match self.table_song(c) {
                    Some(i) => Bar::Song(i),
                    None => Bar::Missing(t, l, k),
                })
                .collect(),
        };
        if bars.is_empty() {
            return None;
        }
        let groups = bars
            .iter()
            .map(|b| matches!(b, Bar::Song(_)).then(Group::default))
            .collect();
        Some(Listing { bars, groups })
    }

    /// 難易度表の譜面
    pub fn table_chart(&self, t: usize, l: usize, k: usize) -> Option<&TableChart> {
        self.tables.get(t)?.levels.get(l)?.charts.get(k)
    }

    /// バーのパス（曲は譜面、フォルダはフォルダ。LR2 の `filepath`）。難易度表とそのレベル・無い譜面はファイルのパスにならない
    pub fn bar_path(&self, bar: &Bar) -> PathBuf {
        match bar {
            Bar::Folder { path, .. } => path.clone(),
            Bar::Song(i) => self.songs[*i].path.clone(),
            Bar::Custom(c) => self.custom[*c].path.clone(),
            Bar::Table(t) => Self::table_key(*t, None),
            Bar::Level(t, l) => Self::table_key(*t, Some(*l)),
            Bar::Missing(t, l, k) => {
                let c = self.table_chart(*t, *l, *k);
                PathBuf::from(format!(
                    "{TABLE_KEY}chart\0{}\0{}",
                    c.map_or("", |c| &c.md5),
                    c.map_or("", |c| &c.sha256)
                ))
            }
        }
    }

    /// 開けるバーなら開くパス
    fn open_path(&self, bar: &Bar) -> Option<PathBuf> {
        match bar {
            Bar::Song(_) | Bar::Missing(..) => None,
            b => Some(self.bar_path(b)),
        }
    }
}

/// 一覧（バーと、曲なら難易度の絞り込みの単位の中での情報）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listing {
    pub bars: Vec<Bar>,
    pub groups: Vec<Option<Group>>,
}

impl Listing {
    fn folders(bars: Vec<Bar>) -> Self {
        Self {
            groups: vec![None; bars.len()],
            bars,
        }
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
    list: Listing,
    cursor: usize,
    /// 開く前の一覧とカーソルと開いたフォルダ
    stack: Vec<(Listing, usize, PathBuf)>,
}

impl SelectState {
    pub fn new(library: &Library) -> Self {
        Self {
            list: Listing::folders(library.root_bars()),
            cursor: 0,
            stack: vec![],
        }
    }
    pub fn bars(&self) -> &[Bar] {
        &self.list.bars
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn current(&self) -> Option<&Bar> {
        self.list.bars.get(self.cursor)
    }
    /// 選んでいる曲の難易度の絞り込みの単位の中での情報
    pub fn current_group(&self) -> Option<&Group> {
        self.list.groups.get(self.cursor)?.as_ref()
    }
    /// カーソルを置く（一覧の本数で回す。選曲スキンの一覧の動き [`BarScroll`] から決める）
    pub fn set_cursor(&mut self, cursor: usize) {
        let n = self.list.bars.len();
        self.cursor = if n == 0 { 0 } else { cursor % n };
    }
    /// 開いているフォルダの深さ（最初の一覧は 0）
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
    /// カーソルを動かす（端で反対側へ回る）
    pub fn scroll(&mut self, delta: i32) {
        let n = self.list.bars.len() as i64;
        if n > 0 {
            self.cursor = (self.cursor as i64 + delta as i64).rem_euclid(n) as usize;
        }
    }
    pub fn decide(
        &mut self,
        library: &Library,
        filter: &mut ListFilter,
        look: &dyn Lookup,
    ) -> Decided {
        match self.current() {
            Some(Bar::Song(i)) => Decided::Song(*i),
            Some(bar) => {
                let Some(path) = library.open_path(bar) else {
                    return Decided::Nothing;
                };
                match library.open(&path, filter, look) {
                    Some(list) => {
                        let prev = std::mem::replace(&mut self.list, list);
                        self.stack.push((prev, self.cursor, path));
                        self.cursor = 0;
                        Decided::Opened
                    }
                    None => Decided::Nothing,
                }
            }
            None => Decided::Nothing,
        }
    }
    /// 1 つ上の一覧へ戻る（最初の一覧なら false）
    pub fn back(&mut self) -> bool {
        match self.stack.pop() {
            Some((list, cursor, _)) => {
                self.list = list;
                self.cursor = cursor;
                true
            }
            None => false,
        }
    }

    /// DOM-SEL-20: 絞り込み・並べ替えを替えたので今の一覧を作り直す（LR2 `SetBmsFilter` → `InitSelectBySearchResult`）。
    /// カーソルは 同じパスのバー → 同じ譜面のフォルダと鍵盤数で同じ難易度の 1 つ目の譜面 →（鍵盤数を替えたなら）同じ譜面のフォルダの
    /// 同じ難易度の 1 つ目の譜面 → 先頭 の順に探す
    pub fn refilter(
        &mut self,
        library: &Library,
        filter: &mut ListFilter,
        look: &dyn Lookup,
        key_changed: bool,
    ) {
        let list = match self.stack.last() {
            None => Listing::folders(library.root_bars()),
            Some((_, _, folder)) => match library.open(folder, filter, look) {
                Some(l) => l,
                None => return,
            },
        };
        let selected = self.current().map(|b| match b {
            Bar::Song(i) => {
                let s = &library.songs()[*i];
                (
                    s.path.clone(),
                    Some((s.path.parent().map(Path::to_path_buf), s.key_mode)),
                )
            }
            other => (library.bar_path(other), None),
        });
        self.list = list;
        let Some((path, song)) = selected else {
            self.cursor = 0;
            return;
        };
        let first_of = |want: &dyn Fn(&SongInfo) -> bool| {
            self.list
                .bars
                .iter()
                .zip(&self.list.groups)
                .position(|(b, g)| match (b, g) {
                    (Bar::Song(i), Some(g)) => g.count == 0 && want(&library.songs()[*i]),
                    _ => false,
                })
        };
        let dir = |s: &SongInfo| s.path.parent().map(Path::to_path_buf);
        let found = self
            .list
            .bars
            .iter()
            .position(|b| library.bar_path(b) == path)
            .or_else(|| {
                let (d, km) = song.clone()?;
                first_of(&|s| dir(s) == d && s.key_mode == km)
            })
            .or_else(|| {
                let (d, _) = song.clone()?;
                key_changed.then(|| first_of(&|s| dir(s) == d)).flatten()
            });
        self.cursor = found.unwrap_or(0);
    }

    /// DOM-SEL-22: 曲一覧が替わったので、開いていたフォルダを開き直してカーソルを置き直す（reprise の拡張）
    /// `old` は前の曲一覧（今の一覧の曲の添字はこれを指す）
    pub fn reload(
        &mut self,
        old: &Library,
        library: &Library,
        filter: &mut ListFilter,
        look: &dyn Lookup,
    ) {
        // 段ごとの (一覧, カーソル)。最後が今の一覧
        let old_levels: Vec<(Listing, usize)> = self
            .stack
            .iter()
            .map(|(l, c, _)| (l.clone(), *c))
            .chain(std::iter::once((self.list.clone(), self.cursor)))
            .collect();
        let opened: Vec<PathBuf> = self.stack.iter().map(|(_, _, p)| p.clone()).collect();
        // 前の一覧で選んでいたバーのパス
        let old_paths: Vec<Option<PathBuf>> = old_levels
            .iter()
            .map(|(l, c)| l.bars.get(*c).map(|b| old.bar_path(b)))
            .collect();
        let place = |list: &Listing, k: usize| -> usize {
            let n = list.bars.len();
            if n == 0 {
                return 0;
            }
            old_paths[k]
                .as_ref()
                .and_then(|p| list.bars.iter().position(|b| &library.bar_path(b) == p))
                .unwrap_or(old_levels[k].1.min(n - 1))
        };
        let mut list = Listing::folders(library.root_bars());
        let mut stack = vec![];
        for (k, folder) in opened.iter().enumerate() {
            let Some(next) = library.open(folder, filter, look) else {
                break;
            };
            let c = place(&list, k);
            stack.push((std::mem::replace(&mut list, next), c, folder.clone()));
        }
        self.cursor = place(&list, stack.len());
        self.list = list;
        self.stack = stack;
    }

    /// DOM-SEL-16: → か SELECT（選んでいるのが曲のとき）。難易度の絞り込みを替えて一覧を作り直す。曲でなければ false
    pub fn change_difficulty(
        &mut self,
        library: &Library,
        filter: &mut ListFilter,
        look: &dyn Lookup,
    ) -> bool {
        let (Some(Bar::Song(i)), Some(g)) = (self.current(), self.current_group()) else {
            return false;
        };
        filter.difficulty =
            filter::next_difficulty(filter.difficulty, library.songs()[*i].difficulty, g);
        self.refilter(library, filter, look, false);
        true
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
#[cfg(test)]
mod tests_custom;
#[cfg(test)]
mod tests_reload;
