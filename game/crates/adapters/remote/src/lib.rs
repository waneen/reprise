//! 曲サーバー（`reprise-server`）とのやりとり（reprise の拡張）。ブラウザ版はこのクレートのポートの実装
//! （`RemoteFs`・`RemoteScores`・`RemoteSongQuery`・`RemoteLibrary`）でサーバーの曲・記録・曲の DB を使い、
//! ネイティブ版と同じ選曲画面とプレイを動かす。HTTP そのものは [`Transport`] で外から渡す（ブラウザ版はページの
//! JavaScript、テストは ureq）。サーバーと同じ形（`*Dto`）もここに置き、サーバーはこれで答える。
//! パスはサーバーが決めた「仮想のパス」（`/songs/…`・`/skin/…`）で、クライアントはそのまま使う。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | RMT-01 | 曲一覧の形: 登録したフォルダ・フォルダ・譜面（MD5・SHA-256 は小文字の 16 進）・カスタムフォルダ・難易度表をそのまま運ぶ。パスは渡した関数で替え、替えられないものは落とす。読み戻すと同じ曲一覧（読めないハッシュの譜面は落とす） |
//! | RMT-02 | 記録の形: 譜面・ルール（`lr2`/`beatoraja`）・ランプ（NO PLAY = 0 … MAX = 10）・判定数・最大コンボ・最小 BP（無ければ null）・回数・どこの記録か（reprise なら null）。読み戻すと同じ記録 |
//! | RMT-03 | クエリの値は英数字と `-_.~/` 以外のバイトを `%XX`（大文字）にする |
//! | RMT-04 | ファイル（`RemoteFs`）: 中身は `GET /api/list?path=`（読めなければ空。フォルダごとに覚える）、読むのは `GET /api/file?path=`（200 以外は誤り）、ファイルかどうかは親のフォルダの中身にファイルとしてあるか |
//! | RMT-05 | 記録（`RemoteScores`）: 初めて探すときに `GET /api/scores` ですべての記録をもらって覚え（もらえなければ誤りで、覚えない）、覚えたものから同じ MD5・ルールの記録を返す。保存は `POST /api/score` で、できたら覚えたものも替える |
//! | RMT-06 | 曲の DB の SQL（`RemoteSongQuery`）: `POST /api/query` に SQL を送り、200 ならパスの並び、400 なら SQL の誤り |
//! | RMT-07 | 曲一覧の届け先（`RemoteLibrary`）: 前に確かめてから 2 秒たったら `GET /api/version` を見て、版が上がっていれば `GET /api/library` の曲一覧を返し、覚えたフォルダの中身を忘れる |
//! | RMT-08 | 応答の読み方: 2xx は成功、401 は合言葉の誤り、0 はつながらない、404 は無い、400 は送ったものの誤り（中身を伝える）、ほかはサーバーの誤り |
//! | RMT-09 | 難易度表の取り直し: `POST /api/tables/update` を送り、答えの版を返す |
use application::ports::{
    DirEntry, FileSystem, LibrarySource, PortError, ScoreRepository, SongQuery,
};
use domain::chart::{ChartHash, KeyMode};
use domain::play::RulesetId;
use domain::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use domain::select::{CustomFolder, DifficultyTable, Library, SongInfo, TableChart, TableLevel};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 曲一覧（`GET /api/library` の `library`）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryDto {
    pub roots: Vec<String>,
    pub folders: Vec<String>,
    pub songs: Vec<SongDto>,
    #[serde(default)]
    pub custom: Vec<CustomDto>,
    #[serde(default)]
    pub tables: Vec<TableDto>,
}

/// 譜面
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SongDto {
    pub path: String,
    pub md5: String,
    #[serde(default)]
    pub sha256: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub genre: String,
    pub level: i32,
    pub difficulty: i32,
    /// 鍵盤数（5・7・9・10・14）
    pub keys: u8,
    pub notes: u32,
}

/// カスタムフォルダ
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CustomDto {
    pub path: String,
    pub title: String,
    pub category: String,
    pub info_a: String,
    pub info_b: String,
    pub command: String,
    pub max: i32,
    pub kind: i32,
}

