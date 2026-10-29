//! 記法共通の譜面モデルと、記法実装が満たす trait。
//!
//! 読み込みは 2 段階:
//! 1. 記法デコーダ（例: `reprise-chart-bms`）が [`ChartDocument`] を作る。
//!    `#RANDOM` 等の分岐・定義（`#WAVxx` 等）・生オブジェクトをそのまま保持する。
//! 2. [`ChartDocument::resolve`] が、注入された乱数で分岐を選び、定義を適用し、
//!    LN を対にして、時刻順の [`ResolvedChart`] を作る。
//!
//! 特定プレイヤー（LR2 / beatoraja）の癖は [`ResolvePolicy`] で切り替える。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | CORE-RND-01 | `Random{Draw(n)}` は `next_inclusive(n-1)+1` を引く |
//! | CORE-RND-02 | `Random{Fixed(v)}` は乱数を消費しない |
//! | CORE-RND-03 | `If` は最も内側の `Random` の値と比較し、一致した腕だけ採用（無ければ `otherwise`） |
//! | CORE-RND-04 | `Random` の外の `If` はどの腕にも一致しない |
//! | CORE-RND-05 | 乱数は文書順に消費される（入れ子含む） |
//! | CORE-DEF-01 | 定義は文書順に適用し、同じ ID は後勝ち |
//! | CORE-DEF-02 | 初期 BPM は `ResolvePolicy::initial_bpm` に従い先勝ち/後勝ち |
//! | CORE-DEF-03 | 未定義の BPM/STOP 参照は `ResolvePolicy` に従い破棄または既定値 |
//! | CORE-LN-01 | `LnToggle` はレーンごとに時刻順で 頭→尾 と対になる |
//! | CORE-LN-02 | LN 終端 ID（`#LNOBJ` 相当）の通常ノーツは、同レーン直前の通常ノーツを LN にする |
//! | CORE-LN-03 | 対にならなかった LN 頭は通常ノーツになり、診断を出す |
//! | CORE-LN-04 | LN 保留はレーンごとに 1 つで、`LnToggle` と LN 終端 ID が共有する（LR2 互換） |
//! | CORE-ORD-01 | 出力イベントは位置順・同位置は文書順（安定ソート） |
//! | CORE-ORD-02 | `shift_when_note_at_zero` 有効時、位置 0 に可視/不可視ノーツがあれば全体を 1 小節後ろへずらす |

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// 乱数源。LR2 互換が欲しい利用側は DxLib 互換 MT を、そうでなければ任意の乱数を渡す。
pub trait RandomSource {
    /// `0..=max` の一様整数
    fn next_inclusive(&mut self, max: u32) -> u32;
}

/// 鍵盤構成
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyLayout {
    Beat5,
    Beat7,
    Beat10,
    Beat14,
    Popn9,
}

impl KeyLayout {
    /// 1 プレイヤーあたりの鍵盤数（皿を除く）
    pub fn keys(self) -> u8 {
        match self {
            KeyLayout::Beat5 | KeyLayout::Beat10 => 5,
            KeyLayout::Beat7 | KeyLayout::Beat14 => 7,
            KeyLayout::Popn9 => 9,
        }
    }
    pub fn is_double(self) -> bool {
        matches!(self, KeyLayout::Beat10 | KeyLayout::Beat14)
    }
}

/// レーン番号。1P: 0=皿, 1..=7=鍵盤。2P: 10=皿, 11..=17=鍵盤。ポップン: 1..=9。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lane(pub u8);

/// 譜面上の位置（小節番号 + 小節内の比率 `0.0..1.0`）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub measure: u32,
    pub fraction: f64,
}

impl Position {
    pub const fn new(measure: u32, fraction: f64) -> Self {
        Self { measure, fraction }
    }
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        self.measure
            .cmp(&other.measure)
            .then(self.fraction.total_cmp(&other.fraction))
    }
}

