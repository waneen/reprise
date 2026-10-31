//! 曲サーバー（reprise の拡張。LR2 には無い）。ネイティブ版と同じ曲の DB（`song.db`）・フォルダの見張り・難易度表・
//! 記録（`score.db`）を持ち、ブラウザ版（`remote` クレートのポートの実装）に HTTP で曲一覧・譜面・音・画像・スキンの
//! ファイルを配り、記録を預かる。ブラウザ版は同じ選曲画面とプレイをそのまま動かす。
//! ファイルは登録したフォルダと LR2 本体のフォルダの中だけを「仮想のパス」（`/songs/<番号>/<名前>/…`・`/skin/…`）で配り、
//! `/api/` はすべて合言葉（`Authorization: Bearer <合言葉>`）が要る。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | SRV-01 | 仮想のパス: 登録したフォルダを実のパスの大文字の順に並べ、i 番目を `/songs/<i を 3 桁>/<フォルダ名>`、LR2 本体のフォルダを `/skin` にする。中のパスは `/` でつなぐ。いちばん深い登録したフォルダで替え、外のパスは替えない。仮想のパスから実のパスにも戻せる |
//! | SRV-02 | 仮想のパスの `.`・`..`・空の部分（と Windows では `\`・`:`）、知らない最初の部分は受けない |
//! | SRV-03 | `/api/` は `Authorization: Bearer <合言葉>` が合っていなければ 401。ページ（`/api/` の外）は要らない |
//! | SRV-04 | `GET /api/library` は曲一覧（パスは仮想のパス）とその版、`GET /api/version` は版。版は起動時 1 で、曲一覧か難易度表が替わるたびに 1 上がる |
//! | SRV-05 | `GET /api/list?path=` はフォルダの中身（仮想のパス。`/`・`/songs`・`/songs/<i>` は登録したものの一覧）、`GET /api/file?path=` はファイルの中身。受けないパス・無いファイルは 404 |
//! | SRV-06 | `GET /api/scores` はすべての記録（SDB-05）、`GET /api/score?md5=&ruleset=` は 1 つの記録（無ければ `null`）、`POST /api/score` は記録を置き換える（204）。読めないものは 400 |
//! | SRV-07 | `POST /api/query` は SQL を読むだけの接続（SNG-09）で引き、パスを仮想のパスにして返す（替えられないものは落とす）。引けなければ 400 |
//! | SRV-08 | 合言葉: `--token` があればそれ、無ければ `<data>/server-token` の中身（前後の空白を除く）。それも無ければ 16 バイトの乱数の 16 進を作って置く |
//! | SRV-09 | `--web` のフォルダがあれば、`/api/` の外はそのフォルダのファイルを配る（`/` は `index.html`） |
//! | SRV-10 | `POST /api/tables/update` はすべての難易度表を取り直し（APP-TBL-03）、取れたものがあれば版を上げる。答えは版 |
//! | SRV-11 | 起動引数: `--data`（既定 `reprise-data`）の `config.json` の `songs`・`skin_root`・`tables` を使い、`--songs`・`--skin-root`・`--table` があれば替える（保存しない）。`--bind`（既定 `127.0.0.1:8770`）・`--token`・`--web`（無ければ今のフォルダの `web` に `index.html` があればそれ）。知らない引数は誤り |
use application::ports::{
    ChartLoader, FileSystem, LibrarySource, PortError, ScoreRepository, SettingsStore, SongQuery,
    TableSource,
};
use axum::body::Bytes;
use axum::extract::{Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use domain::select::{DifficultyTable, Library};
use remote::{
    library_to_dto, score_from_dto, score_to_dto, EntryDto, LibraryResponse, PathsDto, QueryDto,
    ScoreDto, VersionDto,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// 起動引数の説明
pub const USAGE: &str = "使い方: reprise-server [--data <フォルダ>] [--songs <譜面のフォルダ>...] [--skin-root <LR2 本体のフォルダ>] \
[--table <難易度表の URL>...] [--bind <アドレス:ポート>] [--token <合言葉>] [--web <ページのフォルダ>]";

/// 既定のデータのフォルダ（ネイティブ版と同じ）
pub const DATA_DIR: &str = "reprise-data";
/// 既定の待ち受け（この機械からだけ）
pub const BIND: &str = "127.0.0.1:8770";

/// 登録したフォルダと LR2 本体のフォルダの仮想のパス（SRV-01, 02）
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// (実のパス, 仮想のパス)
    songs: Vec<(PathBuf, String)>,
    skin: Option<PathBuf>,
}

fn parts(base: &Path, p: &Path) -> Option<Vec<String>> {
    let rel = p.strip_prefix(base).ok()?;
    rel.components()
        .map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn join(v: &str, parts: &[String]) -> String {
    let mut s = v.to_string();
    for p in parts {
        s.push('/');
        s.push_str(p);
    }
    s
}

/// 受けない部分（SRV-02）
fn bad_part(c: &str) -> bool {
    c.is_empty()
        || c == "."
        || c == ".."
        || c.contains('\0')
        || (cfg!(windows) && (c.contains('\\') || c.contains(':')))
}

impl Roots {
    pub fn new(songs: &[PathBuf], skin: Option<PathBuf>) -> Self {
        let mut sorted: Vec<&PathBuf> = songs.iter().collect();
        sorted.sort_by_key(|p| p.to_string_lossy().to_ascii_uppercase());
        sorted.dedup();
        let songs = sorted
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let name = p
                    .file_name()
                    .map_or_else(|| "root".into(), |n| n.to_string_lossy().into_owned());
                (p.clone(), format!("/songs/{i:03}/{name}"))
            })
            .collect();
        Self { songs, skin }
    }
    /// 実のパス → 仮想のパス（SRV-01）
    pub fn to_virtual(&self, p: &Path) -> Option<String> {
        let skin = self.skin.iter().map(|s| (s, "/skin"));
        let songs = self.songs.iter().map(|(r, v)| (r, v.as_str()));
        songs
            .chain(skin)
            .filter_map(|(r, v)| Some((r.components().count(), v, parts(r, p)?)))
            .max_by_key(|(depth, _, _)| *depth)
            .map(|(_, v, ps)| join(v, &ps))
    }
    /// 仮想のパス → 実のパス（SRV-01, 02）
    pub fn to_real(&self, v: &str) -> Option<PathBuf> {
        let list: Vec<&str> = v.strip_prefix('/')?.split('/').collect();
        if list.iter().any(|c| bad_part(c)) {
            return None;
        }
        let (base, rest) = match list.as_slice() {
            ["skin", rest @ ..] => (self.skin.clone()?, rest),
            ["songs", i, name, rest @ ..] => {
                let key = format!("/songs/{i}/{name}");
                let (r, _) = self.songs.iter().find(|(_, v)| *v == key)?;
                (r.clone(), rest)
            }
            _ => return None,
        };
        Some(rest.iter().fold(base, |p, c| p.join(c)))
    }
    /// `/`・`/songs`・`/songs/<i>` の中身（SRV-05。それ以外は None）
    pub fn list_virtual(&self, v: &str, fs: &dyn FileSystem) -> Option<Vec<EntryDto>> {
        let dir = |name: &str, path: String| EntryDto {
            name: name.into(),
            path,
            is_file: false,
        };
        match v.trim_end_matches('/') {
            "" => {
                let mut out = vec![dir("songs", "/songs".into())];
                if self.skin.is_some() {
                    out.push(dir("skin", "/skin".into()));
                }
                Some(out)
            }
            "/songs" => Some(
                self.songs
                    .iter()
                    .map(|(_, v)| {
                        let i = v.split('/').nth(2).unwrap_or_default();
                        dir(i, format!("/songs/{i}"))
                    })
                    .collect(),
            ),
            other => {
                let i = other.strip_prefix("/songs/")?;
                if i.contains('/') {
                    return None;
                }
                let (r, v) = self
                    .songs
                    .iter()
                    .find(|(_, v)| v.split('/').nth(2) == Some(i))?;
                let name = v.rsplit('/').next().unwrap_or_default();
                Some(vec![EntryDto {
                    name: name.into(),
                    path: v.clone(),
                    is_file: fs.is_file(r),
                }])
            }
        }
    }
}

