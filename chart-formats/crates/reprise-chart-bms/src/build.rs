//! 行列 → ChartDocument（分岐構造の構築・チャンネル/ヘッダの変換・鍵盤構成判定）
use crate::lex::{atof, atol, classify, parse_hex, parse_id, Line};
use crate::Dialect;
use reprise_chart_core::*;
use std::collections::BTreeSet;

pub fn build(text: &str, is_pms: bool, dialect: Dialect) -> (ChartDocument, Vec<Diagnostic>) {
    let lines: Vec<(usize, Line<'_>)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, classify(l)))
        .collect();
    let layout = detect_layout(&lines, is_pms);
    let mut em = Emitter {
        layout,
        dialect,
        base62: false,
        diags: Vec::new(),
        unsupported: BTreeSet::new(),
    };
    let mut tree = Tree::default();
    match dialect {
        Dialect::Lr2 => build_lr2(&lines, &mut tree, &mut em),
        Dialect::Standard => build_standard(&lines, &mut tree, &mut em),
    }
    let body = tree.finish();
    let policy = match dialect {
        Dialect::Lr2 => ResolvePolicy::LR2,
        Dialect::Standard => ResolvePolicy::default(),
    };
    (
        ChartDocument {
            layout,
            policy,
            body,
        },
        em.diags,
    )
}

// ---------------------------------------------------------------------------
// 分岐構造
// ---------------------------------------------------------------------------

enum Frame {
    Random {
        generator: Generator,
        body: Vec<Block>,
    },
    If {
        arms: Vec<(u32, Vec<Block>)>,
        otherwise: Option<Vec<Block>>,
    },
}

#[derive(Default)]
struct Tree {
    root: Vec<Block>,
    stack: Vec<Frame>,
}

impl Tree {
    fn current(&mut self) -> &mut Vec<Block> {
        match self.stack.last_mut() {
            None => &mut self.root,
            Some(Frame::Random { body, .. }) => body,
            Some(Frame::If { arms, otherwise }) => match otherwise {
                Some(o) => o,
                None => &mut arms.last_mut().expect("If は腕を持つ").1,
            },
        }
    }
    fn push(&mut self, stmts: Vec<Statement>) {
        if stmts.is_empty() {
            return;
        }
        let cur = self.current();
        if let Some(Block::Statements(v)) = cur.last_mut() {
            v.extend(stmts);
        } else {
            cur.push(Block::Statements(stmts));
        }
    }
    fn open(&mut self, f: Frame) {
        self.stack.push(f);
    }
    fn top_is_if(&self) -> bool {
        matches!(self.stack.last(), Some(Frame::If { .. }))
    }
    fn top_is_random(&self) -> bool {
        matches!(self.stack.last(), Some(Frame::Random { .. }))
    }
    fn close(&mut self) {
        let block = match self.stack.pop() {
            Some(Frame::Random { generator, body }) => Block::Random { generator, body },
            Some(Frame::If { arms, otherwise }) => Block::If { arms, otherwise },
            None => return,
        };
        self.current().push(block);
    }
    fn close_all(&mut self) {
        while !self.stack.is_empty() {
            self.close();
        }
    }
    fn finish(mut self) -> Vec<Block> {
        self.close_all();
        self.root
    }
}

/// 乱数値は 1 以上。0 以下の `#IF` 値はどの乱数とも一致しない値にする
fn arm_key(v: i64) -> u32 {
    u32::try_from(v)
        .ok()
        .filter(|&v| v >= 1)
        .unwrap_or(u32::MAX)
}

fn draw(n: i64) -> Generator {
    Generator::Draw {
        max: u32::try_from(n).unwrap_or(1).max(1),
    }
}

/// BMS-RND-L2-01..04
fn build_lr2(lines: &[(usize, Line<'_>)], tree: &mut Tree, em: &mut Emitter) {
    let mut skipping = false;
    for (no, line) in lines {
        match line {
            Line::Random(n) => {
                tree.close_all();
                tree.open(Frame::Random {
                    generator: draw(*n),
                    body: vec![],
                });
                skipping = true;
            }
            Line::If(k) => {
                if tree.top_is_if() {
                    tree.close();
                }
                tree.open(Frame::If {
                    arms: vec![(arm_key(*k), vec![])],
                    otherwise: None,
                });
                skipping = false;
            }
            Line::EndIf => {
                if tree.top_is_if() {
                    tree.close();
                }
                skipping = false;
            }
            Line::Else | Line::ElseIf(_) | Line::EndRandom | Line::SetRandom(_) => {
                em.diags.push(Diagnostic::at(
                    *no,
                    "LR2 方言では #ELSE/#ELSEIF/#ENDRANDOM/#SETRANDOM を無視する",
                ));
            }
            other => {
                if !skipping {
                    let s = em.statements(*no, other);
                    tree.push(s);
                }
            }
        }
    }
}

/// BMS-RND-STD-01..02
fn build_standard(lines: &[(usize, Line<'_>)], tree: &mut Tree, em: &mut Emitter) {
    for (no, line) in lines {
        match line {
            Line::Random(_) | Line::SetRandom(_) => {
                if tree.top_is_random() {
                    tree.close(); // 暗黙の #ENDRANDOM
                }
                let generator = match line {
                    Line::Random(n) => draw(*n),
                    Line::SetRandom(v) => Generator::Fixed(arm_key(*v)),
                    _ => unreachable!(),
                };
                tree.open(Frame::Random {
                    generator,
                    body: vec![],
                });
            }
            Line::If(k) => tree.open(Frame::If {
                arms: vec![(arm_key(*k), vec![])],
                otherwise: None,
            }),
            Line::ElseIf(k) => match tree.stack.last_mut() {
                Some(Frame::If {
                    arms,
                    otherwise: None,
                }) => arms.push((arm_key(*k), vec![])),
                _ => em
                    .diags
                    .push(Diagnostic::at(*no, "対応する #IF が無い #ELSEIF")),
            },
            Line::Else => match tree.stack.last_mut() {
                Some(Frame::If { otherwise, .. }) if otherwise.is_none() => {
                    *otherwise = Some(vec![])
                }
                _ => em
                    .diags
                    .push(Diagnostic::at(*no, "対応する #IF が無い #ELSE")),
            },
            Line::EndIf => {
                if tree.top_is_if() {
                    tree.close();
                } else {
                    em.diags
                        .push(Diagnostic::at(*no, "対応する #IF が無い #ENDIF"));
                }
            }
            Line::EndRandom => {
                if tree.top_is_if() {
                    em.diags
                        .push(Diagnostic::at(*no, "#ENDIF が無いまま #ENDRANDOM"));
                    tree.close();
                }
                if tree.top_is_random() {
                    tree.close();
                } else {
                    em.diags
                        .push(Diagnostic::at(*no, "対応する #RANDOM が無い #ENDRANDOM"));
                }
            }
            other => {
                let s = em.statements(*no, other);
                tree.push(s);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 鍵盤構成（BMS-LAY-01）
// ---------------------------------------------------------------------------

fn detect_layout(lines: &[(usize, Line<'_>)], is_pms: bool) -> KeyLayout {
    if is_pms {
        return KeyLayout::Popn9;
    }
    let (mut double, mut seven) = (false, false);
    for (_, l) in lines {
        let Line::Message { channel, data, .. } = l else {
            continue;
        };
        if data.bytes().all(|c| c == b'0') {
            continue;
        }
        let Some((player, _)) = note_class(channel[0]) else {
            continue;
        };
        let x = channel[1];
        if !(b'1'..=b'9').contains(&x) || x == b'7' {
            continue;
        }
        if player == 2 {
            double = true;
        }
        if x == b'8' || x == b'9' {
            seven = true;
        }
    }
    match (double, seven) {
        (true, true) => KeyLayout::Beat14,
        (true, false) => KeyLayout::Beat10,
        (false, true) => KeyLayout::Beat7,
        (false, false) => KeyLayout::Beat5,
    }
}

/// ノーツチャンネルの 1 文字目 → (プレイヤー, 種別)。BMS-CH-08
fn note_class(c: u8) -> Option<(u8, RawObject)> {
    Some(match c {
        b'1' => (1, RawObject::Normal),
        b'2' => (2, RawObject::Normal),
        b'3' => (1, RawObject::Invisible),
        b'4' => (2, RawObject::Invisible),
        b'5' => (1, RawObject::LnToggle),
        b'6' => (2, RawObject::LnToggle),
        b'D' => (1, RawObject::Mine),
        b'E' => (2, RawObject::Mine),
        _ => return None,
    })
}

/// BMS-CH-06 / BMS-CH-07
fn lane_of(layout: KeyLayout, player: u8, x: u8) -> Option<Lane> {
    if layout == KeyLayout::Popn9 {
        let lane = match (player, x) {
            (1, b'1'..=b'5') => x - b'0',
            (1, b'8') => 6,
            (1, b'9') => 7,
            (1, b'6') => 8,
            (1, b'7') => 9,
            (2, b'2') => 6,
            (2, b'3') => 7,
            (2, b'4') => 8,
            (2, b'5') => 9,
            _ => return None,
        };
        return Some(Lane(lane));
    }
    let base = match x {
        b'1'..=b'5' => x - b'0',
        b'6' => 0,
        b'8' => 6,
        b'9' => 7,
        _ => return None,
    };
    Some(Lane(base + if player == 2 { 10 } else { 0 }))
}

// ---------------------------------------------------------------------------
// 行 → 文
// ---------------------------------------------------------------------------

struct Emitter {
    layout: KeyLayout,
    dialect: Dialect,
    base62: bool,
    diags: Vec<Diagnostic>,
    unsupported: BTreeSet<String>,
}

impl Emitter {
    fn statements(&mut self, no: usize, line: &Line<'_>) -> Vec<Statement> {
        match line {
            Line::Header { key, value } => self.header(no, key, value).into_iter().collect(),
            Line::Message {
                measure,
                channel,
                data,
            } => self.message(no, *measure, *channel, data),
            _ => vec![],
        }
    }

    fn id(&mut self, no: usize, s: &str) -> Option<ObjId> {
        let b = s.as_bytes();
        let id = (b.len() >= 2)
            .then(|| parse_id(&b[..2], self.base62))
            .flatten();
        if id.is_none() {
            self.diags
                .push(Diagnostic::at(no, format!("不正な ID: {s}")));
        }
        id.map(ObjId)
    }

    fn header(&mut self, no: usize, key: &str, value: &str) -> Option<Statement> {
        let meta = |m| Some(Statement::Define(Definition::Meta(m)));
        let text = || value.to_string();
        match key {
            "TITLE" => return meta(MetaField::Title(text())),
            "SUBTITLE" => return meta(MetaField::Subtitle(text())),
            "ARTIST" => return meta(MetaField::Artist(text())),
            "SUBARTIST" => return meta(MetaField::Subartist(text())),
            "GENRE" => return meta(MetaField::Genre(text())),
            "BPM" => return meta(MetaField::InitialBpm(atof(value))),
            "PLAYLEVEL" => return meta(MetaField::PlayLevel(atol(value) as i32)),
            "DIFFICULTY" => return meta(MetaField::Difficulty(atol(value) as i32)),
            "RANK" => return meta(MetaField::Rank(atol(value) as i32)),
            "TOTAL" => {
                let v = match self.dialect {
                    Dialect::Lr2 => atol(value) as f64,
                    Dialect::Standard => atof(value),
                };
                return meta(MetaField::Total(v));
            }
            "STAGEFILE" => return meta(MetaField::StageFile(text())),
            "BANNER" => return meta(MetaField::Banner(text())),
            "BACKBMP" => return meta(MetaField::BackBmp(text())),
            "PREVIEW" => return meta(MetaField::Preview(text())),
            "LNOBJ" => {
                return self
                    .id(no, value)
                    .map(|id| Statement::Define(Definition::LnEnd(id)))
            }
            "BASE" => {
                self.base62 = atol(value) == 62;
                return None;
            }
            "PLAYER" => return None,
            "LNTYPE" => {
                if atol(value) != 1 {
                    self.diags.push(Diagnostic::at(
                        no,
                        "#LNTYPE 1 以外は未対応（LNTYPE 1 として扱う）",
                    ));
                }
                return None;
            }
            _ => {}
        }
        let def = |prefix: &str| -> Option<&str> {
            key.strip_prefix(prefix).filter(|rest| rest.len() == 2)
        };
        if let Some(idk) = def("WAV") {
            return self
                .id(no, idk)
                .map(|id| Statement::Define(Definition::Sound(id, text())));
        }
        if let Some(idk) = def("BMP") {
            return self
                .id(no, idk)
                .map(|id| Statement::Define(Definition::Image(id, text())));
        }
        if let Some(idk) = def("BPM").or_else(|| def("EXBPM")) {
            let v = atof(value);
            return self
                .id(no, idk)
                .map(|id| Statement::Define(Definition::Bpm(id, v)));
        }
        if let Some(idk) = def("STOP") {
            let v = match self.dialect {
                Dialect::Lr2 => atol(value) as f64,
                Dialect::Standard => atof(value),
            };
            if v <= 0.0 {
                self.diags
                    .push(Diagnostic::at(no, "#STOP は正の値のみ有効"));
                return None;
            }
            return self
                .id(no, idk)
                .map(|id| Statement::Define(Definition::Stop(id, v)));
        }
        self.diags
            .push(Diagnostic::at(no, format!("未対応のヘッダ #{key}")));
        None
    }

    fn message(&mut self, no: usize, measure: u32, ch: [u8; 2], data: &str) -> Vec<Statement> {
        let ev = Statement::Event;
        if &ch == b"02" {
            let ratio = atof(data);
            if ratio <= 0.0 {
                self.diags
                    .push(Diagnostic::at(no, "小節長は正の値のみ有効"));
                return vec![];
            }
            return vec![ev(RawEvent::MeasureLength { measure, ratio })];
        }
        let b = data.as_bytes();
        let n = b.len() / 2;
        let mut out = Vec::new();
        for i in 0..n {
            let pair = &b[i * 2..i * 2 + 2];
            if pair == b"00" {
                continue;
            }
            let pos = Position::new(measure, i as f64 / n as f64);
            let made = match &ch {
                b"03" => parse_hex(pair).map(|v| RawEvent::Bpm { pos, bpm: v as f64 }),
                _ => {
                    let Some(id) = parse_id(pair, self.base62).map(ObjId) else {
                        self.diags.push(Diagnostic::at(
                            no,
                            format!("不正な ID: {}", String::from_utf8_lossy(pair)),
                        ));
                        continue;
                    };
                    match self.channel_event(&ch, pos, id) {
                        Ok(e) => e,
                        Err(()) => {
                            let name = String::from_utf8_lossy(&ch).into_owned();
                            if self.unsupported.insert(name.clone()) {
                                self.diags.push(Diagnostic::at(
                                    no,
                                    format!("未対応のチャンネル {name} を無視"),
                                ));
                            }
                            return out;
                        }
                    }
                }
            };
            if let Some(e) = made {
                out.push(ev(e));
            }
        }
        out
    }

    /// `Ok(None)` = この位置は捨てる（フリーゾーン等）、`Err` = 未対応チャンネル
    fn channel_event(
        &mut self,
        ch: &[u8; 2],
        pos: Position,
        id: ObjId,
    ) -> Result<Option<RawEvent>, ()> {
        Ok(Some(match ch {
            b"01" => RawEvent::Bgm { pos, id },
            b"04" => RawEvent::Bga {
                pos,
                layer: BgaLayer::Base,
                id,
            },
            b"06" => RawEvent::Bga {
                pos,
                layer: BgaLayer::Poor,
                id,
            },
            b"07" => RawEvent::Bga {
                pos,
                layer: BgaLayer::Layer,
                id,
            },
            b"08" => RawEvent::BpmRef { pos, id },
            b"09" => RawEvent::StopRef { pos, id },
            [c, x] => {
                let (player, object) = note_class(*c).ok_or(())?;
                if !x.is_ascii_digit() || *x == b'0' {
                    return Err(());
                }
                match lane_of(self.layout, player, *x) {
                    Some(lane) => RawEvent::Note {
                        pos,
                        lane,
                        object,
                        id,
                    },
                    None => {
                        if self.unsupported.insert(format!("{}7", *c as char)) {
                            self.diags.push(Diagnostic::global(
                                "フリーゾーン（x7）等の割り当ての無いレーンを破棄",
                            ));
                        }
                        return Ok(None);
                    }
                }
            }
        }))
    }
}
