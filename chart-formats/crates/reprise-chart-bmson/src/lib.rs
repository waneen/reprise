//! bmson（JSON）デコーダ。
//!
//! 根拠は bmson format specification 1.0.0（<https://bmson-spec.readthedocs.io/en/master/doc/>）。
//! 位置はパルス（`resolution` = 4 分音符あたりのパルス数）で書かれているので、小節線（`lines`）で
//! [`Position`]（小節 + 小節内比率）に直し、小節線の間隔が 4/4 と違う小節は小節長として出す。
//!
//! 扱わないもの: ノーツの `c`（続きから鳴らす）は保持しない（常に頭から鳴らす扱い）。
//! `subartists`・`title_image` は読み捨てる。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | BMSON-JSN-01 | UTF-8 の JSON（BOM 可）。JSON として読めなければ `Syntax` エラー（行番号付き） |
//! | BMSON-VER-01 | `version` が無い（旧形式）・null・メジャー 1 以外ならエラー |
//! | BMSON-INF-01 | `info` の題名・副題・作者・ジャンル・譜面名・レベル・画像/プレビューをメタ情報にする |
//! | BMSON-INF-02 | `init_bpm` は必須（無ければエラー） |
//! | BMSON-INF-03 | `judge_rank`（既定 100）・`total`（既定 100、負は絶対値）は百分率としてメタ情報にする |
//! | BMSON-RES-01 | `resolution` は既定 240。0・null も 240、負は絶対値 |
//! | BMSON-POS-01 | 小節線は `lines` の `y`（無い・空なら 4/4 = 4×resolution ごと）。0 は常に小節の頭。最後の小節線より後は 4/4 で続ける |
//! | BMSON-POS-02 | 小節線の間隔が 4×resolution と違う小節は小節長（間隔 ÷ 4×resolution）を出す |
//! | BMSON-MOD-01 | `mode_hint` → 鍵盤構成: beat-5k/7k/10k/14k、popn-5k/9k（既定 beat-7k）。それ以外はエラー |
//! | BMSON-MOD-02 | レーン: beat は 1..=7→鍵盤、8→1P 皿、9..=15→2P 鍵盤、16→2P 皿。popn は 1..=9。範囲外は診断して捨てる |
//! | BMSON-NOT-01 | `x` が 0・null・無しは BGM、それ以外は鍵盤ノーツ。音は所属する `sound_channels` |
//! | BMSON-NOT-02 | `l > 0` は始点 `y`・終点 `y+l` の LN |
//! | BMSON-SND-01 | `sound_channels[i].name` は音定義 i+1。絶対パス・`..`・NUL を含むものは診断してそのチャンネルごと捨てる |
//! | BMSON-BPM-01 | `bpm_events` は BPM 変化（同じ位置は文書順＝後勝ち） |
//! | BMSON-STP-01 | `stop_events` の `duration` はパルス。停止長は `duration ÷ resolution × 48`（1/192 小節単位） |
//! | BMSON-BGA-01 | `bga_header` は画像定義、`bga_events`/`layer_events`/`poor_events` は Base/Layer/Poor の BGA |
//! | BMSON-ORD-01 | 同じ位置ではノーツ・BGA → BPM → STOP の順に並べる |

use reprise_chart_core::*;
use serde_json::{Map, Value};

pub struct BmsonDecoder;

impl ChartDecoder for BmsonDecoder {
    fn name(&self) -> &'static str {
        "bmson"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["bmson"]
    }
    fn decode(
        &self,
        bytes: &[u8],
        _hint: &DecodeHint,
    ) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
        let root: Value = serde_json::from_slice(bytes).map_err(|e| DecodeError::Syntax {
            line: e.line(),
            message: e.to_string(),
        })?;
        let root = root
            .as_object()
            .ok_or_else(|| DecodeError::Other("最上位が JSON オブジェクトではない".into()))?;
        Builder::default().build(root)
    }
}

fn err(m: impl Into<String>) -> DecodeError {
    DecodeError::Other(m.into())
}

fn arr<'a>(o: &'a Map<String, Value>, k: &str) -> &'a [Value] {
    o.get(k)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn int(v: &Value, k: &str) -> Option<i64> {
    v.get(k)
        .and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
}
fn text(o: &Map<String, Value>, k: &str) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(str::to_string)
}

/// BMSON-MOD-01
fn layout_of(mode: &str) -> Option<KeyLayout> {
    Some(match mode {
        "beat-5k" => KeyLayout::Beat5,
        "beat-7k" => KeyLayout::Beat7,
        "beat-10k" => KeyLayout::Beat10,
        "beat-14k" => KeyLayout::Beat14,
        "popn-5k" | "popn-9k" => KeyLayout::Popn9,
        _ => return None,
    })
}

