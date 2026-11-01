//! 難易度表（beatoraja の難易度表。reprise の拡張）。`TableSource` の実装。
//! 読み方は beatoraja が使う jbmstable-parser（`DifficultyTableParser`）と beatoraja の `TableDataAccessor`
//! （`DifficultyTableAccessor.read`）に合わせた: 表の HTML の `<meta name="bmstable" content="…">` でヘッダの JSON を、
//! ヘッダの `data_url` でデータの JSON を引き、レベルごとのフォルダ（`tag`（無ければ `symbol`）+ レベル）にする。
//! 取ってきた表は手元の `<フォルダ>/<URL の SHA-256>.json`（reprise の形）に置く。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | BT-01 | ヘッダの URL: `<meta` を含む行の `name` 属性（大文字小文字を無視）が `bmstable` なら `content` 属性。属性の値は `"` か `'` で囲む。いくつもあれば最後のもの |
//! | BT-02 | ヘッダ: `name` と `symbol` が文字列でなければ誤り。`tag`（文字列。無ければ `symbol`）、`data_url`（文字列か文字列の配列）、`data_rule`（`data_url` と同じ順の { レベル: 新しいレベル }）、`level_order`（配列。数は文字にする） |
//! | BT-03 | データ: 配列でなければ誤り。`title`・`mode`・`md5`・`sha256` が文字列でも null でもない行は飛ばし、`level` があり `md5` か `sha256` が 25 文字以上の行だけ取る。レベルは文字列ならそのまま、ほかは JSON の書き方。MD5・SHA-256 は小文字 |
//! | BT-04 | 表を作る: データを `data_url` の順に足し、前に足した行と MD5 か SHA-256（11 文字以上）が同じ行は飛ばす。`data_rule` にあるレベルは新しいレベルに（空なら飛ばす）。レベルの順は `level_order`、無ければデータに出てきた順（置き換える前のレベル。同じものは 1 つ）。レベルごとに行の順で譜面を並べ、`level_order` に無いレベルの行は出さない |
//! | BT-05 | 取ってくる: URL が `.json` で終わればヘッダ、ほかは HTML（BT-01。相対 URL は HTML の URL から）。データの URL はヘッダの URL から。取れないデータは飛ばし、1 つも取れなければ誤り。取れた表は手元に保存し、手元の表は URL ごとに読み戻せる |
use application::ports::{PortError, TableSource};
use domain::select::{DifficultyTable, TableChart, TableLevel};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn parse_err(what: &str) -> PortError {
    PortError::Parse(format!("難易度表: {what}"))
}

/// `name="value"`・`name='value'`（`name` の前は行頭か空白。大文字小文字を無視）
fn attr(line: &str, name: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    let b = line.as_bytes();
    let mut from = 0;
    while let Some(i) = lower[from..].find(&name).map(|i| i + from) {
        from = i + 1;
        if i > 0 && !b[i - 1].is_ascii_whitespace() {
            continue;
        }
        let mut j = i + name.len();
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b'=' {
            continue;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || !(b[j] == b'"' || b[j] == b'\'') {
            continue;
        }
        let q = b[j] as char;
        if let Some(end) = line[j + 1..].find(q) {
            return Some(line[j + 1..j + 1 + end].to_string());
        }
    }
    None
}

/// BT-01: 表の HTML からヘッダの URL
pub fn header_url(html: &str) -> Option<String> {
    let mut found = None;
    for line in html.lines() {
        if !line.to_ascii_lowercase().contains("<meta") {
            continue;
        }
        if attr(line, "name").is_some_and(|n| n.eq_ignore_ascii_case("bmstable")) {
            if let Some(c) = attr(line, "content") {
                found = Some(c);
            }
        }
    }
    found
}

/// 表のヘッダ（BT-02）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    pub name: String,
    pub symbol: String,
    pub tag: String,
    pub data_url: Vec<String>,
    /// `data_url` と同じ順（レベル → 新しいレベル）
    pub data_rule: Vec<Vec<(String, String)>>,
    pub level_order: Vec<String>,
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// BT-02: ヘッダの JSON
pub fn parse_header(text: &str) -> Result<Header, PortError> {
    let v: Value = serde_json::from_str(text).map_err(|e| parse_err(&e.to_string()))?;
    let o = v
        .as_object()
        .ok_or_else(|| parse_err("ヘッダがオブジェクトでない"))?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    let (Some(name), Some(symbol)) = (s("name"), s("symbol")) else {
        return Err(parse_err("ヘッダに name と symbol が無い"));
    };
    let data_url = match o.get("data_url") {
        Some(Value::String(u)) => vec![u.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    };
    let data_rule = match o.get("data_rule") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|r| {
                r.as_object().map_or(vec![], |m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .collect()
                })
            })
            .collect(),
        _ => vec![],
    };
    let level_order = match o.get("level_order") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    text_of(x)
                }
            })
            .collect(),
        _ => vec![],
    };
    Ok(Header {
        tag: s("tag").unwrap_or_else(|| symbol.clone()),
        name,
        symbol,
        data_url,
        data_rule,
        level_order,
    })
}

