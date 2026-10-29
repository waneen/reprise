//! 記法非依存のスキンモデル（LR2 csv / beatoraja json・lua を共通化した、ゲームが使う形）。
//! 「どの条件で・どのタイマーで・何を・どう補間して出すか」の宣言だけを持つ。
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub i32);
/// 表示条件（負値は否定）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionId(pub i32);

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub scene: SceneKind,
    pub resolution: (u32, u32),
    /// 画像（解決できなかったものは None）
    pub images: Vec<Option<PathBuf>>,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneKind {
    Play7,
    Play5,
    Play14,
    Play10,
    Play9,
    Select,
    Decide,
    Result,
    KeyConfig,
    SkinSelect,
    SoundSet,
    Theme,
    Play7Battle,
    Play5Battle,
    Play9Battle,
    CourseResult,
    Other(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub kind: ElementKind,
    pub source: Option<SourceRect>,
    pub timer: TimerId,
    pub conditions: Vec<OptionId>,
    pub looping: Option<i32>,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRect {
    pub image: u32,
    /// x, y, w, h（w/h が負なら画像全体）
    pub rect: [i32; 4],
    pub divide: (u32, u32),
    pub cycle_ms: i32,
    pub timer: TimerId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementKind {
    Image,
    Number {
        value_ref: i32,
        digits: i32,
        align: i32,
    },
    Text {
        value_ref: i32,
        font: u32,
        align: i32,
    },
    Slider {
        value_ref: i32,
        direction: i32,
        range: i32,
    },
    Bar {
        value_ref: i32,
        direction: i32,
    },
    Button {
        action: i32,
    },
    NoteLane {
        lane: i32,
    },
    /// まだ意味付けしていない要素（記法上の名前と添字）
    Other {
        command: String,
        index: i32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    pub t_ms: i32,
    pub dst: [f32; 4],
    pub color: [u8; 4],
    pub angle: f32,
    pub center: u8,
    pub blend: Blend,
    pub accel: i32,
    pub filter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Add,
    Sub,
    Mul,
    Sub2,
    InvDest,
    InvSrc,
    MulAlpha,
}
