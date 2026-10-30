//! 設定（起動のたびに読み、キー設定を変えたら保存する）。起動引数があればそちらを優先する
use crate::play_options::PlayOptions;
use domain::input::Bindings;
use domain::play::GaugeType;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// 選曲に出す譜面のフォルダ
    pub songs: Vec<PathBuf>,
    /// プレイスキン
    pub skin: Option<PathBuf>,
    /// 選曲スキン（無ければ文字の一覧）
    pub select_skin: Option<PathBuf>,
    /// スキン中のパスの基準（LR2 本体のフォルダ）
    pub skin_root: Option<PathBuf>,
    pub gauge: GaugeType,
    /// プレイオプション（オートプレイは保存しない）
    pub options: PlayOptions,
    pub bindings: Bindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            songs: vec![],
            skin: None,
            select_skin: None,
            skin_root: None,
            gauge: GaugeType::Groove,
            options: PlayOptions::default(),
            bindings: Bindings::default(),
        }
    }
}