/// BMSON-MOD-02
fn lane_of(mode: &str, x: i64) -> Option<Lane> {
    let keys: i64 = match mode {
        "beat-5k" | "beat-10k" | "popn-5k" => 5,
        "beat-7k" | "beat-14k" => 7,
        "popn-9k" => 9,
        _ => return None,
    };
    let double = matches!(mode, "beat-10k" | "beat-14k");
    let lane = if mode.starts_with("popn") {
        (1..=keys).contains(&x).then_some(x)?
    } else if (1..=keys).contains(&x) {
        x
    } else if x == 8 {
        0
    } else if double && (9..9 + keys).contains(&x) {
        x - 8 + 10
    } else if double && x == 16 {
        10
    } else {
        return None;
    };
    Some(Lane(lane as u8))
}

/// BMSON-SND-01
fn safe_path(p: &str) -> bool {
    let b = p.as_bytes();
    let absolute = p.starts_with('/')
        || p.starts_with('\\')
        || (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic());
    let parent = p.split(['/', '\\']).any(|c| c == "..");
    !(absolute || parent || p.contains('\0'))
}

/// パルス → 位置（BMSON-POS-01/02）
struct Grid {
    /// 各小節の頭のパルス（最後の要素は番兵）
    starts: Vec<i64>,
    measure: i64,
}

impl Grid {
    fn new(lines: Option<&[Value]>, resolution: i64, max_y: i64) -> Self {
        let measure = 4 * resolution;
        let mut starts: Vec<i64> = lines
            .unwrap_or(&[])
            .iter()
            .filter_map(|l| int(l, "y"))
            .filter(|y| *y >= 0)
            .collect();
        starts.push(0);
        starts.sort_unstable();
        starts.dedup();
        while *starts.last().unwrap() <= max_y {
            let next = starts.last().unwrap() + measure;
            starts.push(next);
        }
        Self { starts, measure }
    }
    fn pos(&self, y: i64) -> Position {
        let i = self.starts.partition_point(|s| *s <= y) - 1;
        let (a, b) = (self.starts[i], self.starts[i + 1]);
        Position::new(i as u32, (y - a) as f64 / (b - a) as f64)
    }
    fn lengths(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.starts.windows(2).enumerate().filter_map(|(i, w)| {
            let len = w[1] - w[0];
            (len != self.measure).then(|| (i as u32, len as f64 / self.measure as f64))
        })
    }
}

#[derive(Default)]
struct Builder {
    stmts: Vec<Statement>,
    diags: Vec<Diagnostic>,
}

impl Builder {
    fn meta(&mut self, m: MetaField) {
        self.stmts.push(Statement::Define(Definition::Meta(m)));
    }

    fn build(
        mut self,
        root: &Map<String, Value>,
    ) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        // BMSON-VER-01
        match root.get("version") {
            None => return Err(err("version が無い旧形式の bmson は未対応")),
            Some(Value::String(v)) if v.split('.').next() == Some("1") => {}
            Some(v) => return Err(err(format!("未対応の version: {v}"))),
        }
        let info = root
            .get("info")
            .and_then(Value::as_object)
            .ok_or_else(|| err("info が無い"))?;

        // BMSON-INF-02
        let init_bpm = info
            .get("init_bpm")
            .and_then(Value::as_f64)
            .ok_or_else(|| err("init_bpm が無い"))?;
        // BMSON-MOD-01
        let mode = text(info, "mode_hint").unwrap_or_else(|| "beat-7k".into());
        let layout = layout_of(&mode).ok_or_else(|| err(format!("未対応の mode_hint: {mode}")))?;
        // BMSON-RES-01
        let resolution = match info.get("resolution").and_then(Value::as_i64) {
            None | Some(0) => 240,
            Some(r) => r.abs(),
        };

        // BMSON-INF-01 / 03
        self.meta(MetaField::InitialBpm(init_bpm));
        for (k, f) in [
            ("title", MetaField::Title as fn(String) -> MetaField),
            ("subtitle", MetaField::Subtitle),
            ("artist", MetaField::Artist),
            ("genre", MetaField::Genre),
            ("chart_name", MetaField::ChartName),
        ] {
            if let Some(v) = text(info, k) {
                self.meta(f(v));
            }
        }
        for (k, f) in [
            ("back_image", MetaField::BackBmp as fn(String) -> MetaField),
            ("eyecatch_image", MetaField::StageFile),
            ("banner_image", MetaField::Banner),
            ("preview_music", MetaField::Preview),
        ] {
            if let Some(v) = text(info, k).filter(|v| !v.is_empty()) {
                self.meta(f(v));
            }
        }
        if let Some(l) = info.get("level").and_then(Value::as_i64) {
            self.meta(MetaField::PlayLevel(l as i32));
        }
        let judge = info
            .get("judge_rank")
            .and_then(Value::as_f64)
            .unwrap_or(100.0);
        self.meta(MetaField::JudgeRankPercent(judge));
        let total = info.get("total").and_then(Value::as_f64).unwrap_or(100.0);
        self.meta(MetaField::TotalPercent(total.abs()));

