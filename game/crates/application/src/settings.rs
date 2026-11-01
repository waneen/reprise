//! 設定（起動のたびに読み、キー設定を変えたら保存する）。起動引数があればそちらを優先する
use crate::play_options::PlayOptions;
use domain::input::Bindings;
use domain::play::GaugeType;
use domain::select::ListFilter;
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
    /// 選曲の並べ替え・絞り込み（LR2 の `config.select` の `sort`・`key`・`difficulty`）
    pub select: ListFilter,
    /// 選曲のプレビュー音（LR2 の `config.select.preview`。既定は鳴らす）
    pub preview: bool,
    /// 難易度表の URL（beatoraja と同じく HTML の `bmstable` かヘッダの JSON。reprise の拡張）
    pub tables: Vec<String>,
    /// 画面（LR2 の `config.system` の `screenmode`・`windowsize_x`・`windowsize_y`・`vsync`）
    pub screen: Screen,
    /// 音量（0..=100。LR2 の `config.sound.volumemaster`）
    pub volume: i32,
    /// つなぐ曲サーバー（ブラウザ版。reprise の拡張。URL が空ならつながない）
    pub server: ServerAddress,
}

/// 画面の出し方（LR2 の `screenmode`: 0 = フルスクリーン、1 = ウィンドウ）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenMode {
    #[default]
    Window,
    /// LR2 と同じくデスクトップの解像度のまま画面全体に出す（`DX_FSRESOLUTIONMODE_DESKTOP`）
    Fullscreen,
}

/// 画面
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub mode: ScreenMode,
    /// ウィンドウの大きさ（ピクセル）
    pub width: u32,
    pub height: u32,
    /// 垂直同期（LR2 の既定は切）
    pub vsync: bool,
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            mode: ScreenMode::Window,
            width: 1280,
            height: 720,
            vsync: false,
        }
    }
}

/// 曲サーバーの URL と合言葉
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerAddress {
    pub url: String,
    pub token: String,
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
            select: ListFilter::default(),
            preview: true,
            tables: vec![],
            screen: Screen::default(),
            volume: 100,
            server: ServerAddress::default(),
        }
    }
}