/// 難易度表
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableDto {
    pub url: String,
    pub name: String,
    pub levels: Vec<LevelDto>,
}

/// 難易度表のレベル
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelDto {
    pub name: String,
    pub charts: Vec<ChartDto>,
}

/// 難易度表の譜面
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartDto {
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
}

/// `GET /api/library` の答え
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryResponse {
    pub version: u64,
    pub library: LibraryDto,
}

/// `GET /api/version` の答え
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VersionDto {
    pub version: u64,
}

/// `GET /api/list` の 1 つ
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryDto {
    pub name: String,
    pub path: String,
    pub is_file: bool,
}

/// `POST /api/query` に送るもの
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryDto {
    pub sql: String,
}

/// `POST /api/query` の答え
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PathsDto {
    pub paths: Vec<String>,
}

/// 記録（`GET /api/score` の答えと `POST /api/score` に送るもの）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScoreDto {
    pub md5: String,
    #[serde(default)]
    pub sha256: String,
    /// `lr2` か `beatoraja`
    pub ruleset: String,
    /// NO PLAY = 0 … MAX = 10
    pub lamp: u8,
    pub ex_score: u32,
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub max_combo: u32,
    pub min_bp: Option<u32>,
    pub play_count: u32,
    pub clear_count: u32,
    /// どこから取り込んだ記録か（reprise で出した記録なら None）
    pub origin: Option<String>,
}

/// 小文字の 16 進
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 16 進を N バイトに（長さが違うか読めなければ None）
pub fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

fn keys_of(k: KeyMode) -> u8 {
    match k {
        KeyMode::K5 => 5,
        KeyMode::K7 => 7,
        KeyMode::K9 => 9,
        KeyMode::K10 => 10,
        KeyMode::K14 => 14,
    }
}

fn key_mode(n: u8) -> Option<KeyMode> {
    Some(match n {
        5 => KeyMode::K5,
        7 => KeyMode::K7,
        9 => KeyMode::K9,
        10 => KeyMode::K10,
        14 => KeyMode::K14,
        _ => return None,
    })
}

