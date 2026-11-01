//! 本体の解析
use crate::csv::{split, Row};
use crate::Dialect;
use reprise_skin_core::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_INCLUDE_DEPTH: usize = 16;
const OP_ALWAYS_TRUE: i32 = 0;
const OP_ALWAYS_FALSE: i32 = 999;

// ---------------------------------------------------------------------------
// 文字コード（SKN-ENC-01）
// ---------------------------------------------------------------------------

pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(body) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8_lossy(body).into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

pub fn read_text(path: &Path, files: &mut dyn FileResolver) -> Result<String, SkinError> {
    let bytes = files.read(path).map_err(|message| SkinError::Io {
        path: path.to_path_buf(),
        message,
    })?;
    Ok(decode_text(&bytes))
}

fn normalize(p: &str) -> String {
    p.replace('\\', "/") // SKN-IMG-02
}

/// SKN-CSV-02: `#` で始まる行だけを対象にする
fn command_line(raw: &str) -> Option<&str> {
    let t = raw.trim();
    t.starts_with('#').then_some(t)
}

// ---------------------------------------------------------------------------
// ヘッダ（SKN-HDR-01..05）
// ---------------------------------------------------------------------------

pub fn header(text: &str) -> SkinHeader {
    let mut h = SkinHeader::default();
    for raw in text.lines() {
        let Some(line) = command_line(raw) else {
            continue;
        };
        let row = split(line);
        match row.command().as_str() {
            "#INFORMATION" => {
                h.kind = row.v(1);
                h.title = row.s(2).to_string();
                h.maker = row.s(3).to_string();
                h.thumbnail = row.s(4).to_string();
                h.resolution = (row.v(6).max(640) as u32, row.v(7).max(480) as u32);
            }
            "#RESOLUTION" => {
                h.resolution = match row.v(1) {
                    0 => (640, 480),
                    1 => (1280, 720),
                    2 => (1920, 1080),
                    3 => (3840, 2160),
                    _ => (row.v(1).max(640) as u32, row.v(2).max(480) as u32),
                };
            }
            "#CUSTOMOPTION" => {
                if row.v(2) < 1000 {
                    let labels = row.str[3.min(row.str.len())..]
                        .iter()
                        .take_while(|s| !s.is_empty())
                        .cloned()
                        .collect();
                    h.customs.push(Custom::Option {
                        name: row.s(1).to_string(),
                        start_op: row.v(2),
                        labels,
                    });
                }
            }
            "#CUSTOMFILE" => h.customs.push(Custom::File {
                name: row.s(1).to_string(),
                pattern: normalize(row.s(2)),
                default: row.s(3).to_string(),
            }),
            "#ENDOFHEADER" => break,
            _ => {}
        }
    }
    h
}

/// SKN-OPT-01
fn initial_options(h: &SkinHeader, s: &SkinSettings) -> BTreeSet<i32> {
    let mut ops = s.options.clone();
    for (i, c) in h.customs.iter().enumerate() {
        if let Custom::Option {
            start_op, labels, ..
        } = c
        {
            let n = labels.len() as i32;
            let chosen = match s.custom.get(&i) {
                Some(CustomChoice::Option(op)) if (*start_op..*start_op + n).contains(op) => *op,
                _ => *start_op,
            };
            ops.insert(chosen);
        }
    }
    ops.insert(OP_ALWAYS_TRUE);
    ops.remove(&OP_ALWAYS_FALSE);
    ops
}

// ---------------------------------------------------------------------------
// #IF（SKN-IF-01..04）
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    Active,
    Inactive,
    Done,
}

#[derive(Default)]
struct IfState(Vec<Arm>);

impl IfState {
    fn skipping(&self, dialect: Dialect) -> bool {
        match dialect {
            // LR2 は最も内側の状態しか見ない（SKN-IF-03）
            Dialect::Lr2 => self.0.last().is_some_and(|a| *a != Arm::Active),
            Dialect::Standard => self.0.iter().any(|a| *a != Arm::Active),
        }
    }
}

// ---------------------------------------------------------------------------
// 本体
// ---------------------------------------------------------------------------

struct Ctx<'a> {
    files: &'a mut dyn FileResolver,
    settings: &'a SkinSettings,
    dialect: Dialect,
    header: SkinHeader,
    ops: BTreeSet<i32>,
    /// (パターン, 選択)
    custom_files: Vec<(String, String)>,
    custom_index: usize,
    doc_images: Vec<ImageEntry>,
    doc_fonts: Vec<FontEntry>,
    image_fonts: Vec<ImageFontEntry>,
    elements: Vec<Element>,
    directives: Vec<Directive>,
    diags: Vec<Diagnostic>,
    /// 処理した命令行の数（描画順）
    lines: u32,
}

