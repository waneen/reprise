//! 設定（起動のたびに読み、キー設定を変えたら保存する）。起動引数があればそちらを優先する
use crate::play_options::PlayOptions;
use domain::input::Bindings;
use domain::play::GaugeType;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// 選曲に出す譜面のフォルダ
    pub songs: Vec<PathBuf>,
    /// プレイスキン
    pub skin: Option<PathBuf>,
    /// 選曲スキン（無ければ文字の一覧）
    pub select_skin: Option<PathBuf>,
    /// キー設定スキン（無ければ選曲スキンの BUTTON 13 を押しても何もしない）
    pub keyconfig_skin: Option<PathBuf>,
    /// リザルトスキン（無ければ結果を文字で出す）
    pub result_skin: Option<PathBuf>,
    /// スキン中のパスの基準（LR2 本体のフォルダ）
    pub skin_root: Option<PathBuf>,
    pub gauge: GaugeType,
    /// プレイオプション（オートプレイは保存しない）
    pub options: PlayOptions,
    pub bindings: Bindings,
    /// スキン設定の選択（スキンのパス → 項目名 → 選んだ名前。LR2 のスキン選択画面で選んで `SkinCustomize` に保存するもの）。
    /// 書いていない項目は既定（先頭）
    pub skin_custom: BTreeMap<PathBuf, BTreeMap<String, String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            songs: vec![],
            skin: None,
            select_skin: None,
            keyconfig_skin: None,
            result_skin: None,
            skin_root: None,
            gauge: GaugeType::Groove,
            options: PlayOptions::default(),
            bindings: Bindings::default(),
            skin_custom: BTreeMap::new(),
        }
    }
}
