//! JSON の木 → SkinDocument
use crate::lenient;
use reprise_skin_core::*;
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const MAX_INCLUDE_DEPTH: usize = 16;

// ---------------------------------------------------------------------------
// 値の取り出し（int 欄に "10" や 1.7 が来ても libGDX と同じく整数にする）
// ---------------------------------------------------------------------------

fn num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>()
                .ok()
                .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
        }
        _ => None,
    }
}
fn int(o: &Map<String, Value>, k: &str) -> Option<i64> {
    o.get(k).and_then(num)
}
fn i32_or(o: &Map<String, Value>, k: &str, d: i32) -> i32 {
    int(o, k).map(|v| v as i32).unwrap_or(d)
}
fn string(o: &Map<String, Value>, k: &str) -> Option<String> {
    match o.get(k)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn flag(o: &Map<String, Value>, k: &str, d: bool) -> bool {
    match o.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(v) => num(v).map(|n| n != 0).unwrap_or(d),
        None => d,
    }
}
fn objects<'a>(o: &'a Map<String, Value>, k: &str) -> impl Iterator<Item = &'a Map<String, Value>> {
    o.get(k)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

/// BRJ-ENC-01
fn decode_text(bytes: &[u8]) -> String {
    if let Some(body) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8_lossy(body).into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

fn parent(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}
fn join(dir: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    if dir.is_empty() {
        rel
    } else {
        format!("{dir}/{rel}")
    }
}

/// BRJ-IF-02
pub fn test_option(cond: &Value, ops: &BTreeSet<i32>) -> bool {
    let one = |v: &Value| -> Option<bool> {
        let n = num(v)?;
        Some(if n >= 0 {
            ops.contains(&(n as i32))
        } else {
            !ops.contains(&(-n as i32))
        })
    };
    match cond {
        Value::Array(items) => items.iter().all(|i| match i {
            Value::Array(any) => any.iter().any(|a| one(a).unwrap_or(false)),
            v => one(v).unwrap_or(false),
        }),
        v => one(v).unwrap_or(false),
    }
}

// ---------------------------------------------------------------------------
// 読み込み
// ---------------------------------------------------------------------------

pub struct Loader<'a> {
    path: PathBuf,
    /// スキンファイルのフォルダ（`/` 区切り）
    dir: String,
    files: &'a mut dyn FileResolver,
    raw: Value,
    diags: Vec<Diagnostic>,
    /// (filepath の path, 選択ファイル名)
    file_choices: Vec<(String, String)>,
}

fn read_json(path: &Path, files: &mut dyn FileResolver) -> Result<Value, SkinError> {
    let bytes = files.read(path).map_err(|message| SkinError::Io {
        path: path.to_path_buf(),
        message,
    })?;
    // BRJ-JSN-01
    lenient::parse(&decode_text(&bytes)).map_err(|e| SkinError::Syntax {
        path: path.to_path_buf(),
        line: e.line,
        message: e.message,
    })
}

impl<'a> Loader<'a> {
    pub fn new(path: &Path, files: &'a mut dyn FileResolver) -> Result<Self, SkinError> {
        let raw = read_json(path, files)?;
        Ok(Self {
            path: path.to_path_buf(),
            dir: parent(&path.to_string_lossy().replace('\\', "/")),
            files,
            raw,
            diags: vec![],
            file_choices: vec![],
        })
    }

    fn diag(&mut self, message: impl Into<String>) {
        self.diags.push(Diagnostic {
            file: self.path.clone(),
            line: 0,
            message: message.into(),
        });
    }

    /// BRJ-CUS-01
    fn substitute(&self, p: &str) -> String {
        for (key, choice) in &self.file_choices {
            if p.starts_with(key.as_str()) {
                if choice.is_empty() || choice.eq_ignore_ascii_case("random") {
                    return p.to_string();
                }
                let head = &p[..p.rfind('*').unwrap_or(0)];
                return format!("{head}{choice}{}", &p[key.len()..]);
            }
        }
        p.to_string()
    }

    fn resolve(&mut self, dir: &str, rel: &str) -> Option<PathBuf> {
        let p = self.substitute(&join(dir, rel));
        self.files.resolve(&p)
    }

