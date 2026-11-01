pub use crate::library::{FolderEntry, SongEntry};
use domain::{
    chart::{Chart, ChartHash},
    play::RulesetId,
    random::LegacyMt,
    score::ScoreRecord,
    skin::Skin,
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum PortError {
    Unsupported,
    Parse(String),
    Io(String),
}

/// ① 譜面読み込み。#RANDOM解決にゲーム側の乱数（LR2互換MT）を使わせるため rng を渡す。
pub trait ChartLoader {
    fn can_load(&self, path: &Path) -> bool;
    fn load(&self, path: &Path, bytes: &[u8], rng: &mut LegacyMt) -> Result<Chart, PortError>;
}

/// ① スキン読み込み
pub trait SkinLoader {
    fn can_load(&self, path: &Path) -> bool;
    fn load(&self, path: &Path, rng: &mut LegacyMt) -> Result<Skin, PortError>;
}

/// ② 他プレイヤーからの記録取り込み
pub trait ScoreImporter {
    fn source_name(&self) -> &'static str;
    fn import(&self, source: &Path) -> Result<Vec<ScoreRecord>, PortError>;
}

/// 記録の保存先（同じ譜面・同じルールにつき 1 件）
pub trait ScoreRepository {
    fn find(&self, chart: &ChartHash, ruleset: RulesetId)
        -> Result<Option<ScoreRecord>, PortError>;
    fn save(&mut self, record: ScoreRecord) -> Result<(), PortError>;
}

/// フォルダの中身の 1 つ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_file: bool,
}

/// ファイルの読み出し（ネイティブはディスク、ブラウザは選ばれたファイルを置いたメモリ）
pub trait FileSystem: Send + Sync {
    /// フォルダの中身（フォルダでなければ空）
    fn list(&self, dir: &Path) -> Vec<DirEntry>;
    fn read(&self, path: &Path) -> Result<Vec<u8>, PortError>;
    fn is_file(&self, path: &Path) -> bool;
    /// ファイルかフォルダの更新日時（1970 年からの秒）。無ければ None（既定はファイルなら 0）
    fn modified(&self, path: &Path) -> Option<i64> {
        self.is_file(path).then_some(0)
    }
}

/// 設定の保存先
pub trait SettingsStore {
    /// 保存した設定（まだ無ければ None）
    fn load(&self) -> Result<Option<crate::settings::Settings>, PortError>;
    fn save(&self, settings: &crate::settings::Settings) -> Result<(), PortError>;
}

/// 曲の DB（LR2 の `song.db` の `folder`・`song` テーブル。ネイティブ版だけ）。
/// 行はパスで決まる。足す・消すの規則は [`crate::library`]
pub trait SongDb: Send {
    fn folders(&self) -> Result<Vec<FolderEntry>, PortError>;
    fn songs(&self) -> Result<Vec<SongEntry>, PortError>;
    /// 譜面の行の更新日時（行が無ければ None）
    fn song_date(&self, path: &Path) -> Result<Option<i64>, PortError>;
    /// フォルダの行が無ければ足して true、あれば何もせず false（LR2 の `INSERT` が主キーで失敗するのと同じ）。`now` は追加日時
    fn insert_folder(&mut self, folder: &FolderEntry, now: i64) -> Result<bool, PortError>;
    /// フォルダの更新日時を書き換える（LR2 は追加日時も今にする）
    fn touch_folder(&mut self, path: &Path, date: i64, now: i64) -> Result<(), PortError>;
    fn delete_folder(&mut self, path: &Path) -> Result<(), PortError>;
    /// 譜面の行を置く（同じパスがあれば置き換える）。`now` は追加日時
    fn put_song(&mut self, song: &SongEntry, now: i64) -> Result<(), PortError>;
    fn delete_song(&mut self, path: &Path) -> Result<(), PortError>;
    fn set_difficulty(&mut self, path: &Path, difficulty: i32) -> Result<(), PortError>;
    /// すべての行を消す（作り直し）
    fn clear(&mut self) -> Result<(), PortError>;
    fn begin(&mut self) -> Result<(), PortError> {
        Ok(())
    }
    fn commit(&mut self) -> Result<(), PortError> {
        Ok(())
    }
}

/// 曲の DB を SQL で引く（カスタムフォルダ。SQL の組み立ては [`crate::custom_folder`]）
pub trait SongQuery: Send + Sync {
    /// `SELECT * FROM song LEFT JOIN score …` の行の `song.path`（引いた順）。`score` は自分の記録（LR2 のルール）を
    /// LR2 の `score` テーブルと同じ列にしたもの
    fn song_paths(&self, sql: &str) -> Result<Vec<PathBuf>, PortError>;
}

/// 難易度表（beatoraja の難易度表。reprise の拡張）
pub trait TableSource: Send + Sync {
    /// 手元に保存した表（無ければ None）
    fn cached(&self, url: &str) -> Option<domain::select::DifficultyTable>;
    /// 表を取ってきて手元に保存する
    fn fetch(&self, url: &str) -> Result<domain::select::DifficultyTable, PortError>;
}

/// 曲一覧の届け先（ネイティブ版は曲の DB とフォルダの見張り）
pub trait LibrarySource: Send + Sync {
    /// 前に呼んでから新しい曲一覧ができていれば、いちばん新しいものを返す
    fn poll(&self) -> Option<domain::select::Library>;
}

pub trait Clock {
    fn now_us(&self) -> i64;
}
pub trait AudioOutput {
    fn play(&mut self, sound: domain::chart::SoundId, at_us: i64);
}
