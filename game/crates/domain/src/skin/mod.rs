//! 記法非依存のスキンモデル（LR2 csv / beatoraja json・lua を共通化した、ゲームが使う形）。
//! 「どの条件で・どのタイマーで・何を・どう補間して出すか」の宣言だけを持つ。
use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod lr2;
#[cfg(test)]
mod tests;

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
    /// 読み込み終了時点で真の条件（スキン設定の選択など。描画時の条件判定にも使う）
    pub options: Vec<i32>,
    /// 抜き色（アルファの無い画像でこの色を透明にする）
    pub color_key: Option<[u8; 3]>,
    pub timing: SceneTiming,
    /// システムフォント（LR2 の `#FONT`。使えないものは None）
    pub fonts: Vec<Option<SystemFont>>,
    /// 画像フォント（LR2 の `#LR2FONT`。`fonts` とは別の番号。読めなかったものは None）
    pub image_fonts: Vec<Option<ImageFont>>,
    /// 選曲スキンの曲バーの配置
    pub bars: BarLayout,
}

/// 選曲スキンの曲バーの配置（LR2 `#BAR_CENTER`・`#BAR_AVAILABLE`。無ければ 0）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BarLayout {
    /// 選んでいるバーを置くスロット（0..=29）
    pub center: i32,
    /// マウスで選べるスロットの範囲（両端を含む）
    pub available: (i32, i32),
}

/// システムフォント（LR2 `#FONT,大きさ,太さ,種類`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemFont {
    pub size: i32,
    pub thickness: i32,
    pub kind: i32,
}

/// 画像フォント（LR2 の `.lr2font`）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageFont {
    /// 文字の基準の高さ
    pub size: i32,
    /// 文字の間隔
    pub margin: i32,
    pub glyphs: BTreeMap<char, FontGlyph>,
}

/// 画像フォントの 1 文字
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontGlyph {
    /// [`Skin::images`] の添字
    pub image: u32,
    /// x, y, w, h
    pub rect: [i32; 4],
}

/// 場面の時間（ms、場面の開始から）。LR2 の `#STARTINPUT` `#LOADSTART` `#LOADEND` `#PLAYSTART` `#FADEOUT` `#CLOSE`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneTiming {
    /// これを過ぎると入力を受け付ける（タイマー 1）
    pub start_input: i32,
    /// リザルト: ランク表示（タイマー 151）からこれを過ぎると次の入力で記録更新の表示（タイマー 152）へ進む（`#STARTINPUT` の第 2 引数）
    pub start_input_rank: i32,
    /// リザルト: 記録更新の表示（タイマー 152）からこれを過ぎると次の入力で閉じ始める（`#STARTINPUT` の第 3 引数）
    pub start_input_update: i32,
    pub load_start: i32,
    /// 読み込み開始 + これ でタイマー 40（準備完了）
    pub load_end: i32,
    /// タイマー 40 から これ でタイマー 41（プレイ開始）
    pub play_start: i32,
    /// 曲が終わってから（タイマー 2）場面を閉じるまで
    pub fade_out: i32,
    /// 途中終了から（タイマー 3）場面を閉じるまで
    pub close: i32,
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
    /// 条件以外の DST 引数（LR2 の第 21, 22 引数。第 21 引数 1/2 は 1P/2P の皿の角度で回す）
    pub dst_extra: [i32; 2],
    /// 描画順（小さいほど奥。同じなら並び順）
    pub order: u32,
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
    /// 種別ごとの追加引数（LR2 の SRC 第 11..15 引数）
    pub params: [i32; 5],
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
    /// クリックの指定（クリック可・パネル・向き）は `source` の追加引数 2..4 にある（LR2 の SRC 第 12..14 引数）
    Button {
        action: i32,
    },
    /// 選曲の曲バーの題名（LR2 `#SRC_BAR_TITLE`。0 = 通常、1 = 新着）
    BarTitle {
        index: i32,
        font: u32,
        align: i32,
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
