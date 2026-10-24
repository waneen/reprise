//! 記法共通のスキンモデルとtrait。描画APIにもゲームの状態にも依存しない。
//! タイマー・数値・条件は「番号」で参照し、その意味付け（LR2のどの番号が何か）は利用側の仕事。
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerRef(pub i32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueRef(pub i32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConditionRef(pub i32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageRef(pub u32);

#[derive(Debug, Clone)]
pub struct SkinDocument {
    pub header: SkinHeader,
    pub images: Vec<(ImageRef, PathBuf)>,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Default)]
pub struct SkinHeader {
    pub title: String,
    pub kind: String,
    pub resolution: (u32, u32),
    pub options: Vec<CustomOption>,
}

/// スキン設定項目（LR2 #CUSTOMOPTION / beatoraja property）
#[derive(Debug, Clone)]
pub struct CustomOption {
    pub name: String,
    pub choices: Vec<(String, ConditionRef)>,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub source: Option<Source>,
    pub timer: Option<TimerRef>,
    /// すべて満たすと表示（負値=否定 はLR2流。意味付けは利用側）
    pub conditions: Vec<ConditionRef>,
    pub keyframes: Vec<Keyframe>,
    pub looping: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub image: ImageRef,
    pub rect: [i32; 4],
    pub divide: (u32, u32),
    pub cycle_ms: i32,
}

#[derive(Debug, Clone)]
pub enum ElementKind {
    Image,
    Number {
        value: ValueRef,
        digits: u8,
        align: i8,
    },
    Text {
        value: ValueRef,
        font: u32,
        align: i8,
    },
    Slider {
        value: ValueRef,
        direction: u8,
        range: i32,
    },
    Bargraph {
        value: ValueRef,
        direction: u8,
    },
    Button {
        action: i32,
    },
    /// プレイ画面固有（ノーツ・判定ライン等）は名前付きで逃がし、利用側が解釈
    Special(String),
}

#[derive(Debug, Clone)]
pub struct Keyframe {
    pub time_ms: i32,
    pub rect: [f32; 4],
    pub argb: [u8; 4],
    pub angle: f32,
    pub blend: Blend,
    pub filter: bool,
    pub accel: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Alpha,
    Add,
    Subtract,
    Multiply,
    Xor,
    Invert,
}

/// スキンがファイル選択時に乱数・ワイルドカード（#IMAGE xxx/*.png 等）を使うので
/// ファイル解決を利用側に委ねる（LR2互換の乱数もここで注入できる）
pub trait FileResolver {
    fn resolve(&mut self, base: &Path, pattern: &str) -> Option<PathBuf>;
}

/// 選択中のスキン設定（どの #CUSTOMOPTION を選んだか）
#[derive(Debug, Clone, Default)]
pub struct SkinSettings {
    pub selected: Vec<ConditionRef>,
    pub files: Vec<(String, String)>,
}

#[derive(Debug)]
pub enum SkinError {
    Io(String),
    Syntax {
        file: PathBuf,
        line: usize,
        message: String,
    },
    Unsupported(String),
}

pub trait SkinDecoder {
    fn name(&self) -> &'static str;
    fn can_decode(&self, path: &Path) -> bool;
    /// ヘッダだけ読む（スキン選択画面用）
    fn read_header(&self, path: &Path) -> Result<SkinHeader, SkinError>;
    fn decode(
        &self,
        path: &Path,
        settings: &SkinSettings,
        files: &mut dyn FileResolver,
    ) -> Result<SkinDocument, SkinError>;
}
