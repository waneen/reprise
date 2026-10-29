//! beatoraja 系（beatoraja / LR2oraja）のスコア DB（`player/<名前>/score.db`、sqlite）からの取り込み。
//! 判定ルールは出自で決まるので、どちらとして取り込むかを [`BeatorajaScoreDb::ruleset`] で指定する
//! （LR2oraja は beatoraja のフォークで同じ score.db を使う前提）。
//!
//! 根拠: beatoraja `ScoreDatabaseAccessor.java`（`score` テーブル）、`ClearType.java`（ランプ値）、
//! `ScoreData.getExscore()`、`PlayDataAccessor.java`（`mode` 列）、
//! `song/SQLiteSongDatabaseAccessor.java`（`song` テーブルの md5/sha256）。コードは転載していない。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | IMP-BRJ-01 | score.db の `score` テーブルを読み取り専用で開く。開けない・テーブルが無ければ `Io` エラー |
//! | IMP-BRJ-02 | `sha256` は 16 進 64 文字のものだけ（コースは連結で長いので除外）。`mode` が 0 以外（CN/HCN 等で遊んだ記録・コース）は取り込まない |
//! | IMP-BRJ-03 | MD5 は曲 DB（`songdata.db` の `song` テーブル）で sha256 から引く。引けない記録は取り込まない。曲 DB の既定位置は score.db の 2 つ上のフォルダ |
//! | IMP-BRJ-04 | clear: 0..=10 を NoPlay, Failed, AssistEasy, LightAssistEasy, Easy, Normal, Hard, ExHard, FullCombo, Perfect, Max に。それ以外は NoPlay |
//! | IMP-BRJ-05 | 判定内訳は FAST/SLOW の合計（epg+lpg 等）、EX スコア = (epg+lpg)×2 + egr+lgr。空 POOR（ems/lms）は含めない |
//! | IMP-BRJ-06 | combo → 最大コンボ、playcount はそのまま。minbp は負か 2147483647（未設定）なら不明 |
//! | IMP-BRJ-07 | ルールは指定のもの、出自は `ImportedFrom(source_name)`、SHA-256 も保持する |
use application::ports::{PortError, ScoreImporter};
use domain::chart::ChartHash;
use domain::play::RulesetId;
use domain::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct BeatorajaScoreDb {
    pub name: &'static str,
    pub ruleset: RulesetId,
    /// 曲 DB（`songdata.db`）。None なら score.db の 2 つ上のフォルダから探す
    pub song_db: Option<PathBuf>,
}

impl BeatorajaScoreDb {
    pub fn beatoraja() -> Self {
        Self {
            name: "beatoraja",
            ruleset: RulesetId::Beatoraja,
            song_db: None,
        }
    }
    pub fn lr2oraja() -> Self {
        Self {
            name: "LR2oraja",
            ruleset: RulesetId::Lr2,
            song_db: None,
        }
    }
}

/// IMP-BRJ-04
pub fn lamp(clear: i64) -> ClearLamp {
    use ClearLamp::*;
    match clear {
        1 => Failed,
        2 => AssistEasy,
        3 => LightAssistEasy,
        4 => Easy,
        5 => Normal,
        6 => Hard,
        7 => ExHard,
        8 => FullCombo,
        9 => Perfect,
        10 => Max,
        _ => NoPlay,
    }
}

fn hex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn count(v: i64) -> u32 {
    v.clamp(0, u32::MAX as i64) as u32
}

const UNSET: i64 = i32::MAX as i64;

fn open(p: &Path) -> Result<Connection, PortError> {
    Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| PortError::Io(format!("{}: {e}", p.display())))
}

/// IMP-BRJ-03: sha256 → md5
fn md5_table(song_db: &Path) -> Result<HashMap<[u8; 32], [u8; 16]>, PortError> {
    let io = |e: rusqlite::Error| PortError::Io(format!("{}: {e}", song_db.display()));
    let db = open(song_db)?;
    let mut st = db.prepare("SELECT sha256, md5 FROM song").map_err(io)?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        })
        .map_err(io)?;
    let mut m = HashMap::new();
    for row in rows {
        if let (Some(s), Some(d)) = row.map_err(io)? {
            if let (Some(s), Some(d)) = (hex::<32>(&s), hex::<16>(&d)) {
                m.insert(s, d);
            }
        }
    }
    Ok(m)
}

impl ScoreImporter for BeatorajaScoreDb {
    fn source_name(&self) -> &'static str {
        self.name
    }
    fn import(&self, src: &Path) -> Result<Vec<ScoreRecord>, PortError> {
        let io = |e: rusqlite::Error| PortError::Io(format!("{}: {e}", src.display()));
        // IMP-BRJ-01
        let db = open(src)?;
        let mut st = db
            .prepare(
                "SELECT sha256, mode, clear, epg, lpg, egr, lgr, egd, lgd, ebd, lbd, epr, lpr, combo, minbp, playcount FROM score",
            )
            .map_err(io)?;
        let song_db = self.song_db.clone().unwrap_or_else(|| {
            src.parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .unwrap_or(Path::new("."))
                .join("songdata.db")
        });
        let md5s = md5_table(&song_db)?;
        let rows = st
            .query_map([], |r| {
                let sha: Option<String> = r.get(0)?;
                let v = (1..16)
                    .map(|i| r.get::<_, Option<i64>>(i).map(|v| v.unwrap_or(0)))
                    .collect::<Result<Vec<i64>, _>>()?;
                Ok((sha, v))
            })
            .map_err(io)?;
        let mut out = vec![];
        for row in rows {
            let (sha, v) = row.map_err(io)?;
            let [mode, clear, epg, lpg, egr, lgr, egd, lgd, ebd, lbd, epr, lpr, combo, minbp, plays] =
                v[..]
            else {
                unreachable!()
            };
            // IMP-BRJ-02
            let Some(sha256) = sha.as_deref().and_then(hex::<32>) else {
                continue;
            };
            if mode != 0 {
                continue;
            }
            // IMP-BRJ-03
            let Some(&md5) = md5s.get(&sha256) else {
                continue;
            };
            let (pg, gr) = (count(epg) + count(lpg), count(egr) + count(lgr));
            out.push(ScoreRecord {
                chart: ChartHash {
                    md5,
                    sha256: Some(sha256),
                },
                ruleset: self.ruleset,
                lamp: lamp(clear),
                ex_score: pg * 2 + gr,
                pgreat: pg,
                great: gr,
                good: count(egd) + count(lgd),
                bad: count(ebd) + count(lbd),
                poor: count(epr) + count(lpr),
                max_combo: count(combo),
                min_bp: (0..UNSET).contains(&minbp).then(|| count(minbp)),
                play_count: count(plays),
                origin: ScoreOrigin::ImportedFrom(self.name.into()),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
