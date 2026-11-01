//! ネイティブとブラウザで違うところ（起動引数・ファイル・曲一覧・記録と設定の保存先・乱数の種）。
//! 曲一覧はネイティブ版は `--data` のフォルダの `song.db`（LR2 の song.db と同じテーブル）から読み、フォルダを見張って作り直す。
//! ブラウザ版は起動のたびに渡されたファイルから作る。
//! ブラウザ版は、ページの JavaScript が選ばれたファイルを `globalThis.repriseFiles`（パス → `Uint8Array` の `Map`）に、
//! 起動引数（`--songs` と `--skin-root`）を `globalThis.repriseArgs`（文字列の配列）に置いてから起動する（`web/index.html`）。
//! 設定はネイティブと同じ `config.json` の中身を、ブラウザ版はページの保存領域（`localStorage`）に置く。
//! ブラウザ版は曲サーバー（`reprise-server`）にもつなげる: ランチャーで URL と合言葉を入れて「開始」すると（[`connect`]）
//! `globalThis.repriseServer`（`{ url, token }`）を置き、ページの `globalThis.repriseHttp(method, target, body)`
//! （同期の HTTP。`{ status, body: Uint8Array }` を返す）でファイル・曲一覧・記録・カスタムフォルダの SQL をサーバーに頼む
//! （`remote` クレートのポートの実装。ネイティブ版と同じ画面）。
use application::ports::{
    ChartLoader, FileSystem, LibrarySource, ScoreRepository, SettingsStore, SongQuery, TableSource,
};
use application::settings::ServerAddress;
use domain::select::Library;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 記録の保存先（sqlite の接続は Sync でないので呼び出し側で Mutex に入れる）
pub type Scores = Box<dyn ScoreRepository + Send>;
/// 設定の保存先
pub type Store = Box<dyn SettingsStore + Send + Sync>;