    /// 条件分岐と include を展開する（BRJ-IF-01 / BRJ-INC-01）
    fn expand(&mut self, v: &Value, ops: &BTreeSet<i32>, dir: &str, depth: usize) -> Value {
        match v {
            Value::Array(items) => {
                let mut out = vec![];
                for it in items {
                    let Some(o) = it.as_object() else {
                        out.push(self.expand(it, ops, dir, depth));
                        continue;
                    };
                    if let Some(inc) = include_of(o) {
                        match self.include(&inc, ops, dir, depth) {
                            Value::Array(a) => out.extend(a),
                            Value::Null => {}
                            other => out.push(other),
                        }
                    } else if let (Some(cond), true) = (
                        o.get("if"),
                        o.contains_key("value") || o.contains_key("values"),
                    ) {
                        if !test_option(cond, ops) {
                            continue;
                        }
                        if let Some(x) = o.get("value") {
                            out.push(self.expand(x, ops, dir, depth));
                        }
                        if let Some(Value::Array(xs)) = o.get("values") {
                            for x in xs {
                                out.push(self.expand(x, ops, dir, depth));
                            }
                        }
                    } else {
                        out.push(self.expand(it, ops, dir, depth));
                    }
                }
                Value::Array(out)
            }
            Value::Object(o) => {
                if let Some(inc) = include_of(o) {
                    return self.include(&inc, ops, dir, depth);
                }
                Value::Object(
                    o.iter()
                        .map(|(k, x)| (k.clone(), self.expand(x, ops, dir, depth)))
                        .collect(),
                )
            }
            other => other.clone(),
        }
    }

    fn include(&mut self, rel: &str, ops: &BTreeSet<i32>, dir: &str, depth: usize) -> Value {
        if depth + 1 > MAX_INCLUDE_DEPTH {
            self.diag("include が深すぎる");
            return Value::Null;
        }
        let Some(p) = self.resolve(dir, rel) else {
            self.diag(format!("include 先が見つからない: {rel}"));
            return Value::Null;
        };
        match read_json(&p, self.files) {
            Ok(v) => {
                let sub = parent(&p.to_string_lossy().replace('\\', "/"));
                self.expand(&v, ops, &sub, depth + 1)
            }
            Err(e) => {
                self.diag(format!("include 先を読めない: {e:?}"));
                Value::Null
            }
        }
    }

    /// BRJ-HDR-01 / 02（有効な条件は空で展開する）
    pub fn header(&mut self) -> SkinHeader {
        let raw = self.raw.clone();
        let dir = self.dir.clone();
        let root = self.expand(&raw, &BTreeSet::new(), &dir, 0);
        let empty = Map::new();
        let o = root.as_object().unwrap_or(&empty);
        let mut h = SkinHeader {
            kind: i32_or(o, "type", -1),
            title: string(o, "name").unwrap_or_default(),
            maker: string(o, "author").unwrap_or_default(),
            thumbnail: String::new(),
            resolution: (
                int(o, "w").unwrap_or(1280).max(0) as u32,
                int(o, "h").unwrap_or(720).max(0) as u32,
            ),
            customs: vec![],
        };
        for p in objects(o, "property") {
            let items = objects(p, "item")
                .map(|i| (string(i, "name").unwrap_or_default(), i32_or(i, "op", 0)))
                .collect();
            h.customs.push(Custom::Choice {
                name: string(p, "name").unwrap_or_default(),
                items,
                default: string(p, "def"),
            });
        }
        for f in objects(o, "filepath") {
            h.customs.push(Custom::File {
                name: string(f, "name").unwrap_or_default(),
                pattern: string(f, "path").unwrap_or_default().replace('\\', "/"),
                default: string(f, "def").unwrap_or_default(),
            });
        }
        h
    }

    /// BRJ-OPT-01: 有効な条件と、property の op 全体
    fn options(h: &SkinHeader, s: &SkinSettings) -> (BTreeSet<i32>, BTreeSet<i32>) {
        let mut ops = s.options.clone();
        let mut custom = BTreeSet::new();
        for (i, c) in h.customs.iter().enumerate() {
            let Custom::Choice { items, default, .. } = c else {
                continue;
            };
            custom.extend(items.iter().map(|it| it.1));
            let chosen = match s.custom.get(&i) {
                Some(CustomChoice::Option(op)) if items.iter().any(|it| it.1 == *op) => Some(*op),
                _ => default
                    .as_ref()
                    .and_then(|d| items.iter().find(|it| &it.0 == d))
                    .or(items.first())
                    .map(|it| it.1),
            };
            ops.extend(chosen);
        }
        (ops, custom)
    }

