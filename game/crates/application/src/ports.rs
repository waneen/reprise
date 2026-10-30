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
}

pub trait Clock {
    fn now_us(&self) -> i64;
}
pub trait AudioOutput {
    fn play(&mut self, sound: domain::chart::SoundId, at_us: i64);
}
