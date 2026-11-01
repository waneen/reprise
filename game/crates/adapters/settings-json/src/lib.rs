//! 設定を JSON ファイルに置く（`SettingsStore` の実装）。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | SJS-01 | ファイルが無ければ None |
//! | SJS-02 | 保存した設定は読み直すと同じ（オートプレイは保存しない） |
//! | SJS-03 | 書かれていない項目は既定値、知らない項目は無視。操作の割り当ては書かれた操作だけ置き換える |
//! | SJS-04 | JSON として読めない・値の種類が違う・知らない名前（ゲージ・HS-FIX・物理入力の種類）は `Parse` エラー |
//! | SJS-05 | 選曲スキンは `select_skin`（パス。null・無しは None） |
//! | SJS-06 | キー設定スキンは `keyconfig_skin`（パス。null・無しは None） |
//! | SJS-07 | 操作の割り当ては `keys` に操作の名前（2P は `p2_` を付ける）ごとの配列で、配列の位置が割り当ての位置（LR2 の 16 か所。空いた位置は null。16 番目より後は無視） |
use application::ports::{PortError, SettingsStore};
use application::settings::Settings;
use domain::input::{Side, Source, PLACES, SLOTS};
use domain::play::{GaugeType, HsFix};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// 操作の名前（`domain::input` の操作番号の順）
const SLOT_NAMES: [&str; SLOTS] = [
    "scratch_up",
    "scratch_down",
    "key1",
    "key2",
    "key3",
    "key4",
    "key5",
    "key6",
    "key7",
    "start",
    "select",
];

const GAUGES: [(&str, GaugeType); 6] = [
    ("groove", GaugeType::Groove),
    ("easy", GaugeType::Easy),
    ("hard", GaugeType::Hard),
    ("death", GaugeType::Hazard),
    ("pattack", GaugeType::PAttack),
    ("gattack", GaugeType::GAttack),
];

const HS_FIX: [(&str, HsFix); 6] = [
    ("off", HsFix::Off),
    ("max", HsFix::MaxBpm),
    ("min", HsFix::MinBpm),
    ("avg", HsFix::Average),
    ("const", HsFix::Constant),
    ("main", HsFix::MainBpm),
];

pub struct JsonSettingsFile {
    pub path: PathBuf,
}

fn err(what: &str) -> PortError {
    PortError::Parse(format!("設定ファイル: {what}"))
}