/// データの 1 行（BT-03）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub level: String,
    pub chart: TableChart,
}

/// BT-03: データの JSON
pub fn parse_data(text: &str) -> Result<Vec<Row>, PortError> {
    let v: Value = serde_json::from_str(text).map_err(|e| parse_err(&e.to_string()))?;
    let a = v
        .as_array()
        .ok_or_else(|| parse_err("データが配列でない"))?;
    let mut out = vec![];
    for m in a {
        let Some(m) = m.as_object() else {
            continue;
        };
        let str_or_null =
            |k: &str| matches!(m.get(k), None | Some(Value::Null) | Some(Value::String(_)));
        if !["title", "mode", "md5", "sha256"]
            .iter()
            .all(|k| str_or_null(k))
        {
            continue;
        }
        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (md5, sha256) = (s("md5"), s("sha256"));
        let level = match m.get("level") {
            None | Some(Value::Null) => continue,
            Some(l) => text_of(l),
        };
        if md5.len() <= 24 && sha256.len() <= 24 {
            continue;
        }
        out.push(Row {
            level,
            chart: TableChart {
                md5: md5.to_ascii_lowercase(),
                sha256: sha256.to_ascii_lowercase(),
                title: s("title"),
                artist: s("artist"),
            },
        });
    }
    Ok(out)
}

/// BT-04: ヘッダと `data_url` の順のデータから表を作る
pub fn build(url: &str, header: &Header, datas: &[Option<Vec<Row>>]) -> DifficultyTable {
    let mut rows: Vec<Row> = vec![];
    let mut seen_levels: Vec<String> = vec![];
    for (i, data) in datas.iter().enumerate() {
        let Some(data) = data else {
            continue;
        };
        let rule = header.data_rule.get(i);
        for r in data {
            // データに出てきた順（jbmstable-parser はデータごとの全ての行のレベルを数える）
            if !seen_levels.contains(&r.level) {
                seen_levels.push(r.level.clone());
            }
            let to = rule.and_then(|m| m.iter().find(|(k, _)| *k == r.level).map(|(_, v)| v));
            if to.is_some_and(|t| t.is_empty()) {
                continue;
            }
            let dup = rows.iter().any(|x| {
                (r.chart.md5.len() > 10 && r.chart.md5 == x.chart.md5)
                    || (r.chart.sha256.len() > 10 && r.chart.sha256 == x.chart.sha256)
            });
            if dup {
                continue;
            }
            let mut r = r.clone();
            if let Some(t) = to {
                r.level = t.clone();
            }
            rows.push(r);
        }
    }
    let order = if header.level_order.is_empty() {
        seen_levels
    } else {
        header.level_order.clone()
    };
    DifficultyTable {
        url: url.to_string(),
        name: header.name.clone(),
        levels: order
            .iter()
            .map(|lv| TableLevel {
                name: format!("{}{lv}", header.tag),
                charts: rows
                    .iter()
                    .filter(|r| &r.level == lv)
                    .map(|r| r.chart.clone())
                    .collect(),
            })
            .collect(),
    }
}

