//! LR2 のスコア DB（`LR2files/Database/Score/<プレイヤー名>.db`、sqlite）からの取り込み。
//! LR2 固有の clear 値 → `ClearLamp` の写像はここだけが知る。
//!
//! 根拠: OpenLR2 `LR2/LR2_songmanage.cpp`（`score` テーブル定義）、`LR2/LR2_customir_api.h`（ランプ値）、
//! `LR2/LR2_statlong.cpp`（各列の書き込み）。コードは転載していない。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | IMP-LR2-01 | `score` テーブルを読み取り専用で開く。開けない・テーブルが無ければ `Io` エラー |
//! | IMP-LR2-02 | `hash` は譜面ファイルの MD5（16 進 32 文字）。長さが違うもの（コース）と 16 進でないものは取り込まない |
//! | IMP-LR2-03 | clear: 0=NoPlay 1=Failed 2=Easy 3=Normal 4=Hard 5=FullCombo、それ以外は NoPlay |
//! | IMP-LR2-04 | 判定内訳は perfect/great/good/bad/poor をそのまま、EX スコア = perfect×2 + great |
//! | IMP-LR2-05 | maxcombo・playcount はそのまま（負は 0）、minbp は負なら不明（None） |
//! | IMP-LR2-06 | ルールは `Lr2`、出自は `ImportedFrom("LR2")`、SHA-256 は不明（None） |
use application::ports::{PortError, ScoreImporter};
use domain::chart::ChartHash;
use domain::play::RulesetId;
use domain::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub struct Lr2ScoreDb;

/// IMP-LR2-03
pub fn lamp(clear: i64) -> ClearLamp {
    match clear {
        1 => ClearLamp::Failed,
        2 => ClearLamp::Easy,
        3 => ClearLamp::Normal,
        4 => ClearLamp::Hard,
        5 => ClearLamp::FullCombo,
        _ => ClearLamp::NoPlay,
    }
}

/// IMP-LR2-02
fn md5_of(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn count(v: i64) -> u32 {
    v.clamp(0, u32::MAX as i64) as u32
}

impl ScoreImporter for Lr2ScoreDb {
    fn source_name(&self) -> &'static str {
        "LR2"
    }
    fn import(&self, src: &Path) -> Result<Vec<ScoreRecord>, PortError> {
        let io = |e: rusqlite::Error| PortError::Io(format!("{}: {e}", src.display()));
        // IMP-LR2-01
        let db = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(io)?;
        let mut st = db
            .prepare(
                "SELECT hash, clear, perfect, great, good, bad, poor, maxcombo, minbp, playcount FROM score",
            )
            .map_err(io)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    (2..10)
                        .map(|i| r.get::<_, Option<i64>>(i).map(|v| v.unwrap_or(0)))
                        .collect::<Result<Vec<i64>, _>>()?,
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
            })
            .map_err(io)?;
        let mut out = vec![];
        for row in rows {
            let (hash, v, clear) = row.map_err(io)?;
            let Some(md5) = hash.as_deref().and_then(md5_of) else {
                continue;
            };
            let [pg, gr, gd, bd, pr, combo, minbp, plays] = v[..] else {
                unreachable!()
            };
            out.push(ScoreRecord {
                chart: ChartHash { md5, sha256: None },
                ruleset: RulesetId::Lr2,
                lamp: lamp(clear),
                ex_score: count(pg) * 2 + count(gr),
                pgreat: count(pg),
                great: count(gr),
                good: count(gd),
                bad: count(bd),
                poor: count(pr),
                max_combo: count(combo),
                min_bp: (minbp >= 0).then(|| count(minbp)),
                play_count: count(plays),
                origin: ScoreOrigin::ImportedFrom("LR2".into()),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