/// 起動の設定（SRV-11）
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub data: PathBuf,
    pub songs: Vec<PathBuf>,
    pub skin_root: Option<PathBuf>,
    pub tables: Vec<String>,
    pub bind: String,
    pub token: Option<String>,
    pub web: Option<PathBuf>,
}

/// SRV-11: 起動引数と `<data>/config.json`
pub fn parse_args(args: &[String]) -> Result<Config, String> {
    let data = args
        .iter()
        .position(|a| a == "--data")
        .and_then(|i| args.get(i + 1))
        .map_or_else(|| PathBuf::from(DATA_DIR), PathBuf::from);
    let saved = settings_json::JsonSettingsFile {
        path: data.join("config.json"),
    }
    .load()
    .map_err(|e| format!("設定を読めない: {e:?}"))?
    .unwrap_or_default();
    let mut c = Config {
        data,
        songs: saved.songs,
        skin_root: saved.skin_root,
        tables: saved.tables,
        bind: BIND.into(),
        token: None,
        web: None,
    };
    let (mut songs, mut tables) = (vec![], vec![]);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| USAGE.to_string());
        match a.as_str() {
            "--data" => {
                value()?;
            }
            "--songs" => songs.push(PathBuf::from(value()?)),
            "--skin-root" => c.skin_root = Some(PathBuf::from(value()?)),
            "--table" => tables.push(value()?),
            "--bind" => c.bind = value()?,
            "--token" => c.token = Some(value()?),
            "--web" => c.web = Some(PathBuf::from(value()?)),
            _ => return Err(USAGE.into()),
        }
    }
    if !songs.is_empty() {
        c.songs = songs;
    }
    if !tables.is_empty() {
        c.tables = tables;
    }
    if c.web.is_none() && Path::new("web").join("index.html").is_file() {
        c.web = Some(PathBuf::from("web"));
    }
    Ok(c)
}