    pub fn document(mut self, header: SkinHeader, settings: &SkinSettings) -> SkinDocument {
        let (ops, custom_ops) = Self::options(&header, settings);
        for (i, c) in header.customs.iter().enumerate() {
            if let Custom::File {
                pattern, default, ..
            } = c
            {
                let choice = match settings.custom.get(&i) {
                    Some(CustomChoice::File(f)) => f.clone(),
                    _ => default.clone(),
                };
                self.file_choices.push((join(&self.dir, pattern), choice));
            }
        }
        let raw = self.raw.clone();
        let dir = self.dir.clone();
        let root = self.expand(&raw, &ops, &dir, 0);
        let empty = Map::new();
        let o = root.as_object().unwrap_or(&empty);

        let mut doc = SkinDocument {
            header: header.clone(),
            images: vec![],
            fonts: vec![],
            elements: vec![],
            directives: vec![],
            diagnostics: vec![],
            options: ops.clone(),
        };

        // BRJ-HDR-03
        for k in [
            "fadeout",
            "input",
            "scene",
            "close",
            "loadend",
            "playstart",
            "judgetimer",
            "finishmargin",
        ] {
            if let Some(v) = int(o, k) {
                doc.directives.push(Directive {
                    command: k.to_ascii_uppercase(),
                    args: vec![v.to_string()],
                });
            }
        }

        // BRJ-SRC-01
        let mut sources: HashMap<String, u32> = HashMap::new();
        for s in objects(o, "source") {
            let (Some(id), Some(p)) = (string(s, "id"), string(s, "path")) else {
                continue;
            };
            let entry = match self.resolve(&dir, &p) {
                Some(f) => ImageEntry::File(f),
                None => {
                    self.diag(format!("画像が見つからない: {p}"));
                    ImageEntry::Missing(p.replace('\\', "/"))
                }
            };
            sources.insert(id, doc.images.len() as u32);
            doc.images.push(entry);
        }

        // BRJ-FNT-01
        let mut fonts: HashMap<String, u32> = HashMap::new();
        for f in objects(o, "font") {
            let (Some(id), Some(p)) = (string(f, "id"), string(f, "path")) else {
                continue;
            };
            let entry = match self.resolve(&dir, &p) {
                Some(f) => FontEntry::File(f),
                None => {
                    self.diag(format!("フォントが見つからない: {p}"));
                    FontEntry::Missing(p.replace('\\', "/"))
                }
            };
            fonts.insert(id, doc.fonts.len() as u32);
            doc.fonts.push(entry);
        }

        // 定義の索引（BRJ-OBJ-01 の順に引く）
        let mut defs: HashMap<String, (&'static str, &Map<String, Value>)> = HashMap::new();
        for kind in ["graph", "slider", "text", "value", "image"] {
            // 後から入れた方が優先されるので逆順に入れる
            for d in objects(o, kind) {
                if let Some(id) = string(d, "id") {
                    defs.insert(id, (kind, d));
                }
            }
        }

        let height = header.resolution.1 as f32;
        for d in objects(o, "destination") {
            if let Some(e) = self.element(d, &defs, &sources, &fonts, &ops, &custom_ops, height) {
                doc.elements.push(e);
            }
        }
        doc.diagnostics = self.diags;
        doc
    }

    #[allow(clippy::too_many_arguments)]
    fn element(
        &mut self,
        d: &Map<String, Value>,
        defs: &HashMap<String, (&'static str, &Map<String, Value>)>,
        sources: &HashMap<String, u32>,
        fonts: &HashMap<String, u32>,
        ops: &BTreeSet<i32>,
        custom_ops: &BTreeSet<i32>,
        height: f32,
    ) -> Option<Element> {
        let id = string(d, "id").unwrap_or_default();

        // BRJ-DST-04
        let mut conditions = vec![];
        for v in d.get("op").and_then(Value::as_array).into_iter().flatten() {
            match num(v) {
                Some(0) => {}
                Some(n) if custom_ops.contains(&(n.unsigned_abs() as i32)) => {
                    let on = ops.contains(&(n.unsigned_abs() as i32));
                    if on != (n > 0) {
                        return None;
                    }
                }
                Some(n) => conditions.push(ConditionRef(n as i32)),
                None => self.diag(format!("op の式は未対応（{id}）: {v}")),
            }
        }

        // BRJ-OBJ-01..03
        let source_of = |o: &Map<String, Value>| -> Option<Source> {
            let image = sources.get(&string(o, "src")?).copied()?;
            Some(Source {
                image: ImageRef(image),
                rect: [
                    i32_or(o, "x", 0),
                    i32_or(o, "y", 0),
                    i32_or(o, "w", 0),
                    i32_or(o, "h", 0),
                ],
                divide: (
                    i32_or(o, "divx", 1).max(1) as u32,
                    i32_or(o, "divy", 1).max(1) as u32,
                ),
                cycle_ms: i32_or(o, "cycle", 0),
                timer: int(o, "timer")
                    .filter(|t| *t >= 0)
                    .map(|t| TimerRef(t as i32)),
                params: [0; 5],
            })
        };
        let (command, kind, source) = match defs.get(&id) {
            Some(("image", o)) => ("IMAGE".to_string(), ElementKind::Image, source_of(o)),
            Some(("value", o)) => (
                "NUMBER".into(),
                ElementKind::Number {
                    value: ValueRef(i32_or(o, "ref", 0)),
                    align: i32_or(o, "align", 0),
                    digits: i32_or(o, "digit", 0),
                },
                source_of(o),
            ),
            Some(("text", o)) => (
                "TEXT".into(),
                ElementKind::Text {
                    font: FontRef(
                        string(o, "font")
                            .and_then(|f| fonts.get(&f).copied())
                            .unwrap_or(0),
                    ),
                    value: ValueRef(i32_or(o, "ref", 0)),
                    align: i32_or(o, "align", 0),
                    editable: flag(o, "editable", false),
                },
                None,
            ),
            Some(("slider", o)) => (
                "SLIDER".into(),
                ElementKind::Slider {
                    direction: i32_or(o, "angle", 0),
                    range: i32_or(o, "range", 0),
                    value: ValueRef(i32_or(o, "type", 0)),
                    disabled: !flag(o, "changeable", true),
                },
                source_of(o),
            ),
            Some(("graph", o)) => (
                "BARGRAPH".into(),
                ElementKind::Bargraph {
                    value: ValueRef(i32_or(o, "type", 0)),
                    direction: i32_or(o, "angle", 1),
                },
                source_of(o),
            ),
            _ => (id.clone(), ElementKind::Other, None),
        };

        // BRJ-DST-03
        let timer = match d.get("timer") {
            None | Some(Value::Null) => None,
            Some(v) => match num(v) {
                Some(t) if t >= 0 => Some(TimerRef(t as i32)),
                Some(_) => None,
                None => {
                    self.diag(format!("timer の式は未対応（{id}）: {v}"));
                    None
                }
            },
        };
        // BRJ-BLD-01
        let blend = match i32_or(d, "blend", 0) {
            0 | 1 => Blend::Alpha,
            2 => Blend::Add,
            3 => Blend::Subtract,
            4 => Blend::Multiply,
            9 => Blend::InvertDest,
            other => {
                self.diag(format!("不明な合成 {other}（Alpha として扱う）"));
                Blend::Alpha
            }
        };
        let filter = flag(d, "filter", false);
        let center = i32_or(d, "center", 0).clamp(0, 255) as u8;

        // BRJ-DST-01 / 02
        let mut keyframes: Vec<Keyframe> = vec![];
        let mut prev: Option<[i64; 11]> = None;
        for a in objects(d, "dst") {
            const KEYS: [&str; 11] = [
                "time", "x", "y", "w", "h", "acc", "angle", "a", "r", "g", "b",
            ];
            let mut v = [0i64; 11];
            for (i, k) in KEYS.iter().enumerate() {
                v[i] = int(a, k).unwrap_or(match prev {
                    Some(p) => p[i],
                    None if i >= 7 => 255,
                    None => 0,
                });
            }
            prev = Some(v);
            let byte = |x: i64| x.clamp(0, 255) as u8;
            let kf = Keyframe {
                time_ms: v[0] as i32,
                rect: [
                    v[1] as f32,
                    height - (v[2] + v[4]) as f32,
                    v[3] as f32,
                    v[4] as f32,
                ],
                accel: v[5] as i32,
                angle: v[6] as f32,
                argb: [byte(v[7]), byte(v[8]), byte(v[9]), byte(v[10])],
                blend,
                filter,
                center,
            };
            let at = keyframes.partition_point(|k| k.time_ms <= kf.time_ms);
            keyframes.insert(at, kf);
        }

        Some(Element {
            command,
            index: 0,
            kind,
            source,
            timer,
            conditions,
            looping: Some(i32_or(d, "loop", 0)),
            dst_extra: [0; 2],
            order: 0,
            keyframes,
        })
    }
}

fn include_of(o: &Map<String, Value>) -> Option<String> {
    (o.len() == 1)
        .then(|| o.get("include").and_then(Value::as_str).map(str::to_string))
        .flatten()
}
