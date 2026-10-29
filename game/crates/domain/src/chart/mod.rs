//! ゲーム内の譜面モデル（確定済み）。記法の読み込み・#RANDOM 解決は外側
//! （汎用 crate reprise-chart-core + 腐敗防止層 adapters/chart-loader）で行い、ここには結果だけが入る。

/// 譜面の同一性（LR2 は MD5、beatoraja は SHA-256 を使う）
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

/// レーン番号。1P: 0=皿, 1..=7=鍵盤。2P: 10=皿, 11..=17=鍵盤。9 鍵: 1..=9。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lane(pub u8);

/// 拍位置（4/4 の 1 小節 = 4 拍）
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Beat(pub f64);

#[derive(Debug, Clone, PartialEq)]
pub enum NoteKind {
    Normal,
    Long {
        end: Beat,
        end_sound: Option<SoundId>,
    },
    /// 生の値（ダメージ量への換算はルール側）
    Mine {
        damage_raw: u32,
    },
    Invisible,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub beat: Beat,
    pub lane: Lane,
    pub kind: NoteKind,
    pub sound: Option<SoundId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoundId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgaLayer {
    Base,
    Poor,
    Layer,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimingEvent {
    Bpm { beat: Beat, bpm: f64 },
    Stop { beat: Beat, beats: f64 },
    Measure { index: u32, length: f64 },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChartMeta {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub initial_bpm: Option<f64>,
    pub level: Option<i32>,
    pub total: Option<f64>,
    pub judge_rank: Option<i32>,
}

/// 確定した譜面（プレイ・計算はすべてこれを対象にする）
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub hash: ChartHash,
    pub meta: ChartMeta,
    pub key_mode: KeyMode,
    /// 拍順
    pub notes: Vec<Note>,
    pub bgm: Vec<(Beat, SoundId)>,
    pub bga: Vec<(Beat, BgaLayer, ImageId)>,
    /// 拍順。Measure は小節番号順
    pub timing: Vec<TimingEvent>,
    pub sounds: Vec<(SoundId, String)>,
    pub images: Vec<(ImageId, String)>,
}
