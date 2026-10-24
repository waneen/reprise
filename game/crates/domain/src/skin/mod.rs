//! 記法非依存のスキンモデル（LR2 csv / beatoraja json・lua を共通化）。
//! 「どの条件で・どのタイマーで・何を・どう補間して出すか」の宣言だけを持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionId(pub i32);

#[derive(Debug, Clone)]
pub struct Skin {
    pub scene: SceneKind,
    pub resolution: (u32, u32),
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneKind {
    Select,
    Decide,
    Play7,
    Play5,
    Play14,
    Play10,
    Play9,
    Result,
    CourseResult,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub timer: Option<TimerId>,
    pub conditions: Vec<OptionId>,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone)]
pub enum ElementKind {
    Image { source: u32, src_rect: [i32; 4] },
    Number { value_ref: u32 },
    Text { text_ref: u32 },
    Slider,
    Bar,
    NoteLane { lane: u8 },
}

#[derive(Debug, Clone)]
pub struct Keyframe {
    pub t_ms: i32,
    pub dst: [f32; 4],
    pub color: [u8; 4],
    pub angle: f32,
    pub blend: Blend,
    pub accel: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum Blend {
    Alpha,
    Add,
    Sub,
    Mul,
}
