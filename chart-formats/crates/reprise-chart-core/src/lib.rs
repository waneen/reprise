//! 記法共通の譜面モデルと、記法実装が満たすtrait。
//! 特定のプレイヤー（LR2/beatoraja）の挙動は持たない。乱数は利用側が注入する。

/// 乱数源。LR2互換が欲しい利用側はDxLib互換MTを、そうでなければ任意の乱数を渡す。
pub trait RandomSource {
    /// 0..=max の一様整数
    fn next_inclusive(&mut self, max: u32) -> u32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyLayout {
    Beat5,
    Beat7,
    Beat10,
    Beat14,
    Popn9,
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lane(pub u8);

/// 拍位置（小節番号 + 小節内の比率）
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Position {
    pub measure: u32,
    pub fraction: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundRef(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum NoteKind {
    Normal,
    Long { end: Position },
    Mine { damage: f64 },
    Invisible,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Note {
        pos: Position,
        lane: Lane,
        kind: NoteKind,
        sound: Option<SoundRef>,
    },
    Bgm {
        pos: Position,
        sound: SoundRef,
    },
    Bpm {
        pos: Position,
        bpm: f64,
    },
    Stop {
        pos: Position,
        duration_192th: f64,
    },
    MeasureLength {
        measure: u32,
        ratio: f64,
    },
    Bga {
        pos: Position,
        layer: u8,
        image: u32,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub initial_bpm: Option<f64>,
    pub level: Option<i32>,
    pub total: Option<f64>,
    pub judge_rank: Option<i32>,
    pub sounds: Vec<(SoundRef, String)>,
    pub images: Vec<(u32, String)>,
}

/// 未解決の譜面（#RANDOM等の分岐を保持）
#[derive(Debug, Clone)]
pub struct ChartDocument {
    pub layout: KeyLayout,
    pub meta: Metadata,
    pub body: Vec<Block>,
}

#[derive(Debug, Clone)]
pub enum Block {
    Events(Vec<Event>),
    /// #RANDOM max { #IF k ... }
    Random {
        max: u32,
        branches: Vec<(u32, Vec<Block>)>,
    },
    /// #SETRANDOM v
    SetRandom {
        value: u32,
        branches: Vec<(u32, Vec<Block>)>,
    },
}

/// 分岐解決済みの譜面
#[derive(Debug, Clone)]
pub struct ResolvedChart {
    pub layout: KeyLayout,
    pub meta: Metadata,
    pub events: Vec<Event>,
}

impl ChartDocument {
    /// 文書順に乱数を消費して分岐を解決（#RANDOM n → next_inclusive(n-1)+1）
    pub fn resolve(self, rng: &mut dyn RandomSource) -> ResolvedChart {
        fn walk(bs: Vec<Block>, rng: &mut dyn RandomSource, out: &mut Vec<Event>) {
            for b in bs {
                match b {
                    Block::Events(es) => out.extend(es),
                    Block::Random { max, branches } => {
                        let v = rng.next_inclusive(max.saturating_sub(1)) + 1;
                        take(branches, v, rng, out);
                    }
                    Block::SetRandom { value, branches } => take(branches, value, rng, out),
                }
            }
        }
        fn take(
            br: Vec<(u32, Vec<Block>)>,
            v: u32,
            rng: &mut dyn RandomSource,
            out: &mut Vec<Event>,
        ) {
            for (k, body) in br {
                if k == v {
                    walk(body, rng, out);
                }
            }
        }
        let mut events = Vec::new();
        walk(self.body, rng, &mut events);
        ResolvedChart {
            layout: self.layout,
            meta: self.meta,
            events,
        }
    }
}

/// 読み込み時の警告（未知コマンド等）。エラーにせず報告だけする
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug)]
pub enum DecodeError {
    Encoding(String),
    Syntax { line: usize, message: String },
    Other(String),
}

/// 記法実装が満たすtrait
pub trait ChartDecoder {
    fn name(&self) -> &'static str;
    /// 拡張子（小文字, ドット無し）
    fn extensions(&self) -> &'static [&'static str];
    fn decode(&self, bytes: &[u8]) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixed(u32);
    impl RandomSource for Fixed {
        fn next_inclusive(&mut self, _m: u32) -> u32 {
            self.0
        }
    }
    #[test]
    fn random_branch_is_selected_by_injected_rng() {
        let bgm = |s| {
            Block::Events(vec![Event::Bgm {
                pos: Position {
                    measure: 0,
                    fraction: 0.0,
                },
                sound: SoundRef(s),
            }])
        };
        let doc = ChartDocument {
            layout: KeyLayout::Beat7,
            meta: Metadata::default(),
            body: vec![Block::Random {
                max: 2,
                branches: vec![(1, vec![bgm(10)]), (2, vec![bgm(20)])],
            }],
        };
        let r = doc.resolve(&mut Fixed(1)); // 1+1 = 2
        assert_eq!(
            r.events,
            vec![Event::Bgm {
                pos: Position {
                    measure: 0,
                    fraction: 0.0
                },
                sound: SoundRef(20)
            }]
        );
    }
}