/// 記法上のオブジェクト ID（BMS なら 01..ZZ を 10 進化したもの）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BgaLayer {
    Base,
    Poor,
    Layer,
}

/// 分岐解決前のオブジェクト種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawObject {
    Normal,
    Invisible,
    Mine,
    /// LN チャンネルのオブジェクト（頭か尾かは解決時に決まる）
    LnToggle,
}

/// 分岐解決前のイベント
#[derive(Debug, Clone, PartialEq)]
pub enum RawEvent {
    Note {
        pos: Position,
        lane: Lane,
        object: RawObject,
        id: ObjId,
    },
    Bgm {
        pos: Position,
        id: ObjId,
    },
    /// 値が直接書かれた BPM 変化（BMS ch03）
    Bpm {
        pos: Position,
        bpm: f64,
    },
    /// 定義を参照する BPM 変化（BMS ch08）
    BpmRef {
        pos: Position,
        id: ObjId,
    },
    /// 定義を参照する STOP（BMS ch09）
    StopRef {
        pos: Position,
        id: ObjId,
    },
    MeasureLength {
        measure: u32,
        ratio: f64,
    },
    Bga {
        pos: Position,
        layer: BgaLayer,
        id: ObjId,
    },
}

/// 譜面のメタ情報
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub initial_bpm: Option<f64>,
    pub play_level: Option<i32>,
    pub difficulty: Option<i32>,
    pub rank: Option<i32>,
    pub total: Option<f64>,
    pub stage_file: Option<String>,
    pub banner: Option<String>,
    pub back_bmp: Option<String>,
    pub preview: Option<String>,
}

/// メタ情報の 1 項目（分岐の中に書かれることがあるので定義として扱う）
#[derive(Debug, Clone, PartialEq)]
pub enum MetaField {
    Title(String),
    Subtitle(String),
    Artist(String),
    Subartist(String),
    Genre(String),
    InitialBpm(f64),
    PlayLevel(i32),
    Difficulty(i32),
    Rank(i32),
    Total(f64),
    StageFile(String),
    Banner(String),
    BackBmp(String),
    Preview(String),
}