fn name_of<T: PartialEq + Copy>(table: &[(&'static str, T)], v: T) -> &'static str {
    table.iter().find(|(_, t)| *t == v).map_or("", |(n, _)| n)
}

fn value_of<T: Copy>(table: &[(&str, T)], v: &Value, what: &str) -> Result<T, PortError> {
    let s = v.as_str().ok_or_else(|| err(what))?;
    table
        .iter()
        .find(|(n, _)| *n == s)
        .map(|(_, t)| *t)
        .ok_or_else(|| err(&format!("{what}: {s}")))
}

fn source_json(s: &Option<Source>) -> Value {
    let Some(s) = s else {
        return Value::Null;
    };
    match s {
        Source::Key(k) => json!({ "key": k }),
        Source::Button(b) => json!({ "button": b }),
        Source::Axis { axis, plus } => json!({ "axis": axis, "plus": plus }),
    }
}

fn source_of(v: &Value) -> Result<Option<Source>, PortError> {
    if v.is_null() {
        return Ok(None);
    }
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    if let Some(k) = text("key") {
        Ok(Some(Source::Key(k)))
    } else if let Some(b) = text("button") {
        Ok(Some(Source::Button(b)))
    } else if let Some(axis) = text("axis") {
        let plus = v
            .get("plus")
            .and_then(Value::as_bool)
            .ok_or_else(|| err("plus"))?;
        Ok(Some(Source::Axis { axis, plus }))
    } else {
        Err(err("物理入力の種類"))
    }
}

/// 設定 → JSON
pub fn to_json(s: &Settings) -> Value {
    let o = &s.options;
    let b = &s.bindings;
    let keys: Map<String, Value> = [(Side::P1, ""), (Side::P2, "p2_")]
        .into_iter()
        .flat_map(|(side, prefix)| {
            let list = if side == Side::P1 { &b.slots } else { &b.p2 };
            SLOT_NAMES
                .iter()
                .zip(list)
                .map(move |(n, v)| (format!("{prefix}{n}"), v.iter().map(source_json).collect()))
        })
        .collect();
    json!({
        "songs": s.songs,
        "skin": s.skin,
        "select_skin": s.select_skin,
        "keyconfig_skin": s.keyconfig_skin,
        "skin_root": s.skin_root,
        "gauge": name_of(&GAUGES, s.gauge),
        "hi_speed": o.hi_speed,
        "hs_fix": name_of(&HS_FIX, o.hs_fix),
        "lane_cover": o.lane_cover,
        "cover": o.cover,
        "judge_offset": o.judge_offset,
        "analog_scratch_ms": s.bindings.analog_scratch_ms,
        "keys": keys,
    })
}

/// JSON → 設定（SJS-03, 04）
pub fn from_json(v: &Value) -> Result<Settings, PortError> {
    let mut s = Settings::default();
    let o = v.as_object().ok_or_else(|| err("全体"))?;
    let path = |v: &Value| v.as_str().map(PathBuf::from).ok_or_else(|| err("パス"));
    let int = |v: &Value, what| v.as_i64().map(|n| n as i32).ok_or_else(|| err(what));
    for (k, v) in o {
        match k.as_str() {
            "songs" => {
                s.songs = v
                    .as_array()
                    .ok_or_else(|| err("songs"))?
                    .iter()
                    .map(path)
                    .collect::<Result<_, _>>()?
            }
            "skin" if !v.is_null() => s.skin = Some(path(v)?),
            "skin_root" if !v.is_null() => s.skin_root = Some(path(v)?),
            "select_skin" if !v.is_null() => s.select_skin = Some(path(v)?),
            "keyconfig_skin" if !v.is_null() => s.keyconfig_skin = Some(path(v)?),
            "gauge" => s.gauge = value_of(&GAUGES, v, "gauge")?,
            "hi_speed" => s.options.hi_speed = int(v, "hi_speed")?,
            "hs_fix" => s.options.hs_fix = value_of(&HS_FIX, v, "hs_fix")?,
            "lane_cover" => s.options.lane_cover = v.as_bool().ok_or_else(|| err("lane_cover"))?,
            "cover" => s.options.cover = int(v, "cover")?,
            "judge_offset" => s.options.judge_offset = int(v, "judge_offset")?,
            "analog_scratch_ms" if !v.is_null() => {
                s.bindings.analog_scratch_ms =
                    Some(v.as_u64().ok_or_else(|| err("analog_scratch_ms"))? as u32)
            }
            "keys" => {
                let keys = v.as_object().ok_or_else(|| err("keys"))?;
                for (side, prefix) in [(Side::P1, ""), (Side::P2, "p2_")] {
                    for (i, name) in SLOT_NAMES.iter().enumerate() {
                        let name = format!("{prefix}{name}");
                        let Some(list) = keys.get(&name) else {
                            continue;
                        };
                        let list = list.as_array().ok_or_else(|| err(&name))?;
                        for p in 0..PLACES {
                            s.bindings.set(side, i, p, None);
                        }
                        for (p, v) in list.iter().enumerate().take(PLACES) {
                            s.bindings.set(side, i, p, source_of(v)?);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    s.options = s.options.clamped();
    Ok(s)
}

impl SettingsStore for JsonSettingsFile {
    fn load(&self) -> Result<Option<Settings>, PortError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(PortError::Io(format!("{}: {e}", self.path.display()))),
        };
        let v: Value =
            serde_json::from_str(&text).map_err(|e| err(&format!("JSON として読めない: {e}")))?;
        from_json(&v).map(Some)
    }
    fn save(&self, settings: &Settings) -> Result<(), PortError> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| PortError::Io(e.to_string()))?;
        }
        let text = serde_json::to_string_pretty(&to_json(settings)).unwrap_or_default();
        std::fs::write(&self.path, text)
            .map_err(|e| PortError::Io(format!("{}: {e}", self.path.display())))
    }
}

#[cfg(test)]
mod tests;
