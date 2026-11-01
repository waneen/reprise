//! ネイティブとブラウザで違うところ（起動引数・ファイル・曲一覧・記録と設定の保存先・乱数の種）。
//! 曲一覧はネイティブ版は `--data` のフォルダの `song.db`（LR2 の song.db と同じテーブル）から読み、フォルダを見張って作り直す。
//! ブラウザ版は起動のたびに渡されたファイルから作る。
//! ブラウザ版は、ページの JavaScript が選ばれたファイルを `globalThis.repriseFiles`（パス → `Uint8Array` の `Map`）に、
//! 起動引数（`--songs` と `--skin-root`）を `globalThis.repriseArgs`（文字列の配列）に置いてから起動する（`web/index.html`）。
//! 設定はネイティブと同じ `config.json` の中身を、ブラウザ版はページの保存領域（`localStorage`）に置く。
use application::ports::{ChartLoader, FileSystem, LibrarySource, ScoreRepository, SettingsStore};
use domain::select::Library;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 記録の保存先（sqlite の接続は Sync でないので呼び出し側で Mutex に入れる）
pub type Scores = Box<dyn ScoreRepository + Send>;
/// 設定の保存先
pub type Store = Box<dyn SettingsStore + Send + Sync>;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use application::ports::ScoreImporter;

    pub fn args() -> Vec<String> {
        std::env::args().skip(1).collect()
    }
    pub fn file_system() -> Arc<dyn FileSystem> {
        Arc::new(file_system::StdFs)
    }
    pub fn default_skin_root() -> PathBuf {
        std::env::current_dir().unwrap_or_default()
    }
    /// 曲一覧: `song.db` の今の中身と、確かめ・見張りの届け先（DB を開けなければその場で読んだ一覧だけ）
    pub fn library(
        data_dir: &Path,
        roots: &[PathBuf],
        fs: Arc<dyn FileSystem>,
        loaders: fn() -> Vec<Box<dyn ChartLoader>>,
    ) -> (Library, Option<Box<dyn LibrarySource>>) {
        match library_watch::LocalLibrary::start(
            data_dir.join("song.db"),
            roots.to_vec(),
            fs.clone(),
            loaders,
        ) {
            Ok((lib, source)) => (lib, Some(Box::new(source))),
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
    /// 設定は記録と同じフォルダの config.json
    pub fn settings_store(data_dir: &Path) -> Option<Store> {
        Some(Box::new(settings_json::JsonSettingsFile {
            path: data_dir.join("config.json"),
        }))
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
    use wasm_bindgen::JsCast;

    fn global(name: &str) -> wasm_bindgen::JsValue {
        js_sys::Reflect::get(&js_sys::global(), &name.into()).unwrap_or_default()
    }

    pub fn args() -> Vec<String> {
        global("repriseArgs")
            .dyn_into::<js_sys::Array>()
            .map(|a| a.iter().filter_map(|v| v.as_string()).collect())
            .unwrap_or_default()
    }
    pub fn file_system() -> Arc<dyn FileSystem> {
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
        PathBuf::from("/")
    }
    /// 曲一覧: 渡されたファイルから作る（DB も見張りも無い）
    pub fn library(
        _data_dir: &Path,
        roots: &[PathBuf],
        fs: Arc<dyn FileSystem>,
        loaders: fn() -> Vec<Box<dyn ChartLoader>>,
    ) -> (Library, Option<Box<dyn LibrarySource>>) {
        let lib = application::usecase::scan_library(fs.as_ref(), &loaders(), roots);
        (lib, None)
    }
    /// ブラウザ版は記録を保存しない
    pub fn open_scores(_data_dir: &Path) -> Option<Scores> {
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
    pub fn seed() -> u32 {
        js_sys::Date::now() as u64 as u32
    }
    /// 起動できないことをページに例外として伝える
    pub fn fatal(msg: &str, _code: i32) -> ! {
        wasm_bindgen::throw_str(msg)
    }
}

pub use imp::*;
