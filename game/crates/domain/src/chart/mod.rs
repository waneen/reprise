//! ゲーム内の譜面モデル（確定済み）。記法の読み込み・#RANDOM 解決は外側
//! （汎用 crate reprise-chart-core + 腐敗防止層 adapters/chart-loader）で行い、ここには結果だけが入る。

/// 譜面の同一性（LR2 は MD5、beatoraja は SHA-256 を使う）。
/// LR2 の記録には SHA-256 が無いので、同一性は MD5 だけで判定する（DOM-CHT-01）
#[derive(Debug, Clone, Copy)]
pub struct ChartHash {
    pub md5: [u8; 16],
    pub sha256: Option<[u8; 32]>,
}

impl PartialEq for ChartHash {
    fn eq(&self, other: &Self) -> bool {
        self.md5 == other.md5
    }
}
impl Eq for ChartHash {}
impl std::hash::Hash for ChartHash {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.md5.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::HashSet;

    /// DOM-CHT-01: 同一性は MD5 だけ
    #[rstest]
    #[case::same_md5_without_sha(1, None, 1, Some([9; 32]), true)]
    #[case::same_md5_different_sha(1, Some([8; 32]), 1, Some([9; 32]), true)]
    #[case::different_md5(1, Some([9; 32]), 2, Some([9; 32]), false)]
    fn dom_cht_01_identity_is_md5(
        #[case] a: u8,
        #[case] sa: Option<[u8; 32]>,
        #[case] b: u8,
        #[case] sb: Option<[u8; 32]>,
        #[case] same: bool,
    ) {
        let x = ChartHash {
            md5: [a; 16],
            sha256: sa,
        };
        let y = ChartHash {
            md5: [b; 16],
            sha256: sb,
        };
        assert_eq!(x == y, same);
        assert_eq!(HashSet::from([x]).contains(&y), same);
    }
}

/// 鍵盤数（並びは鍵盤数の小さい順）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    pub subartist: String,
    pub genre: String,
    pub initial_bpm: Option<f64>,
    pub level: Option<i32>,
    /// BMS `#DIFFICULTY`（1..=5。選曲の並びに使う）
    pub difficulty: Option<i32>,
    pub total: Option<f64>,
    /// BMS `#RANK`（0..=3 の段階）
    pub judge_rank: Option<i32>,
    /// 判定幅の百分率（bmson `judge_rank`。100 = 標準）
    pub judge_rank_percent: Option<f64>,
    /// ゲージ増加量の百分率（bmson `total`。100 = 標準）
    pub total_percent: Option<f64>,
    /// 譜面名（bmson `chart_name`）
    pub chart_name: String,
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