pub fn decode(
    path: &Path,
    settings: &SkinSettings,
    files: &mut dyn FileResolver,
    dialect: Dialect,
) -> Result<SkinDocument, SkinError> {
    let text = read_text(path, files)?;
    let header = header(&text);
    let ops = initial_options(&header, settings);
    let mut cx = Ctx {
        files,
        settings,
        dialect,
        header,
        ops,
        custom_files: vec![],
        custom_index: 0,
        doc_images: vec![],
        doc_fonts: vec![],
        image_fonts: vec![],
        elements: vec![],
        directives: vec![],
        diags: vec![],
        lines: 0,
    };
    cx.process(path, &text, 0);
    Ok(SkinDocument {
        header: cx.header,
        images: cx.doc_images,
        fonts: cx.doc_fonts,
        image_fonts: cx.image_fonts,
        elements: cx.elements,
        directives: cx.directives,
        diagnostics: cx.diags,
        options: cx.ops,
    })
}

impl Ctx<'_> {
    fn diag(&mut self, file: &Path, line: usize, message: impl Into<String>) {
        self.diags.push(Diagnostic {
            file: file.to_path_buf(),
            line,
            message: message.into(),
        });
    }

    fn op_true(&self, op: i32) -> bool {
        self.ops.contains(&op)
    }

    /// SKN-IF-01 / SKN-IF-04
    fn eval(&self, row: &Row) -> bool {
        (1..10).all(|i| {
            let v = row.v(i);
            match self.dialect {
                Dialect::Lr2 => (0..=999).contains(&v) && self.op_true(v),
                Dialect::Standard if v < 0 => !self.op_true(-v),
                Dialect::Standard => v <= 999 && self.op_true(v),
            }
        })
    }

    /// SKN-CUS-01
    fn substitute(&self, path: &str) -> String {
        let path = normalize(path);
        for (pattern, choice) in &self.custom_files {
            if path.starts_with(pattern.as_str())
                && !matches!(choice.as_str(), "RANDOM" | "ERROR" | "")
            {
                return path.replace('*', choice);
            }
        }
        path
    }

    fn resolve(&mut self, pattern: &str) -> Option<PathBuf> {
        let p = self.substitute(pattern);
        self.files.resolve(&p)
    }

    /// SKN-FNT-02: `.lr2font` を読み、画像はフォントファイルと同じフォルダから探す
    fn image_font(&mut self, file: &Path, no: usize, raw: &str) -> ImageFontEntry {
        let missing = |s: &mut Self, what: &str| {
            s.diag(file, no, format!("フォントが見つからない: {what}"));
            ImageFontEntry::Missing(normalize(raw))
        };
        let pattern = self.substitute(raw);
        let Some(path) = self.files.resolve(&pattern) else {
            return missing(self, raw);
        };
        let Ok(text) = read_text(&path, self.files) else {
            return missing(self, raw);
        };
        let font = crate::lr2font::parse(&text);
        let dir = pattern.rsplit_once('/').map_or("", |(d, _)| d);
        let mut textures = vec![];
        for (id, name) in font.textures {
            let p = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let entry = match self.files.resolve(&p) {
                Some(found) => ImageEntry::File(found),
                None => {
                    self.diag(file, no, format!("フォントの画像が見つからない: {p}"));
                    ImageEntry::Missing(p)
                }
            };
            textures.push((id, entry));
        }
        ImageFontEntry::Loaded(ImageFont {
            size: font.size,
            margin: font.margin,
            textures,
            glyphs: font.glyphs,
        })
    }

    fn process(&mut self, file: &Path, text: &str, depth: usize) {
        let mut ifs = IfState::default();
        for (i, raw) in text.lines().enumerate() {
            let no = i + 1;
            let raw = raw.trim_end_matches('\r');
            if self.dialect == Dialect::Lr2 && raw.len() <= 5 {
                continue; // SKN-CSV-03
            }
            let Some(line) = command_line(raw) else {
                continue;
            };
            let row = split(line);
            let cmd = row.command();
            // SKN-IF-01..04（#IF 系はスキップ中でも状態を更新する）
            match cmd.as_str() {
                "#IF" => {
                    let parent_off =
                        self.dialect == Dialect::Standard && ifs.skipping(self.dialect);
                    let arm = if parent_off {
                        Arm::Done
                    } else if self.eval(&row) {
                        Arm::Active
                    } else {
                        Arm::Inactive
                    };
                    ifs.0.push(arm);
                    continue;
                }
                "#ELSEIF" => {
                    match ifs.0.last().copied() {
                        None => self.diag(file, no, "対応する #IF が無い #ELSEIF"),
                        Some(Arm::Done) => {}
                        Some(Arm::Active) => *ifs.0.last_mut().unwrap() = Arm::Done,
                        Some(Arm::Inactive) => {
                            *ifs.0.last_mut().unwrap() = if self.eval(&row) {
                                Arm::Active
                            } else {
                                Arm::Inactive
                            }
                        }
                    }
                    continue;
                }
                "#ELSE" => {
                    match ifs.0.last().copied() {
                        None => self.diag(file, no, "対応する #IF が無い #ELSE"),
                        Some(Arm::Done) => {}
                        Some(Arm::Active) => *ifs.0.last_mut().unwrap() = Arm::Done,
                        Some(Arm::Inactive) => *ifs.0.last_mut().unwrap() = Arm::Active,
                    }
                    continue;
                }
                "#ENDIF" => {
                    if ifs.0.pop().is_none() {
                        self.diag(file, no, "対応する #IF が無い #ENDIF");
                    }
                    continue;
                }
                _ => {}
            }
            if ifs.skipping(self.dialect) {
                continue;
            }
            self.lines += 1;
            self.command(file, no, &cmd, &row, depth);
        }
    }

    fn command(&mut self, file: &Path, no: usize, cmd: &str, row: &Row, depth: usize) {
        match cmd {
            "#INFORMATION" | "#RESOLUTION" | "#ENDOFHEADER" => {}
            "#CUSTOMOPTION" => self.custom_index += 1,
            "#CUSTOMFILE" => {
                // SKN-CUS-02
                let pattern = normalize(row.s(2));
                let choice = match self.settings.custom.get(&self.custom_index) {
                    Some(CustomChoice::File(f)) => f.clone(),
                    _ => match self.header.customs.get(self.custom_index) {
                        Some(Custom::File { default, .. }) => default.clone(),
                        _ => row.s(3).to_string(),
                    },
                };
                let choice = if choice == "RANDOM" {
                    self.files
                        .resolve(&pattern)
                        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                        .unwrap_or_default()
                } else {
                    choice
                };
                self.custom_files.push((pattern, choice));
                self.custom_index += 1;
            }
            "#SETOPTION" => {
                // SKN-SET-01
                let op = row.v(1);
                if (0..1000).contains(&op) {
                    if row.v(2) != 0 {
                        self.ops.insert(op);
                    } else {
                        self.ops.remove(&op);
                    }
                } else {
                    self.diag(file, no, "#SETOPTION の番号は 0..=999");
                }
            }
            "#IMAGE" => {
                // SKN-IMG-01
                let raw = row.s(1);
                let entry = if raw.eq_ignore_ascii_case("CONTINUE") {
                    ImageEntry::Continue
                } else {
                    match self.resolve(raw) {
                        Some(p) => ImageEntry::File(p),
                        None => {
                            self.diag(file, no, format!("画像が見つからない: {raw}"));
                            ImageEntry::Missing(normalize(raw))
                        }
                    }
                };
                self.doc_images.push(entry);
            }
            "#FONT" => self.doc_fonts.push(FontEntry::System {
                size: row.v(1),
                thickness: row.v(2),
                kind: row.v(3),
            }),
            "#LR2FONT" => {
                let entry = self.image_font(file, no, row.s(1));
                self.image_fonts.push(entry);
            }
            "#INCLUDE" => {
                // SKN-INC-01
                if depth + 1 > MAX_INCLUDE_DEPTH {
                    self.diag(file, no, "#INCLUDE が深すぎる");
                    return;
                }
                let raw = row.s(1);
                let Some(p) = self.resolve(raw) else {
                    self.diag(file, no, format!("#INCLUDE 先が見つからない: {raw}"));
                    return;
                };
                match read_text(&p, self.files) {
                    Ok(text) => self.process(&p, &text, depth + 1),
                    Err(e) => self.diag(file, no, format!("#INCLUDE 先を読めない: {e:?}")),
                }
            }
            _ => {
                if let Some(name) = cmd.strip_prefix("#SRC_") {
                    self.src(name, row);
                } else if let Some(name) = cmd.strip_prefix("#DST_") {
                    self.dst(file, no, name, row);
                } else {
                    // SKN-DIR-01
                    self.directives.push(Directive {
                        command: cmd.trim_start_matches('#').to_string(),
                        args: row.str[1..].to_vec(),
                    });
                }
            }
        }
    }

    /// SKN-SRC-01..03
    fn src(&mut self, name: &str, row: &Row) {
        let v = |i| row.v(i);
        let (kind, source) = if name == "TEXT" {
            (
                ElementKind::Text {
                    font: FontRef(v(2).max(0) as u32),
                    value: ValueRef(v(3)),
                    align: v(4),
                    editable: v(5) != 0,
                },
                None,
            )
        } else {
            let kind = match name {
                "IMAGE" => ElementKind::Image,
                "NUMBER" => ElementKind::Number {
                    value: ValueRef(v(11)),
                    align: v(12),
                    digits: v(13),
                },
                "SLIDER" => ElementKind::Slider {
                    direction: v(11),
                    range: v(12),
                    value: ValueRef(v(13)),
                    disabled: v(14) != 0,
                },
                "BARGRAPH" => ElementKind::Bargraph {
                    value: ValueRef(v(11)),
                    direction: v(12),
                },
                "BUTTON" => ElementKind::Button {
                    action: v(11),
                    click: v(12),
                    panel: v(13),
                    plus_only: v(14) != 0,
                },
                _ => ElementKind::Other,
            };
            let source = Source {
                image: ImageRef(v(2).max(0) as u32),
                rect: [v(3).max(0), v(4).max(0), v(5), v(6)],
                divide: (v(7).max(1) as u32, v(8).max(1) as u32),
                cycle_ms: v(9),
                timer: Some(TimerRef(v(10))),
                params: [v(11), v(12), v(13), v(14), v(15)],
            };
            (kind, Some(source))
        };
        self.elements.push(Element {
            command: name.to_string(),
            index: v(1),
            kind,
            source,
            timer: None,
            conditions: vec![],
            looping: None,
            dst_extra: [0; 2],
            order: 0,
            keyframes: vec![],
        });
    }

    /// SKN-DST-01..04
    fn dst(&mut self, file: &Path, no: usize, name: &str, row: &Row) {
        let index = row.v(1);
        let same = |e: &Element| e.command == name;
        let target = self
            .elements
            .iter()
            .rposition(|e| same(e) && e.index == index)
            .or_else(|| self.elements.iter().rposition(same));
        let target = match target {
            Some(t) => t,
            None => {
                let prefix = self
                    .elements
                    .iter()
                    .rposition(|e| name.starts_with(&format!("{}_", e.command)));
                match prefix {
                    Some(p) => {
                        let mut e = self.elements[p].clone();
                        e.command = name.to_string();
                        e.index = index;
                        e.timer = None;
                        e.conditions.clear();
                        e.looping = None;
                        e.dst_extra = [0; 2];
                        e.keyframes.clear();
                        self.elements.push(e);
                        self.elements.len() - 1
                    }
                    None => {
                        self.diag(file, no, format!("対応する #SRC_{name} が無い"));
                        return;
                    }
                }
            }
        };
        let blend = match row.v(12) {
            0 => Blend::None,
            1 => Blend::Alpha,
            2 => Blend::Add,
            3 => Blend::Subtract,
            4 => Blend::Multiply,
            5 => Blend::Subtract2,
            9 => Blend::InvertDest,
            10 => Blend::InvertSrc,
            11 => Blend::MultiplyAlpha,
            other => {
                self.diag(file, no, format!("不明な合成 {other}（Alpha として扱う）"));
                Blend::Alpha
            }
        };
        let byte = |i: usize| row.v(i).clamp(0, 255) as u8;
        let kf = Keyframe {
            time_ms: row.v(2),
            rect: [
                row.v(3) as f32,
                row.v(4) as f32,
                row.v(5) as f32,
                row.v(6) as f32,
            ],
            accel: row.v(7),
            argb: [byte(8), byte(9), byte(10), byte(11)],
            blend,
            filter: row.v(13) != 0,
            angle: row.v(14) as f32,
            center: byte(15),
        };
        let e = &mut self.elements[target];
        if e.keyframes.is_empty() {
            e.looping = Some(row.v(16));
            e.timer = Some(TimerRef(row.v(17)));
            e.conditions = (18..=20)
                .map(|i| row.v(i))
                .filter(|v| *v != 0)
                .map(ConditionRef)
                .collect();
            e.dst_extra = [row.v(21), row.v(22)];
            e.order = self.lines;
        }
        e.keyframes.push(kf);
    }
}