/// 定義（後勝ち。例外は [`ResolvePolicy`]）
#[derive(Debug, Clone, PartialEq)]
pub enum Definition {
    Sound(ObjId, String),
    Image(ObjId, String),
    Bpm(ObjId, f64),
    /// 1/192 小節単位
    Stop(ObjId, f64),
    /// この ID の通常ノーツは LN 終端になる（BMS `#LNOBJ`）
    LnEnd(ObjId),
    Meta(MetaField),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Event(RawEvent),
    Define(Definition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generator {
    /// `1..=max` を引く
    Draw { max: u32 },
    /// 値を固定（BMS `#SETRANDOM`）。乱数を消費しない
    Fixed(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Statements(Vec<Statement>),
    Random {
        generator: Generator,
        body: Vec<Block>,
    },
    /// 最も内側の `Random` の値と比較する
    If {
        arms: Vec<(u32, Vec<Block>)>,
        otherwise: Option<Vec<Block>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precedence {
    #[default]
    LastWins,
    FirstWins,
}

/// 未定義参照の扱い
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UndefinedRef {
    #[default]
    Drop,
    /// その値として扱う（LR2 は未定義 `#BPMxx` を -1.0 として扱う）
    Value(f64),
}

/// プレイヤーごとの解決時の差異
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvePolicy {
    pub initial_bpm: Precedence,
    pub undefined_bpm: UndefinedRef,
    pub undefined_stop: UndefinedRef,
    /// 位置 0 に可視/不可視ノーツがあると全体を 1 小節ずらす（LR2）
    pub shift_when_note_at_zero: bool,
}

impl ResolvePolicy {
    /// LR2 (beta3 100201) 互換
    pub const LR2: Self = Self {
        initial_bpm: Precedence::FirstWins,
        undefined_bpm: UndefinedRef::Value(-1.0),
        undefined_stop: UndefinedRef::Drop,
        shift_when_note_at_zero: true,
    };
}

/// 分岐を保持した未解決の譜面
#[derive(Debug, Clone, PartialEq)]
pub struct ChartDocument {
    pub layout: KeyLayout,
    pub policy: ResolvePolicy,
    pub body: Vec<Block>,
}

/// 確定したノーツ種別
#[derive(Debug, Clone, PartialEq)]
pub enum NoteKind {
    Normal,
    Invisible,
    /// 生の ID 値（ダメージ量の解釈は利用側）
    Mine {
        damage_raw: u32,
    },
    Long {
        end: Position,
        end_sound: Option<ObjId>,
    },
}

/// 確定したイベント
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Note {
        pos: Position,
        lane: Lane,
        kind: NoteKind,
        sound: Option<ObjId>,
    },
    Bgm {
        pos: Position,
        sound: ObjId,
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
        layer: BgaLayer,
        image: ObjId,
    },
}

impl Event {
    pub fn position(&self) -> Position {
        match self {
            Event::Note { pos, .. }
            | Event::Bgm { pos, .. }
            | Event::Bpm { pos, .. }
            | Event::Stop { pos, .. }
            | Event::Bga { pos, .. } => *pos,
            Event::MeasureLength { measure, .. } => Position::new(*measure, 0.0),
        }
    }
}

/// 分岐解決済みの譜面
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedChart {
    pub layout: KeyLayout,
    pub meta: Metadata,
    pub sounds: BTreeMap<ObjId, String>,
    pub images: BTreeMap<ObjId, String>,
    /// 位置順（同位置は文書順）
    pub events: Vec<Event>,
    pub diagnostics: Vec<Diagnostic>,
}

/// 読み込み・解決時の警告（エラーにはしない）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    pub fn at(line: usize, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }
    pub fn global(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Encoding(String),
    Syntax { line: usize, message: String },
    Other(String),
}

/// デコード時のヒント（記法によっては拡張子で解釈が変わる。例: `.pms`）
#[derive(Debug, Clone, Default)]
pub struct DecodeHint {
    /// 小文字・ドット無し
    pub extension: Option<String>,
}

/// 記法実装が満たす trait
pub trait ChartDecoder {
    fn name(&self) -> &'static str;
    /// 拡張子（小文字, ドット無し）
    fn extensions(&self) -> &'static [&'static str];
    fn decode(
        &self,
        bytes: &[u8],
        hint: &DecodeHint,
    ) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError>;
}

// ---------------------------------------------------------------------------
// 解決
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Collected {
    events: Vec<RawEvent>,
    defs: Vec<Definition>,
}

fn walk(blocks: &[Block], stack: &mut Vec<u32>, rng: &mut dyn RandomSource, out: &mut Collected) {
    for b in blocks {
        match b {
            Block::Statements(ss) => {
                for s in ss {
                    match s {
                        Statement::Event(e) => out.events.push(e.clone()),
                        Statement::Define(d) => out.defs.push(d.clone()),
                    }
                }
            }
            Block::Random { generator, body } => {
                let v = match *generator {
                    Generator::Draw { max } => rng.next_inclusive(max.saturating_sub(1)) + 1,
                    Generator::Fixed(v) => v,
                };
                stack.push(v);
                walk(body, stack, rng, out);
                stack.pop();
            }
            Block::If { arms, otherwise } => {
                let current = stack.last().copied();
                let hit = arms.iter().find(|(k, _)| Some(*k) == current);
                match (hit, otherwise) {
                    (Some((_, body)), _) => walk(body, stack, rng, out),
                    (None, Some(body)) if current.is_some() => walk(body, stack, rng, out),
                    _ => {}
                }
            }
        }
    }
}

impl ChartDocument {
    /// 分岐を解決して確定譜面を作る
    pub fn resolve(&self, rng: &mut dyn RandomSource) -> ResolvedChart {
        let mut c = Collected::default();
        walk(&self.body, &mut Vec::new(), rng, &mut c);

        let mut diagnostics = Vec::new();
        let mut meta = Metadata::default();
        let mut sounds = BTreeMap::new();
        let mut images = BTreeMap::new();
        let mut bpms: BTreeMap<ObjId, f64> = BTreeMap::new();
        let mut stops: BTreeMap<ObjId, f64> = BTreeMap::new();
        let mut ln_end: Option<ObjId> = None;
        for d in c.defs {
            match d {
                Definition::Sound(id, p) => {
                    sounds.insert(id, p);
                }
                Definition::Image(id, p) => {
                    images.insert(id, p);
                }
                Definition::Bpm(id, v) => {
                    bpms.insert(id, v);
                }
                Definition::Stop(id, v) => {
                    stops.insert(id, v);
                }
                Definition::LnEnd(id) => ln_end = Some(id),
                Definition::Meta(m) => apply_meta(&mut meta, m, self.policy.initial_bpm),
            }
        }

        // 参照の解決（位置付きの中間表現へ）
        let mut events: Vec<Event> = Vec::with_capacity(c.events.len());
        let mut raw_kinds: Vec<Option<RawObject>> = Vec::with_capacity(c.events.len());
        for e in c.events {
            let (ev, raw) = match e {
                RawEvent::Note {
                    pos,
                    lane,
                    object,
                    id,
                } => {
                    let kind = match object {
                        RawObject::Mine => NoteKind::Mine { damage_raw: id.0 },
                        RawObject::Invisible => NoteKind::Invisible,
                        RawObject::Normal | RawObject::LnToggle => NoteKind::Normal,
                    };
                    let sound = (object != RawObject::Mine).then_some(id);
                    (
                        Event::Note {
                            pos,
                            lane,
                            kind,
                            sound,
                        },
                        Some(object),
                    )
                }
                RawEvent::Bgm { pos, id } => (Event::Bgm { pos, sound: id }, None),
                RawEvent::Bpm { pos, bpm } => (Event::Bpm { pos, bpm }, None),
                RawEvent::BpmRef { pos, id } => {
                    match bpms.get(&id).copied().or(match self.policy.undefined_bpm {
                        UndefinedRef::Value(v) => Some(v),
                        UndefinedRef::Drop => None,
                    }) {
                        Some(bpm) => (Event::Bpm { pos, bpm }, None),
                        None => {
                            diagnostics.push(Diagnostic::global(format!(
                                "未定義の BPM 参照 {:?} を破棄",
                                id
                            )));
                            continue;
                        }
                    }
                }
                RawEvent::StopRef { pos, id } => {
                    match stops
                        .get(&id)
                        .copied()
                        .or(match self.policy.undefined_stop {
                            UndefinedRef::Value(v) => Some(v),
                            UndefinedRef::Drop => None,
                        }) {
                        Some(d) => (
                            Event::Stop {
                                pos,
                                duration_192th: d,
                            },
                            None,
                        ),
                        None => {
                            diagnostics.push(Diagnostic::global(format!(
                                "未定義の STOP 参照 {:?} を破棄",
                                id
                            )));
                            continue;
                        }
                    }
                }
                RawEvent::MeasureLength { measure, ratio } => {
                    (Event::MeasureLength { measure, ratio }, None)
                }
                RawEvent::Bga { pos, layer, id } => (
                    Event::Bga {
                        pos,
                        layer,
                        image: id,
                    },
                    None,
                ),
            };
            events.push(ev);
            raw_kinds.push(raw);
        }

        // 位置順に安定ソート（CORE-ORD-01）
        let mut order: Vec<usize> = (0..events.len()).collect();
        order.sort_by(|&a, &b| events[a].position().total_cmp(&events[b].position()));
        let mut sorted: Vec<Option<Event>> =
            order.iter().map(|&i| Some(events[i].clone())).collect();
        let kinds: Vec<Option<RawObject>> = order.iter().map(|&i| raw_kinds[i]).collect();

        // LN の対応付け（CORE-LN-01..04）
        let mut pending: BTreeMap<Lane, (usize, bool)> = BTreeMap::new(); // (index, is_toggle_head)
        for i in 0..sorted.len() {
            let Some(Event::Note {
                pos, lane, sound, ..
            }) = sorted[i].clone()
            else {
                continue;
            };
            match kinds[i] {
                Some(RawObject::LnToggle) => match pending.remove(&lane) {
                    None => {
                        pending.insert(lane, (i, true));
                    }
                    Some((head, _)) => {
                        make_long(&mut sorted[head], pos, sound);
                        sorted[i] = None;
                    }
                },
                Some(RawObject::Normal) if ln_end.is_some() => {
                    let is_end = sound == ln_end;
                    match (is_end, pending.get(&lane).copied()) {
                        (true, Some((head, _))) => {
                            make_long(&mut sorted[head], pos, sound);
                            pending.remove(&lane);
                            sorted[i] = None;
                        }
                        _ => {
                            if let Some((_, true)) = pending.insert(lane, (i, false)) {
                                diagnostics.push(Diagnostic::global(format!(
                                    "LN の頭 (lane {}) が通常ノーツで上書きされ、通常ノーツとして扱う",
                                    lane.0
                                )));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for (lane, (_, is_toggle)) in pending {
            if is_toggle {
                diagnostics.push(Diagnostic::global(format!(
                    "対になる LN 終端が無い (lane {})。通常ノーツとして扱う",
                    lane.0
                )));
            }
        }
        let mut events: Vec<Event> = sorted.into_iter().flatten().collect();

        // CORE-ORD-02
        if self.policy.shift_when_note_at_zero
            && events.iter().any(|e| {
                matches!(e, Event::Note { pos, kind: NoteKind::Normal | NoteKind::Invisible, .. }
                    if pos.measure == 0 && pos.fraction == 0.0)
            })
        {
            for e in &mut events {
                shift_one_measure(e);
            }
        }

        ResolvedChart {
            layout: self.layout,
            meta,
            sounds,
            images,
            events,
            diagnostics,
        }
    }
}

fn make_long(slot: &mut Option<Event>, end: Position, end_sound: Option<ObjId>) {
    if let Some(Event::Note { kind, .. }) = slot {
        *kind = NoteKind::Long { end, end_sound };
    }
}

fn shift_one_measure(e: &mut Event) {
    match e {
        Event::Note { pos, kind, .. } => {
            pos.measure += 1;
            if let NoteKind::Long { end, .. } = kind {
                end.measure += 1;
            }
        }
        Event::Bgm { pos, .. }
        | Event::Bpm { pos, .. }
        | Event::Stop { pos, .. }
        | Event::Bga { pos, .. } => pos.measure += 1,
        Event::MeasureLength { measure, .. } => *measure += 1,
    }
}

fn apply_meta(meta: &mut Metadata, m: MetaField, bpm_precedence: Precedence) {
    match m {
        MetaField::Title(v) => meta.title = v,
        MetaField::Subtitle(v) => meta.subtitle = v,
        MetaField::Artist(v) => meta.artist = v,
        MetaField::Subartist(v) => meta.subartist = v,
        MetaField::Genre(v) => meta.genre = v,
        MetaField::InitialBpm(v) => {
            if bpm_precedence == Precedence::LastWins || meta.initial_bpm.is_none() {
                meta.initial_bpm = Some(v);
            }
        }
        MetaField::PlayLevel(v) => meta.play_level = Some(v),
        MetaField::Difficulty(v) => meta.difficulty = Some(v),
        MetaField::Rank(v) => meta.rank = Some(v),
        MetaField::Total(v) => meta.total = Some(v),
        MetaField::StageFile(v) => meta.stage_file = Some(v),
        MetaField::Banner(v) => meta.banner = Some(v),
        MetaField::BackBmp(v) => meta.back_bmp = Some(v),
        MetaField::Preview(v) => meta.preview = Some(v),
    }
}

#[cfg(test)]
mod tests;