/// SRV-08: 合言葉
pub fn token(data: &Path, given: Option<String>) -> Result<String, String> {
    if let Some(t) = given {
        return Ok(t);
    }
    let path = data.join("server-token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| format!("乱数を作れない: {e}"))?;
    let t = remote::hex(&b);
    std::fs::create_dir_all(data).map_err(|e| format!("{}: {e}", data.display()))?;
    std::fs::write(&path, &t).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(t)
}

/// 譜面の読み込み（ネイティブ版と同じ）
pub fn chart_loaders() -> Vec<Box<dyn ChartLoader>> {
    vec![
        Box::new(chart_loader::DecoderLoader(
            reprise_chart_bms::BmsDecoder::default(),
        )),
        Box::new(chart_loader::DecoderLoader(
            reprise_chart_bmson::BmsonDecoder,
        )),
    ]
}

/// 配っている曲一覧
struct Published {
    version: u64,
    body: Bytes,
}

/// サーバーが持つもの
pub struct Shared {
    token: String,
    roots: Roots,
    fs: file_system::StdFs,
    base: Mutex<Library>,
    tables: Mutex<Vec<DifficultyTable>>,
    published: RwLock<Published>,
    scores: Option<Mutex<score_db::SqliteScoreDb>>,
    query: Option<song_db::SqliteSongQuery>,
    table_src: Arc<dyn TableSource>,
    table_urls: Vec<String>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    /// 曲一覧を作り直して版を上げる（SRV-04）
    fn publish(&self) -> u64 {
        let lib = lock(&self.base)
            .clone()
            .with_tables(lock(&self.tables).clone());
        let mut p = self.published.write().unwrap_or_else(|e| e.into_inner());
        let version = p.version + 1;
        let library = library_to_dto(&lib, &|x| self.roots.to_virtual(x));
        let body = serde_json::to_vec(&LibraryResponse { version, library }).unwrap_or_default();
        *p = Published {
            version,
            body: body.into(),
        };
        version
    }
    fn version(&self) -> u64 {
        self.published
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .version
    }
    fn set_library(&self, lib: Library) {
        *lock(&self.base) = lib;
        self.publish();
    }
    fn refresh_tables(&self) {
        *lock(&self.tables) =
            application::tables::cached(self.table_src.as_ref(), &self.table_urls);
        self.publish();
    }
}

type St = State<Arc<Shared>>;

fn json_response<T: serde::Serialize>(v: &T) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_vec(v).unwrap_or_default(),
    )
        .into_response()
}

fn bad(msg: impl Into<String>) -> Response {
    (StatusCode::BAD_REQUEST, msg.into()).into_response()
}