/// RMT-01: 曲一覧を運ぶ形にする（パスは `map` で替え、替えられないものは落とす）
pub fn library_to_dto(lib: &Library, map: &dyn Fn(&Path) -> Option<String>) -> LibraryDto {
    LibraryDto {
        roots: lib.roots().iter().filter_map(|p| map(p)).collect(),
        folders: lib.folders().iter().filter_map(|p| map(p)).collect(),
        songs: lib
            .songs()
            .iter()
            .filter_map(|s| {
                Some(SongDto {
                    path: map(&s.path)?,
                    md5: hex(&s.hash.md5),
                    sha256: s.hash.sha256.map(|h| hex(&h)).unwrap_or_default(),
                    title: s.title.clone(),
                    subtitle: s.subtitle.clone(),
                    artist: s.artist.clone(),
                    genre: s.genre.clone(),
                    level: s.level,
                    difficulty: s.difficulty,
                    keys: keys_of(s.key_mode),
                    notes: s.notes,
                })
            })
            .collect(),
        custom: lib
            .custom()
            .iter()
            .filter_map(|c| {
                Some(CustomDto {
                    path: map(&c.path)?,
                    title: c.title.clone(),
                    category: c.category.clone(),
                    info_a: c.info_a.clone(),
                    info_b: c.info_b.clone(),
                    command: c.command.clone(),
                    max: c.max,
                    kind: c.kind,
                })
            })
            .collect(),
        tables: lib
            .tables()
            .iter()
            .map(|t| TableDto {
                url: t.url.clone(),
                name: t.name.clone(),
                levels: t
                    .levels
                    .iter()
                    .map(|l| LevelDto {
                        name: l.name.clone(),
                        charts: l
                            .charts
                            .iter()
                            .map(|c| ChartDto {
                                md5: c.md5.clone(),
                                sha256: c.sha256.clone(),
                                title: c.title.clone(),
                                artist: c.artist.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// RMT-01: 運ぶ形から曲一覧に戻す（MD5 か SHA-256・鍵盤数が読めない譜面は落とす）
pub fn library_from_dto(d: LibraryDto) -> Library {
    let songs = d
        .songs
        .into_iter()
        .filter_map(|s| {
            let sha256 = if s.sha256.is_empty() {
                None
            } else {
                Some(unhex::<32>(&s.sha256)?)
            };
            Some(SongInfo {
                path: PathBuf::from(s.path),
                hash: ChartHash {
                    md5: unhex::<16>(&s.md5)?,
                    sha256,
                },
                title: s.title,
                subtitle: s.subtitle,
                artist: s.artist,
                genre: s.genre,
                level: s.level,
                difficulty: s.difficulty,
                key_mode: key_mode(s.keys)?,
                notes: s.notes,
            })
        })
        .collect();
    let custom = d
        .custom
        .into_iter()
        .map(|c| CustomFolder {
            path: PathBuf::from(c.path),
            title: c.title,
            category: c.category,
            info_a: c.info_a,
            info_b: c.info_b,
            command: c.command,
            max: c.max,
            kind: c.kind,
        })
        .collect();
    let tables = d
        .tables
        .into_iter()
        .map(|t| DifficultyTable {
            url: t.url,
            name: t.name,
            levels: t
                .levels
                .into_iter()
                .map(|l| TableLevel {
                    name: l.name,
                    charts: l
                        .charts
                        .into_iter()
                        .map(|c| TableChart {
                            md5: c.md5,
                            sha256: c.sha256,
                            title: c.title,
                            artist: c.artist,
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Library::new(
        d.roots.into_iter().map(PathBuf::from).collect(),
        d.folders.into_iter().map(PathBuf::from).collect(),
        songs,
    )
    .with_custom(custom)
    .with_tables(tables)
}

const LAMPS: [ClearLamp; 11] = [
    ClearLamp::NoPlay,
    ClearLamp::Failed,
    ClearLamp::AssistEasy,
    ClearLamp::LightAssistEasy,
    ClearLamp::Easy,
    ClearLamp::Normal,
    ClearLamp::Hard,
    ClearLamp::ExHard,
    ClearLamp::FullCombo,
    ClearLamp::Perfect,
    ClearLamp::Max,
];

/// ルールの名前（`lr2`・`beatoraja`）
pub fn ruleset_name(r: RulesetId) -> &'static str {
    match r {
        RulesetId::Lr2 => "lr2",
        RulesetId::Beatoraja => "beatoraja",
    }
}

/// ルールの名前から（知らない名前は None）
pub fn ruleset_of(s: &str) -> Option<RulesetId> {
    match s {
        "lr2" => Some(RulesetId::Lr2),
        "beatoraja" => Some(RulesetId::Beatoraja),
        _ => None,
    }
}

/// RMT-02: 記録を運ぶ形にする
pub fn score_to_dto(r: &ScoreRecord) -> ScoreDto {
    ScoreDto {
        md5: hex(&r.chart.md5),
        sha256: r.chart.sha256.map(|h| hex(&h)).unwrap_or_default(),
        ruleset: ruleset_name(r.ruleset).into(),
        lamp: LAMPS.iter().position(|l| *l == r.lamp).unwrap_or(0) as u8,
        ex_score: r.ex_score,
        pgreat: r.pgreat,
        great: r.great,
        good: r.good,
        bad: r.bad,
        poor: r.poor,
        max_combo: r.max_combo,
        min_bp: r.min_bp,
        play_count: r.play_count,
        clear_count: r.clear_count,
        origin: match &r.origin {
            ScoreOrigin::Native => None,
            ScoreOrigin::ImportedFrom(s) => Some(s.clone()),
        },
    }
}

/// RMT-02: 運ぶ形から記録に戻す（読めなければ None）
pub fn score_from_dto(d: &ScoreDto) -> Option<ScoreRecord> {
    let sha256 = if d.sha256.is_empty() {
        None
    } else {
        Some(unhex::<32>(&d.sha256)?)
    };
    Some(ScoreRecord {
        chart: ChartHash {
            md5: unhex::<16>(&d.md5)?,
            sha256,
        },
        ruleset: ruleset_of(&d.ruleset)?,
        lamp: *LAMPS.get(d.lamp as usize)?,
        ex_score: d.ex_score,
        pgreat: d.pgreat,
        great: d.great,
        good: d.good,
        bad: d.bad,
        poor: d.poor,
        max_combo: d.max_combo,
        min_bp: d.min_bp,
        play_count: d.play_count,
        clear_count: d.clear_count,
        origin: match &d.origin {
            None => ScoreOrigin::Native,
            Some(s) => ScoreOrigin::ImportedFrom(s.clone()),
        },
    })
}

/// RMT-03: クエリの値の符号化
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// HTTP を送るもの（ブラウザ版はページの JavaScript、テストは ureq）
pub trait Transport: Send + Sync {
    /// `method` を `target`（`/api/…` とクエリ）に送り、(状態, 中身) を返す。通じなければ状態 0
    fn request(&self, method: &str, target: &str, body: Option<&[u8]>) -> (u16, Vec<u8>);
}

/// RMT-08: 応答の読み方
pub fn check(status: u16, body: Vec<u8>) -> Result<Vec<u8>, PortError> {
    match status {
        200..=299 => Ok(body),
        0 => Err(PortError::Io("曲サーバーにつながらない".into())),
        401 => Err(PortError::Io("曲サーバーの合言葉が違う".into())),
        404 => Err(PortError::Io("曲サーバーに無い".into())),
        400 => Err(PortError::Parse(
            String::from_utf8_lossy(&body).into_owned(),
        )),
        s => Err(PortError::Io(format!("曲サーバーの誤り（{s}）"))),
    }
}

fn json<T: DeserializeOwned>(body: &[u8]) -> Result<T, PortError> {
    serde_json::from_slice(body).map_err(|e| PortError::Parse(format!("曲サーバーの答え: {e}")))
}

/// 曲サーバーへのつなぎ（フォルダの中身を覚える）
pub struct Remote {
    transport: Box<dyn Transport>,
    dirs: Mutex<HashMap<PathBuf, Vec<DirEntry>>>,
}

impl Remote {
    pub fn new(transport: impl Transport + 'static) -> Arc<Self> {
        Arc::new(Self {
            transport: Box::new(transport),
            dirs: Mutex::new(HashMap::new()),
        })
    }
    fn call(&self, method: &str, target: &str, body: Option<&[u8]>) -> Result<Vec<u8>, PortError> {
        let (status, bytes) = self.transport.request(method, target, body);
        check(status, bytes)
    }
    /// 曲一覧の版
    pub fn version(&self) -> Result<u64, PortError> {
        Ok(json::<VersionDto>(&self.call("GET", "/api/version", None)?)?.version)
    }
    /// 曲一覧とその版
    pub fn library(&self) -> Result<(u64, Library), PortError> {
        let r: LibraryResponse = json(&self.call("GET", "/api/library", None)?)?;
        Ok((r.version, library_from_dto(r.library)))
    }
    /// RMT-09: 難易度表を取り直してもらう（答えは曲一覧の版）
    pub fn update_tables(&self) -> Result<u64, PortError> {
        Ok(json::<VersionDto>(&self.call("POST", "/api/tables/update", None)?)?.version)
    }
    /// 覚えたフォルダの中身を忘れる
    pub fn forget(&self) {
        self.dirs.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// ファイル（RMT-04）
pub struct RemoteFs(pub Arc<Remote>);

impl FileSystem for RemoteFs {
    fn list(&self, dir: &Path) -> Vec<DirEntry> {
        let key = dir.to_path_buf();
        if let Some(v) = self
            .0
            .dirs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return v.clone();
        }
        let target = format!("/api/list?path={}", encode(&dir.to_string_lossy()));
        let Ok(entries) = self
            .0
            .call("GET", &target, None)
            .and_then(|b| json::<Vec<EntryDto>>(&b))
        else {
            return vec![];
        };
        let v: Vec<DirEntry> = entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                path: PathBuf::from(e.path),
                is_file: e.is_file,
            })
            .collect();
        self.0
            .dirs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, v.clone());
        v
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>, PortError> {
        let target = format!("/api/file?path={}", encode(&path.to_string_lossy()));
        self.0.call("GET", &target, None)
    }
    fn is_file(&self, path: &Path) -> bool {
        let Some(dir) = path.parent() else {
            return false;
        };
        self.list(dir).iter().any(|e| e.is_file && e.path == path)
    }
}

/// 記録の覚え（(MD5, ルール) → 記録）
type ScoreMap = HashMap<([u8; 16], &'static str), ScoreRecord>;

fn score_key(r: &ScoreRecord) -> ([u8; 16], &'static str) {
    (r.chart.md5, ruleset_name(r.ruleset))
}

/// 記録（RMT-05。選曲の一覧はバーごとに記録を引くので、すべてをまとめてもらって覚える）
pub struct RemoteScores {
    remote: Arc<Remote>,
    known: Mutex<Option<ScoreMap>>,
}

impl RemoteScores {
    pub fn new(remote: Arc<Remote>) -> Self {
        Self {
            remote,
            known: Mutex::new(None),
        }
    }
}

impl ScoreRepository for RemoteScores {
    fn find(
        &self,
        chart: &ChartHash,
        ruleset: RulesetId,
    ) -> Result<Option<ScoreRecord>, PortError> {
        let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        if known.is_none() {
            let all: Vec<ScoreDto> = json(&self.remote.call("GET", "/api/scores", None)?)?;
            let map = all
                .iter()
                .filter_map(score_from_dto)
                .map(|r| (score_key(&r), r))
                .collect();
            *known = Some(map);
        }
        Ok(known
            .as_ref()
            .and_then(|m| m.get(&(chart.md5, ruleset_name(ruleset))).cloned()))
    }
    fn save(&mut self, record: ScoreRecord) -> Result<(), PortError> {
        let body = serde_json::to_vec(&score_to_dto(&record))
            .map_err(|e| PortError::Parse(e.to_string()))?;
        self.remote.call("POST", "/api/score", Some(&body))?;
        if let Some(m) = self
            .known
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            m.insert(score_key(&record), record);
        }
        Ok(())
    }
}

/// 曲の DB の SQL（RMT-06）
pub struct RemoteSongQuery(pub Arc<Remote>);

impl SongQuery for RemoteSongQuery {
    fn song_paths(&self, sql: &str) -> Result<Vec<PathBuf>, PortError> {
        let body = serde_json::to_vec(&QueryDto { sql: sql.into() })
            .map_err(|e| PortError::Parse(e.to_string()))?;
        let r: PathsDto = json(&self.0.call("POST", "/api/query", Some(&body))?)?;
        Ok(r.paths.into_iter().map(PathBuf::from).collect())
    }
}

/// 曲一覧を確かめる間隔（ms）
pub const POLL_MS: f64 = 2000.0;

/// 曲一覧の届け先（RMT-07）
pub struct RemoteLibrary {
    remote: Arc<Remote>,
    version: Mutex<u64>,
    last: Mutex<f64>,
    /// 今の時刻（ms）
    now: fn() -> f64,
}

impl RemoteLibrary {
    /// `version` は持っている曲一覧の版
    pub fn new(remote: Arc<Remote>, version: u64, now: fn() -> f64) -> Self {
        Self {
            remote,
            version: Mutex::new(version),
            last: Mutex::new(now()),
            now,
        }
    }
}

impl LibrarySource for RemoteLibrary {
    fn poll(&self) -> Option<Library> {
        let t = (self.now)();
        {
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            if t - *last < POLL_MS {
                return None;
            }
            *last = t;
        }
        let v = self.remote.version().ok()?;
        let mut cur = self.version.lock().unwrap_or_else(|e| e.into_inner());
        if v <= *cur {
            return None;
        }
        let (v, lib) = self.remote.library().ok()?;
        *cur = v;
        self.remote.forget();
        Some(lib)
    }
}

#[cfg(test)]
mod tests;