/// 曲サーバーにつなげなかった（ネイティブ版はつながないので作らない）
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// URL が `http://`・`https://` で始まらない
    BadUrl,
    /// 合言葉が違う（401）
    WrongToken,
    /// つながらない
    Unreachable,
    /// ほかの答え（HTTP の状態）
    Status(u16),
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use application::ports::ScoreImporter;

    pub fn args() -> Vec<String> {
        std::env::args().skip(1).collect()
    }
    /// ブラウザ版か（ネイティブ版は曲サーバーにつながず、同じポートをこの機械の中で使う）
    pub const BROWSER: bool = false;
    /// 曲サーバーにつなぐ（ネイティブ版はつながない）
    pub fn connect(_server: &ServerAddress) -> Result<(), ConnectError> {
        Ok(())
    }
    /// ページが渡した曲サーバー（ブラウザ版だけ）
    pub fn server_hint() -> Option<ServerAddress> {
        None
    }
    /// 難易度表をすべて取り直す（APP-TBL-03）。取れたものがあれば true
    pub fn update_tables(data_dir: &Path, urls: &[String]) -> bool {
        let src = bms_table::HttpTables::new(data_dir.join("table"));
        application::tables::fetch_all(&src, urls)
    }
    /// ランチャーの日本語の字（OS のフォントから、よく入っている日本語のフォントを名前で探す。(中身, コレクションの中の番号)）
    pub fn japanese_font() -> Option<(Vec<u8>, u32)> {
        use fontique::{Collection, CollectionOptions};
        const NAMES: [&str; 13] = [
            "Yu Gothic UI",
            "Yu Gothic",
            "Meiryo UI",
            "Meiryo",
            "MS UI Gothic",
            "Hiragino Sans",
            "Hiragino Kaku Gothic ProN",
            "Noto Sans CJK JP",
            "Noto Sans JP",
            "Source Han Sans JP",
            "IPAexGothic",
            "IPAGothic",
            "VL Gothic",
        ];
        let mut c = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: true,
        });
        NAMES.iter().find_map(|n| {
            let id = c.family_id(n)?;
            let f = c.family(id)?;
            // 標準の太さに近いもの
            let font = f
                .fonts()
                .iter()
                .min_by_key(|f| (f.weight().value() - 400.0).abs() as i32)?;
            let blob = font.load(None)?;
            Some((blob.as_ref().to_vec(), font.index()))
        })
    }
    pub fn file_system() -> Arc<dyn FileSystem> {
        Arc::new(file_system::StdFs)
    }
    pub fn default_skin_root() -> PathBuf {
        std::env::current_dir().unwrap_or_default()
    }
    /// 曲一覧: `song.db` の今の中身と、確かめ・見張りの届け先（DB を開けなければその場で読んだ一覧だけ）。
    /// `rebuild` なら DB を作り直す（ランチャーで選んだとき）
    pub fn library(
        data_dir: &Path,
        roots: &[PathBuf],
        fs: Arc<dyn FileSystem>,
        loaders: fn() -> Vec<Box<dyn ChartLoader>>,
        rebuild: bool,
    ) -> (Library, Option<Box<dyn LibrarySource>>) {
        match library_watch::LocalLibrary::start(
            data_dir.join("song.db"),
            roots.to_vec(),
            fs.clone(),
            loaders,
        ) {
            Ok((lib, source)) => {
                if rebuild {
                    source.rebuild();
                }
                (lib, Some(Box::new(source)))
            }
            Err(e) => {
                eprintln!("曲の DB を開けないので、フォルダを読んだ一覧だけで出す: {e:?}");
                let lib = application::usecase::scan_library(fs.as_ref(), &loaders(), roots);
                (lib, None)
            }
        }
    }
    pub fn open_scores(data_dir: &Path) -> Option<Scores> {
        match score_db::SqliteScoreDb::open(&data_dir.join("score.db")) {
            Ok(db) => Some(Box::new(db)),
            Err(e) => {
                eprintln!("記録を保存できない: {e:?}");
                None
            }
        }
    }
    /// カスタムフォルダの SQL を引く接続（`song.db` に `score.db` を付けたもの。記録の DB を開いた後に呼ぶ）
    pub fn song_query(data_dir: &Path) -> Option<Box<dyn SongQuery>> {
        match song_db::SqliteSongQuery::open(&data_dir.join("song.db"), &data_dir.join("score.db"))
        {
            Ok(q) => Some(Box::new(q)),
            Err(e) => {
                eprintln!("カスタムフォルダの SQL を引けない: {e:?}");
                None
            }
        }
    }
    /// 難易度表の取り方と置き場所（`--data` のフォルダの `table`）
    pub fn table_source(data_dir: &Path) -> Option<Arc<dyn TableSource>> {
        Some(Arc::new(bms_table::HttpTables::new(data_dir.join("table"))))
    }
    /// 設定は記録と同じフォルダの config.json
    pub fn settings_store(data_dir: &Path) -> Option<Store> {
        Some(Box::new(settings_json::JsonSettingsFile {
            path: data_dir.join("config.json"),
        }))
    }
    /// 今の時刻（1970 年からの秒）
    pub fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    }
    pub fn seed() -> u32 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u32)
            .unwrap_or(0)
    }
    /// 起動できないことを伝えて終わる
    pub fn fatal(msg: &str, code: i32) -> ! {
        eprintln!("{msg}");
        std::process::exit(code);
    }
    /// 記録の取り込み（まだ画面からは使わない。登録だけしておく）
    pub fn score_importers() -> Vec<Box<dyn ScoreImporter>> {
        vec![
            Box::new(score_import_lr2::Lr2ScoreDb),
            Box::new(score_import_beatoraja::BeatorajaScoreDb::beatoraja()),
            Box::new(score_import_beatoraja::BeatorajaScoreDb::lr2oraja()),
        ]
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use application::ports::PortError;
    use file_system::MemFs;
    use remote::{Remote, RemoteFs, RemoteLibrary, RemoteScores, RemoteSongQuery, Transport};
    use std::sync::OnceLock;
    use wasm_bindgen::{JsCast, JsValue};

    fn global(name: &str) -> JsValue {
        js_sys::Reflect::get(&js_sys::global(), &name.into()).unwrap_or_default()
    }

    /// ページの `repriseHttp` で送る（同期の XMLHttpRequest。合言葉はページが付ける）
    struct PageHttp;

    impl Transport for PageHttp {
        fn request(&self, method: &str, target: &str, body: Option<&[u8]>) -> (u16, Vec<u8>) {
            let Ok(f) = global("repriseHttp").dyn_into::<js_sys::Function>() else {
                return (0, vec![]);
            };
            let body = body.map_or(JsValue::NULL, |b| js_sys::Uint8Array::from(b).into());
            let Ok(r) = f.call3(&JsValue::NULL, &method.into(), &target.into(), &body) else {
                return (0, vec![]);
            };
            let get = |k: &str| js_sys::Reflect::get(&r, &k.into()).unwrap_or_default();
            let status = get("status").as_f64().unwrap_or(0.0) as u16;
            let bytes = get("body")
                .dyn_into::<js_sys::Uint8Array>()
                .map(|a| a.to_vec())
                .unwrap_or_default();
            (status, bytes)
        }
    }

    /// 曲サーバー（`repriseServer` を置いたときだけ）
    fn remote() -> Option<Arc<Remote>> {
        static REMOTE: OnceLock<Arc<Remote>> = OnceLock::new();
        let s = global("repriseServer");
        (!s.is_undefined() && !s.is_null())
            .then(|| REMOTE.get_or_init(|| Remote::new(PageHttp)).clone())
    }

    fn set_global(name: &str, v: &JsValue) {
        let _ = js_sys::Reflect::set(&js_sys::global(), &name.into(), v);
    }

    /// ブラウザ版か（ランチャーで譜面のフォルダの代わりに曲サーバーを選ぶ）
    pub const BROWSER: bool = true;

    /// 曲サーバーにつなぐ（URL が空ならつながない）。`GET /api/version` で確かめる
    pub fn connect(server: &ServerAddress) -> Result<(), ConnectError> {
        let url = match application::launcher::server_url(&server.url) {
            Ok(Some(u)) => u,
            Ok(None) => {
                set_global("repriseServer", &JsValue::UNDEFINED);
                return Ok(());
            }
            Err(_) => return Err(ConnectError::BadUrl),
        };
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"url".into(), &url.into());
        let _ = js_sys::Reflect::set(&o, &"token".into(), &server.token.as_str().into());
        set_global("repriseServer", &o);
        let (status, _) = PageHttp.request("GET", "/api/version", None);
        let err = match status {
            200..=299 => return Ok(()),
            0 => ConnectError::Unreachable,
            401 => ConnectError::WrongToken,
            s => ConnectError::Status(s),
        };
        set_global("repriseServer", &JsValue::UNDEFINED);
        Err(err)
    }
    /// ページが渡した曲サーバー（`globalThis.repriseServerHint`。サーバーが配ったページの URL と `#token=`）
    pub fn server_hint() -> Option<ServerAddress> {
        let h = global("repriseServerHint");
        let get = |k: &str| js_sys::Reflect::get(&h, &k.into()).ok()?.as_string();
        (!h.is_undefined() && !h.is_null()).then(|| ServerAddress {
            url: get("url").unwrap_or_default(),
            token: get("token").unwrap_or_default(),
        })
    }
    /// 難易度表をすべて取り直す（曲サーバーに頼む。`POST /api/tables/update`）
    pub fn update_tables(_data_dir: &Path, _urls: &[String]) -> bool {
        remote().is_some_and(|r| r.update_tables().is_ok())
    }
    /// ブラウザ版は OS のフォントを読めない（ランチャーは英語で出す）
    pub fn japanese_font() -> Option<(Vec<u8>, u32)> {
        None
    }

    pub fn args() -> Vec<String> {
        global("repriseArgs")
            .dyn_into::<js_sys::Array>()
            .map(|a| a.iter().filter_map(|v| v.as_string()).collect())
            .unwrap_or_default()
    }
    pub fn file_system() -> Arc<dyn FileSystem> {
        if let Some(r) = remote() {
            return Arc::new(RemoteFs(r));
        }
        let mut fs = MemFs::new();
        if let Ok(map) = global("repriseFiles").dyn_into::<js_sys::Map>() {
            map.for_each(&mut |bytes, path| {
                if let (Some(path), Ok(bytes)) =
                    (path.as_string(), bytes.dyn_into::<js_sys::Uint8Array>())
                {
                    fs.insert(path, bytes.to_vec());
                }
            });
        }
        Arc::new(fs)
    }
    pub fn default_skin_root() -> PathBuf {
        // 曲サーバーは LR2 本体のフォルダを `/skin` で配る
        PathBuf::from(if remote().is_some() { "/skin" } else { "/" })
    }
    /// 曲一覧: 曲サーバーにつないだらサーバーの曲一覧（見張りの結果も届く）、それ以外は渡されたファイルから作る
    pub fn library(
        _data_dir: &Path,
        roots: &[PathBuf],
        fs: Arc<dyn FileSystem>,
        loaders: fn() -> Vec<Box<dyn ChartLoader>>,
        _rebuild: bool,
    ) -> (Library, Option<Box<dyn LibrarySource>>) {
        if let Some(r) = remote() {
            return match r.library() {
                Ok((version, lib)) => {
                    let feed = RemoteLibrary::new(r, version, js_sys::Date::now);
                    (lib, Some(Box::new(feed)))
                }
                Err(e) => fatal(&format!("曲サーバーから曲一覧をもらえない: {e:?}"), 1),
            };
        }
        let lib = application::usecase::scan_library(fs.as_ref(), &loaders(), roots);
        (lib, None)
    }
    /// 記録は曲サーバーに預ける（つないでいなければ保存しない）
    pub fn open_scores(_data_dir: &Path) -> Option<Scores> {
        remote().map(|r| Box::new(RemoteScores::new(r)) as Scores)
    }
    /// カスタムフォルダの SQL は曲サーバーが引く（つないでいなければ引けない。曲の DB が無い）
    pub fn song_query(_data_dir: &Path) -> Option<Box<dyn SongQuery>> {
        remote().map(|r| Box::new(RemoteSongQuery(r)) as Box<dyn SongQuery>)
    }
    /// ブラウザ版は難易度表を取ってこない（曲サーバーの曲一覧に入っている）
    pub fn table_source(_data_dir: &Path) -> Option<Arc<dyn TableSource>> {
        None
    }
    /// 設定は `config.json` と同じ中身をページの保存領域（`localStorage`）に置く
    pub fn settings_store(_data_dir: &Path) -> Option<Store> {
        Some(Box::new(PageStorage))
    }

    /// `localStorage` のこの名前に `config.json` の中身を置く
    const SETTINGS_KEY: &str = "reprise/config.json";

    struct PageStorage;

    fn port_error(e: wasm_bindgen::JsValue) -> PortError {
        PortError::Io(format!("localStorage: {e:?}"))
    }

    /// `localStorage.name(args)`（使えなければ Err）
    fn call_storage(
        name: &str,
        args: &[wasm_bindgen::JsValue],
    ) -> Result<wasm_bindgen::JsValue, PortError> {
        let storage =
            js_sys::Reflect::get(&js_sys::global(), &"localStorage".into()).map_err(port_error)?;
        if storage.is_null() || storage.is_undefined() {
            return Err(PortError::Io("localStorage が無い".into()));
        }
        let f: js_sys::Function = js_sys::Reflect::get(&storage, &name.into())
            .map_err(port_error)?
            .dyn_into()
            .map_err(port_error)?;
        f.apply(&storage, &args.iter().collect::<js_sys::Array>())
            .map_err(port_error)
    }

    impl SettingsStore for PageStorage {
        fn load(&self) -> Result<Option<application::settings::Settings>, PortError> {
            match call_storage("getItem", &[SETTINGS_KEY.into()])?.as_string() {
                Some(text) => settings_json::parse(&text).map(Some),
                None => Ok(None),
            }
        }
        fn save(&self, settings: &application::settings::Settings) -> Result<(), PortError> {
            let text = settings_json::render(settings);
            call_storage("setItem", &[SETTINGS_KEY.into(), text.into()]).map(|_| ())
        }
    }
    /// 今の時刻（1970 年からの秒。ブラウザでは `SystemTime` が使えない）
    pub fn unix_now() -> i64 {
        (js_sys::Date::now() / 1000.0) as i64
    }
    pub fn seed() -> u32 {
        js_sys::Date::now() as u64 as u32
    }
    /// 起動できないことをページに例外として伝える
    pub fn fatal(msg: &str, _code: i32) -> ! {
        wasm_bindgen::throw_str(msg)
    }
}

pub use imp::*;