fn to_json(t: &DifficultyTable) -> Value {
    json!({
        "url": t.url,
        "name": t.name,
        "levels": t.levels.iter().map(|l| json!({
            "name": l.name,
            "charts": l.charts.iter().map(|c| json!({
                "md5": c.md5, "sha256": c.sha256, "title": c.title, "artist": c.artist,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn from_json(v: &Value) -> Option<DifficultyTable> {
    let empty = Map::new();
    let s = |o: &Map<String, Value>, k: &str| {
        o.get(k).and_then(Value::as_str).unwrap_or("").to_string()
    };
    let o = v.as_object()?;
    Some(DifficultyTable {
        url: s(o, "url"),
        name: s(o, "name"),
        levels: o
            .get("levels")?
            .as_array()?
            .iter()
            .map(|l| {
                let l = l.as_object().unwrap_or(&empty);
                TableLevel {
                    name: s(l, "name"),
                    charts: l
                        .get("charts")
                        .and_then(Value::as_array)
                        .map_or(vec![], |a| {
                            a.iter()
                                .map(|c| {
                                    let c = c.as_object().unwrap_or(&empty);
                                    TableChart {
                                        md5: s(c, "md5"),
                                        sha256: s(c, "sha256"),
                                        title: s(c, "title"),
                                        artist: s(c, "artist"),
                                    }
                                })
                                .collect()
                        }),
                }
            })
            .collect(),
    })
}

/// 難易度表を HTTP で取ってきて、手元のフォルダに置く
pub struct HttpTables {
    dir: PathBuf,
    agent: ureq::Agent,
}

/// 表の HTML・JSON の大きさの上限
const LIMIT: u64 = 64 * 1024 * 1024;

fn resolve(base: &str, rel: &str) -> Result<String, PortError> {
    let b = url::Url::parse(base).map_err(|e| parse_err(&e.to_string()))?;
    b.join(rel)
        .map(|u| u.to_string())
        .map_err(|e| parse_err(&e.to_string()))
}

impl HttpTables {
    /// `dir` は手元に置くフォルダ（環境変数のプロキシを使う）
    pub fn new(dir: PathBuf) -> Self {
        Self::with_agent(dir, ureq::Agent::new_with_defaults())
    }
    pub fn with_agent(dir: PathBuf, agent: ureq::Agent) -> Self {
        Self { dir, agent }
    }
    fn file(&self, url: &str) -> PathBuf {
        let h = Sha256::digest(url.as_bytes());
        let name: String = h.iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join(format!("{name}.json"))
    }
    fn get(&self, url: &str) -> Result<String, PortError> {
        let io = |e: ureq::Error| PortError::Io(format!("{url}: {e}"));
        let mut res = self.agent.get(url).call().map_err(io)?;
        let bytes = res
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_vec()
            .map_err(io)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl TableSource for HttpTables {
    fn cached(&self, url: &str) -> Option<DifficultyTable> {
        let text = std::fs::read_to_string(self.file(url)).ok()?;
        from_json(&serde_json::from_str(&text).ok()?)
    }

    /// BT-05
    fn fetch(&self, url: &str) -> Result<DifficultyTable, PortError> {
        let head_url = if url.ends_with(".json") {
            url.to_string()
        } else {
            let html = self.get(url)?;
            let rel = header_url(&html).ok_or_else(|| parse_err("bmstable が無い"))?;
            resolve(url, &rel)?
        };
        let header = parse_header(&self.get(&head_url)?)?;
        let datas: Vec<Option<Vec<Row>>> = header
            .data_url
            .iter()
            .map(|d| {
                if d.trim().is_empty() {
                    return None;
                }
                let u = resolve(&head_url, d).ok()?;
                parse_data(&self.get(&u).ok()?).ok()
            })
            .collect();
        if datas.iter().all(Option::is_none) {
            return Err(parse_err("データを 1 つも取れない"));
        }
        let table = build(url, &header, &datas);
        std::fs::create_dir_all(&self.dir).map_err(|e| PortError::Io(e.to_string()))?;
        std::fs::write(self.file(url), to_json(&table).to_string())
            .map_err(|e| PortError::Io(e.to_string()))?;
        Ok(table)
    }
}

#[cfg(test)]
mod tests;
