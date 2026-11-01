//! ネイティブとブラウザで違うところ（起動引数・ファイル・記録と設定の保存先・乱数の種）。
//! ブラウザ版は、ページの JavaScript が選ばれたファイルを `globalThis.repriseFiles`（パス → `Uint8Array` の `Map`）に、
//! 起動引数を `globalThis.repriseArgs`（文字列の配列）に置いてから起動する（`web/index.html`）。
use application::ports::{FileSystem, ScoreRepository, SettingsStore};
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
    /// ブラウザ版は記録を保存しない
    pub fn open_scores(_data_dir: &Path) -> Option<Scores> {
        None
    }
    /// ブラウザ版は設定を保存しない
    pub fn settings_store(_data_dir: &Path) -> Option<Store> {
        None
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