/// SRV-03: 合言葉
async fn auth(State(s): St, req: Request, next: Next) -> Response {
    let want = format!("Bearer {}", s.token);
    let got = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    // 長さと中身を比べる（中身は最後まで比べる）
    let same = got.len() == want.len()
        && got
            .bytes()
            .zip(want.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !same {
        return (StatusCode::UNAUTHORIZED, "合言葉が違う").into_response();
    }
    next.run(req).await
}

async fn version(State(s): St) -> Response {
    json_response(&VersionDto {
        version: s.version(),
    })
}

async fn library(State(s): St) -> Response {
    let body = s
        .published
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .body
        .clone();
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// 時間のかかる処理（ファイル・DB）は別のスレッドで
async fn blocking(
    s: Arc<Shared>,
    f: impl FnOnce(&Shared) -> Response + Send + 'static,
) -> Response {
    tokio::task::spawn_blocking(move || f(&s))
        .await
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn list(State(s): St, Query(q): Query<HashMap<String, String>>) -> Response {
    let path = q.get("path").cloned().unwrap_or_default();
    blocking(s, move |s| {
        if let Some(v) = s.roots.list_virtual(&path, &s.fs) {
            return json_response(&v);
        }
        let Some(real) = s.roots.to_real(&path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let base = path.trim_end_matches('/');
        let out: Vec<EntryDto> =
            s.fs.list(&real)
                .into_iter()
                .filter(|e| !bad_part(&e.name))
                .map(|e| EntryDto {
                    path: format!("{base}/{}", e.name),
                    name: e.name,
                    is_file: e.is_file,
                })
                .collect();
        json_response(&out)
    })
    .await
}

async fn file(State(s): St, Query(q): Query<HashMap<String, String>>) -> Response {
    let path = q.get("path").cloned().unwrap_or_default();
    blocking(s, move |s| {
        let Some(real) = s.roots.to_real(&path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        match s.fs.read(&real) {
            Ok(b) => ([(header::CONTENT_TYPE, "application/octet-stream")], b).into_response(),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        }
    })
    .await
}

async fn get_score(State(s): St, Query(q): Query<HashMap<String, String>>) -> Response {
    blocking(s, move |s| {
        let md5 = q.get("md5").and_then(|m| remote::unhex::<16>(m));
        let ruleset = q.get("ruleset").and_then(|r| remote::ruleset_of(r));
        let (Some(md5), Some(ruleset)) = (md5, ruleset) else {
            return bad("md5 と ruleset が要る");
        };
        let chart = domain::chart::ChartHash { md5, sha256: None };
        let found = s
            .scores
            .as_ref()
            .and_then(|db| lock(db).find(&chart, ruleset).ok().flatten());
        json_response(&found.map(|r| score_to_dto(&r)))
    })
    .await
}

async fn all_scores(State(s): St) -> Response {
    blocking(s, |s| {
        let all = s
            .scores
            .as_ref()
            .and_then(|db| lock(db).all().ok())
            .unwrap_or_default();
        json_response(&all.iter().map(score_to_dto).collect::<Vec<_>>())
    })
    .await
}

async fn post_score(State(s): St, body: Bytes) -> Response {
    blocking(s, move |s| {
        let Some(rec) = serde_json::from_slice::<ScoreDto>(&body)
            .ok()
            .and_then(|d| score_from_dto(&d))
        else {
            return bad("記録を読めない");
        };
        let Some(db) = s.scores.as_ref() else {
            return (StatusCode::SERVICE_UNAVAILABLE, "記録を保存できない").into_response();
        };
        match lock(db).save(rec) {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:?}")).into_response(),
        }
    })
    .await
}

async fn query(State(s): St, body: Bytes) -> Response {
    blocking(s, move |s| {
        let Ok(q) = serde_json::from_slice::<QueryDto>(&body) else {
            return bad("SQL を読めない");
        };
        let Some(db) = s.query.as_ref() else {
            return bad("曲の DB を引けない");
        };
        match db.song_paths(&q.sql) {
            Ok(paths) => json_response(&PathsDto {
                paths: paths.iter().filter_map(|p| s.roots.to_virtual(p)).collect(),
            }),
            Err(PortError::Parse(e)) => bad(e),
            Err(e) => bad(format!("{e:?}")),
        }
    })
    .await
}

async fn update_tables(State(s): St) -> Response {
    blocking(s, |s| {
        if application::tables::fetch_all(s.table_src.as_ref(), &s.table_urls) {
            s.refresh_tables();
        }
        json_response(&VersionDto {
            version: s.version(),
        })
    })
    .await
}

/// 道筋（SRV-03..07, 09, 10）
pub fn router(s: Arc<Shared>, web: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/version", get(version))
        .route("/api/library", get(library))
        .route("/api/list", get(list))
        .route("/api/file", get(file))
        .route("/api/scores", get(all_scores))
        .route("/api/score", get(get_score).post(post_score))
        .route("/api/query", post(query))
        .route("/api/tables/update", post(update_tables))
        .route_layer(middleware::from_fn_with_state(s.clone(), auth))
        .with_state(s);
    let app = match web {
        Some(dir) => api.fallback_service(tower_http::services::ServeDir::new(dir)),
        None => api,
    };
    app.layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::cors::CorsLayer::permissive())
}

/// 動いているサーバー
pub struct Running {
    pub addr: SocketAddr,
    pub token: String,
    pub shared: Arc<Shared>,
    thread: std::thread::JoinHandle<()>,
}

impl Running {
    /// 止まるまで待つ
    pub fn join(self) {
        let _ = self.thread.join();
    }
}

/// サーバーを始める（待ち受けを始めてから返す）
pub fn start(c: Config) -> Result<Running, String> {
    let tables = Arc::new(bms_table::HttpTables::new(table_dir(&c.data)));
    start_with(c, tables)
}

/// 難易度表の取り方を替えて始める（テスト用。既定は `<data>/table` に置く HTTP）
pub fn start_with(c: Config, table_src: Arc<dyn TableSource>) -> Result<Running, String> {
    let token = token(&c.data, c.token.clone())?;
    std::fs::create_dir_all(&c.data).map_err(|e| format!("{}: {e}", c.data.display()))?;
    let listener = std::net::TcpListener::bind(&c.bind)
        .map_err(|e| format!("{} で待ち受けられない: {e}", c.bind))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let fs: Arc<dyn FileSystem> = Arc::new(file_system::StdFs);
    let (lib, source) = library_watch::LocalLibrary::start(
        c.data.join("song.db"),
        c.songs.clone(),
        fs,
        chart_loaders,
    )
    .map_err(|e| format!("曲の DB を開けない: {e:?}"))?;
    let scores = score_db::SqliteScoreDb::open(&c.data.join("score.db"))
        .map_err(|e| eprintln!("記録を保存できない: {e:?}"))
        .ok()
        .map(Mutex::new);
    let query =
        song_db::SqliteSongQuery::open_read_only(&c.data.join("song.db"), &c.data.join("score.db"))
            .map_err(|e| eprintln!("カスタムフォルダの SQL を引けない: {e:?}"))
            .ok();
    let tables = application::tables::cached(table_src.as_ref(), &c.tables);
    let shared = Arc::new(Shared {
        token: token.clone(),
        roots: Roots::new(&c.songs, c.skin_root.clone()),
        fs: file_system::StdFs,
        base: Mutex::new(lib),
        tables: Mutex::new(tables),
        published: RwLock::new(Published {
            version: 0,
            body: Bytes::new(),
        }),
        scores,
        query,
        table_src,
        table_urls: c.tables.clone(),
    });
    shared.publish();
    // 見張りの曲一覧を受け取る
    let s = shared.clone();
    std::thread::spawn(move || loop {
        if let Some(l) = source.poll() {
            s.set_library(l);
        }
        std::thread::sleep(Duration::from_millis(100));
    });
    // 手元に無い難易度表を取ってくる
    let s = shared.clone();
    std::thread::spawn(move || {
        if application::tables::fetch_missing(s.table_src.as_ref(), &s.table_urls) {
            s.refresh_tables();
        }
    });
    let app = router(shared.clone(), c.web.clone());
    let thread = std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => return eprintln!("{e}"),
        };
        rt.block_on(async move {
            match tokio::net::TcpListener::from_std(listener) {
                Ok(l) => {
                    if let Err(e) = axum::serve(l, app).await {
                        eprintln!("{e}");
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        });
    });
    Ok(Running {
        addr,
        token,
        shared,
        thread,
    })
}

/// 表の置き場所（`<data>/table`）
pub fn table_dir(data: &Path) -> PathBuf {
    data.join("table")
}

#[cfg(test)]
mod tests;
