//! ゲーム内の譜面モデル（確定済み）。記法の読み込み・#RANDOM解決は外側
//! （汎用crate reprise-chart-core + 腐敗防止層 adapters/chart-loader）で行い、ここには結果だけが入る。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChartHash {
    pub md5: [u8; 16],
    pub sha256: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    K5,
    K7,
    K9,
    K10,
    K14,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lane(pub u8);

/// 拍位置（小節基準の有理数に近い精度が要るので固定小数）
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Beat(pub f64);

#[derive(Debug, Clone, PartialEq)]
pub enum NoteKind {
    Normal,
    Long { end: Beat },
    Mine { damage: f64 },
    Invisible,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub beat: Beat,
    pub lane: Lane,
    pub kind: NoteKind,
    pub sound: Option<SoundId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum TimingEvent {
    Bpm { beat: Beat, bpm: f64 },
    Stop { beat: Beat, beats: f64 },
    Measure { index: u32, length: f64 },
}

#[derive(Debug, Clone, Default)]
pub struct ChartMeta {
    pub title: String,
    pub artist: String,
    pub level: Option<i32>,
    pub total: Option<f64>,
    pub judge_rank: Option<i32>,
}

/// 確定した譜面（プレイ・計算はすべてこれを対象にする）
#[derive(Debug, Clone)]
pub struct Chart {
    pub hash: ChartHash,
    pub meta: ChartMeta,
    pub key_mode: KeyMode,
    pub notes: Vec<Note>,
    pub bgm: Vec<(Beat, SoundId)>,
    pub timing: Vec<TimingEvent>,
}