        // 位置の範囲
        let channels = arr(root, "sound_channels");
        let bga = root.get("bga").and_then(Value::as_object);
        let mut max_y = 0;
        let mut see = |v: &Value| {
            let y = int(v, "y").unwrap_or(0);
            max_y = max_y.max(y + int(v, "l").unwrap_or(0).max(0));
        };
        for c in channels {
            c.get("notes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .for_each(&mut see);
        }
        arr(root, "bpm_events").iter().for_each(&mut see);
        arr(root, "stop_events").iter().for_each(&mut see);
        if let Some(b) = bga {
            for k in ["bga_events", "layer_events", "poor_events"] {
                arr(b, k).iter().for_each(&mut see);
            }
        }
        let lines = root
            .get("lines")
            .and_then(Value::as_array)
            .map(Vec::as_slice);
        let grid = Grid::new(lines, resolution, max_y);
        for (measure, ratio) in grid.lengths() {
            self.stmts
                .push(Statement::Event(RawEvent::MeasureLength { measure, ratio }));
        }

        // (y, 種別順, 文) を集めて並べる（BMSON-ORD-01）
        let mut events: Vec<(i64, u8, Statement)> = vec![];

        // BMSON-SND-01 / NOT-01 / NOT-02
        for (i, c) in channels.iter().enumerate() {
            let id = ObjId(i as u32 + 1);
            let name = c.get("name").and_then(Value::as_str).unwrap_or("");
            if !safe_path(name) {
                self.diags.push(Diagnostic::global(format!(
                    "不正な音声パスのチャンネルを捨てる: {name}"
                )));
                continue;
            }
            self.stmts
                .push(Statement::Define(Definition::Sound(id, name.to_string())));
            for n in c
                .get("notes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let y = int(n, "y").unwrap_or(0);
                let pos = grid.pos(y);
                let x = int(n, "x").unwrap_or(0);
                let l = int(n, "l").unwrap_or(0);
                let ev = if x == 0 {
                    RawEvent::Bgm { pos, id }
                } else {
                    let Some(lane) = lane_of(&mode, x) else {
                        self.diags.push(Diagnostic::global(format!(
                            "{mode} に無いレーン x={x} のノーツを捨てる"
                        )));
                        continue;
                    };
                    if l > 0 {
                        RawEvent::Long {
                            pos,
                            end: grid.pos(y + l),
                            lane,
                            id,
                        }
                    } else {
                        RawEvent::Note {
                            pos,
                            lane,
                            object: RawObject::Normal,
                            id,
                        }
                    }
                };
                events.push((y, 0, Statement::Event(ev)));
            }
        }

        // BMSON-BGA-01
        if let Some(b) = bga {
            for h in arr(b, "bga_header") {
                if let (Some(id), Some(name)) =
                    (int(h, "id"), h.get("name").and_then(Value::as_str))
                {
                    self.stmts.push(Statement::Define(Definition::Image(
                        ObjId(id as u32),
                        name.to_string(),
                    )));
                }
            }
            for (k, layer) in [
                ("bga_events", BgaLayer::Base),
                ("layer_events", BgaLayer::Layer),
                ("poor_events", BgaLayer::Poor),
            ] {
                for e in arr(b, k) {
                    let y = int(e, "y").unwrap_or(0);
                    let id = ObjId(int(e, "id").unwrap_or(0) as u32);
                    let ev = RawEvent::Bga {
                        pos: grid.pos(y),
                        layer,
                        id,
                    };
                    events.push((y, 0, Statement::Event(ev)));
                }
            }
        }

        // BMSON-BPM-01
        for e in arr(root, "bpm_events") {
            let y = int(e, "y").unwrap_or(0);
            if let Some(bpm) = e.get("bpm").and_then(Value::as_f64) {
                let ev = RawEvent::Bpm {
                    pos: grid.pos(y),
                    bpm,
                };
                events.push((y, 1, Statement::Event(ev)));
            }
        }

        // BMSON-STP-01
        for (i, e) in arr(root, "stop_events").iter().enumerate() {
            let y = int(e, "y").unwrap_or(0);
            let d = int(e, "duration").unwrap_or(0);
            if d <= 0 {
                continue;
            }
            let id = ObjId(i as u32 + 1);
            self.stmts.push(Statement::Define(Definition::Stop(
                id,
                d as f64 / resolution as f64 * 48.0,
            )));
            let ev = RawEvent::StopRef {
                pos: grid.pos(y),
                id,
            };
            events.push((y, 2, Statement::Event(ev)));
        }

        events.sort_by_key(|(y, k, _)| (*y, *k));
        self.stmts.extend(events.into_iter().map(|e| e.2));

        Ok((
            ChartDocument {
                layout,
                policy: ResolvePolicy::default(),
                body: vec![Block::Statements(self.stmts)],
            },
            self.diags,
        ))
    }
}

#[cfg(test)]
mod tests;
