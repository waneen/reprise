//! 記法共通のスキンモデルと trait。描画 API にもゲームの状態にも依存しない。
//!
//! タイマー・数値・条件は「番号」で参照し、その意味付け（LR2 のどの番号が何か）は利用側の仕事。
//! ファイルの探索・ワイルドカードの乱択・読み込みは [`FileResolver`] として利用側が注入する。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerRef(pub i32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueRef(pub i32);
/// 表示条件（負値は否定。LR2 流）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConditionRef(pub i32);
/// [`SkinDocument::images`] の添字
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageRef(pub u32);
/// [`SkinDocument::fonts`] の添字
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontRef(pub u32);

/// スキンのヘッダ（選択画面で使う情報）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinHeader {
    /// スキン種別番号（LR2: 0=7鍵, 5=選曲, 7=リザルト …）
    pub kind: i32,
    pub title: String,
    pub maker: String,
    pub thumbnail: String,
    pub resolution: (u32, u32),
    /// 出現順（添字が [`SkinSettings::custom`] のキー）
    pub customs: Vec<Custom>,
}

impl Default for SkinHeader {
    fn default() -> Self {
        Self {
            kind: -1,
            title: String::new(),
            maker: String::new(),
            thumbnail: String::new(),
            resolution: (640, 480),
            customs: Vec::new(),
        }
    }
}

/// スキン設定項目
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Custom {
    /// 選択肢 i を選ぶと条件 `start_op + i` が真になる
    Option {
        name: String,
        start_op: i32,
        labels: Vec<String>,
    },
    /// 選択肢ごとに条件番号を持つ（beatoraja `property`）。選んだ項目の条件が真になる
    Choice {
        name: String,
        /// (表示名, 条件番号)
        items: Vec<(String, i32)>,
        /// 既定の項目名
        default: Option<String>,
    },
    /// パターン中の `*` を選んだファイル名で置き換える（拡張子の扱いは記法による）
    File {
        name: String,
        pattern: String,
        default: String,
    },
}

/// 利用者の選択
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomChoice {
    /// 条件番号（`Custom::Option` / `Custom::Choice`）
    Option(i32),
    /// ファイル名（LR2 csv は拡張子無し、beatoraja は拡張子付き）または `"RANDOM"`（beatoraja は `"Random"`）
    File(String),
}

/// 読み込み時の設定
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkinSettings {
    /// 読み込み時点で真の条件番号（ゲーム状態由来のもの）
    pub options: BTreeSet<i32>,
    /// ヘッダの [`SkinHeader::customs`] の添字 → 選択
    pub custom: BTreeMap<usize, CustomChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageEntry {
    File(PathBuf),
    /// 直前の画像を引き継ぐ（LR2 `CONTINUE`）
    Continue,
    /// 見つからなかった（元のパス）
    Missing(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontEntry {
    System {
        size: i32,
        thickness: i32,
        kind: i32,
    },
    Bitmap(PathBuf),
    /// フォントファイル（TTF やビットマップ等。種類の判別は利用側）
    File(PathBuf),
    Missing(String),
}

/// 解釈しない設定行（`#STARTINPUT` など）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinDocument {
    pub header: SkinHeader,
    pub images: Vec<ImageEntry>,
    pub fonts: Vec<FontEntry>,
    pub elements: Vec<Element>,
    pub directives: Vec<Directive>,
    pub diagnostics: Vec<Diagnostic>,
    /// 読み込み終了時点で真の条件（スキン設定の選択などを含む。描画時の条件判定にも使う）
    pub options: BTreeSet<i32>,
}

/// 描画要素（SRC 1 行 + DST 複数行）
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// `#SRC_` の後ろの名前（例: `IMAGE`, `NUMBER`, `NOTE`）
    pub command: String,
    /// SRC の第 1 引数（レーン番号等の添字）
    pub index: i32,
    pub kind: ElementKind,
    pub source: Option<Source>,
    /// 移動用タイマー（最初の DST）
    pub timer: Option<TimerRef>,
    /// 表示条件（最初の DST。すべて満たすと表示）
    pub conditions: Vec<ConditionRef>,
    /// ループ開始時刻（最初の DST）
    pub looping: Option<i32>,
    /// 最初の DST の条件以外の引数（LR2 の DST 第 21, 22 引数。第 21 引数 1/2 は 1P/2P の皿の角度で回す）
    pub dst_extra: [i32; 2],
    /// 描画順（小さいほど奥。同じなら `elements` の並び順）。LR2 は最初の DST 行の行番号（`#IF` で飛ばした行は数えない）
    pub order: u32,
    pub keyframes: Vec<Keyframe>,
}

/// 画像の切り出し
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub image: ImageRef,
    /// x, y, w, h（w/h が負なら画像全体）
    pub rect: [i32; 4],
    pub divide: (u32, u32),
    pub cycle_ms: i32,
    /// アニメーション用タイマー
    pub timer: Option<TimerRef>,
    /// 種別ごとの追加引数（LR2 の SRC 第 11..15 引数）
    pub params: [i32; 5],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementKind {
    Image,
    Number {
        value: ValueRef,
        align: i32,
        digits: i32,
    },
    Text {
        font: FontRef,
        value: ValueRef,
        align: i32,
        editable: bool,
    },
    Slider {
        direction: i32,
        range: i32,
        value: ValueRef,
        disabled: bool,
    },
    Bargraph {
        value: ValueRef,
        direction: i32,
    },
    Button {
        action: i32,
        click: i32,
        panel: i32,
        plus_only: bool,
    },
    /// 上記以外（ノーツ・判定等）。解釈は `command` を見て利用側が行う
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    pub time_ms: i32,
    /// x, y, w, h
    pub rect: [f32; 4],
    pub argb: [u8; 4],
    pub angle: f32,
    /// 回転中心（LR2: 0..=9）
    pub center: u8,
    pub blend: Blend,
    pub filter: bool,
    pub accel: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Add,
    Subtract,
    Multiply,
    /// LR2 の 5
    Subtract2,
    /// LR2 の 9（描画先の色を反転）
    InvertDest,
    /// LR2 の 10
    InvertSrc,
    /// LR2 の 11
    MultiplyAlpha,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkinError {
    Io {
        path: PathBuf,
        message: String,
    },
    NotASkin(PathBuf),
    /// 記法として読めない（行番号は分かる範囲で）
    Syntax {
        path: PathBuf,
        line: usize,
        message: String,
    },
}

/// ファイルの探索と読み込み（利用側が実装する）
pub trait FileResolver {
    /// スキン中のパス表記（`*` を含むことがある）を実在のファイルに解決する。
    /// `*` が複数に一致した場合の選び方（LR2 は乱択）は実装が決める
    fn resolve(&mut self, pattern: &str) -> Option<PathBuf>;
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String>;
}

pub trait SkinDecoder {
    fn name(&self) -> &'static str;
    fn can_decode(&self, path: &Path) -> bool;
    /// ヘッダだけ読む（スキン選択画面用）
    fn read_header(
        &self,
        path: &Path,
        files: &mut dyn FileResolver,
    ) -> Result<SkinHeader, SkinError>;
    fn decode(
        &self,
        path: &Path,
        settings: &SkinSettings,
        files: &mut dyn FileResolver,
    ) -> Result<SkinDocument, SkinError>;
}
