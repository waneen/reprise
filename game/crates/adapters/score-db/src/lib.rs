//! reprise 自身の記録の保存先（sqlite）。`ScoreRepository` の実装。
//! 列の値（ランプ・ルール・出自の表し方）はこのクレートだけが知る。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | SDB-01 | 開くときにファイルとフォルダが無ければ作り、`score` テーブルが無ければ作る。開けなければ `Io` エラー |
//! | SDB-02 | 保存した記録は同じ MD5・同じルールで読み戻せる（すべての項目、最小 BP の不明、SHA-256 の有無、出自を含む）。同じ MD5・ルールへの保存は置き換える |
//! | SDB-03 | MD5 かルールが違えば別の記録。無ければ None |
//! | SDB-04 | ランプは NoPlay..Max を 0..=10、ルールは LR2 = 0・beatoraja = 1 で保存し、知らない値のランプは NoPlay、知らないルールの行は無いものとする |
use application::ports::{PortError, ScoreRepository};
use domain::chart::ChartHash;
use domain::play::RulesetId;
use domain::score::{ClearLamp, ScoreOrigin, ScoreRecord};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const CREATE: &str = "CREATE TABLE IF NOT EXISTS score(\
md5 TEXT NOT NULL, ruleset INTEGER NOT NULL, sha256 TEXT, lamp INTEGER NOT NULL, \
pgreat INTEGER NOT NULL, great INTEGER NOT NULL, good INTEGER NOT NULL, bad INTEGER NOT NULL, poor INTEGER NOT NULL, \
maxcombo INTEGER NOT NULL, minbp INTEGER, playcount INTEGER NOT NULL, clearcount INTEGER NOT NULL, origin TEXT NOT NULL, \
PRIMARY KEY(md5, ruleset))";

const LAMPS: [ClearLamp; 11] = [
    ClearLamp::NoPlay,
    ClearLamp::Failed,
    ClearLamp::AssistEasy,
    ClearLamp::LightAssistEasy,
    ClearLamp::Easy,
    ClearLamp::Normal,
    ClearLamp::Hard,
    ClearLamp::ExHard,
    ClearLamp::FullCombo,
    ClearLamp::Perfect,
    ClearLamp::Max,
];

pub struct SqliteScoreDb {
    db: Connection,
}

fn io(e: impl std::fmt::Display) -> PortError {
    PortError::Io(e.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn ruleset_code(r: RulesetId) -> i64 {
    match r {
        RulesetId::Lr2 => 0,
        RulesetId::Beatoraja => 1,
    }
}

fn lamp_code(l: ClearLamp) -> i64 {
    LAMPS.iter().position(|x| *x == l).unwrap_or(0) as i64
}

fn origin_text(o: &ScoreOrigin) -> String {
    match o {
        ScoreOrigin::Native => "native".into(),
        ScoreOrigin::ImportedFrom(s) => format!("import:{s}"),
    }
}

fn origin_of(s: &str) -> ScoreOrigin {
    match s.strip_prefix("import:") {
        Some(name) => ScoreOrigin::ImportedFrom(name.into()),
        None => ScoreOrigin::Native,
    }
}

impl SqliteScoreDb {
    /// SDB-01
    pub fn open(path: &Path) -> Result<Self, PortError> {
        let err = |e: &dyn std::fmt::Display| PortError::Io(format!("{}: {e}", path.display()));
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| err(&e))?;
        }
        let db = Connection::open(path).map_err(|e| err(&e))?;
        db.execute(CREATE, []).map_err(|e| err(&e))?;
        Ok(Self { db })
    }
}

impl ScoreRepository for SqliteScoreDb {
    fn find(
        &self,
        chart: &ChartHash,
        ruleset: RulesetId,
    ) -> Result<Option<ScoreRecord>, PortError> {
        let row = self
            .db
            .query_row(
                "SELECT sha256, lamp, pgreat, great, good, bad, poor, maxcombo, minbp, playcount, clearcount, origin \
                 FROM score WHERE md5 = ?1 AND ruleset = ?2",
                params![hex(&chart.md5), ruleset_code(ruleset)],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        (1..11)
                            .map(|i| r.get::<_, Option<i64>>(i))
                            .collect::<Result<Vec<_>, _>>()?,
                        r.get::<_, String>(11)?,
                    ))
                },
            )
            .optional()
            .map_err(io)?;
        let Some((sha, v, origin)) = row else {
            return Ok(None);
        };
        let n = |x: Option<i64>| x.unwrap_or(0).clamp(0, u32::MAX as i64) as u32;
        let [lamp, pg, gr, gd, bd, pr, combo, minbp, plays, clears] = v[..] else {
            unreachable!()
        };
        Ok(Some(ScoreRecord {
            chart: ChartHash {
                md5: chart.md5,
                sha256: sha.as_deref().and_then(unhex::<32>),
            },
            ruleset,
            lamp: lamp
                .and_then(|l| usize::try_from(l).ok())
                .and_then(|l| LAMPS.get(l).copied())
                .unwrap_or(ClearLamp::NoPlay),
            ex_score: n(pg) * 2 + n(gr),
            pgreat: n(pg),
            great: n(gr),
            good: n(gd),
            bad: n(bd),
            poor: n(pr),
            max_combo: n(combo),
            min_bp: minbp.map(|b| n(Some(b))),
            play_count: n(plays),
            clear_count: n(clears),
            origin: origin_of(&origin),
        }))
    }

    fn save(&mut self, r: ScoreRecord) -> Result<(), PortError> {
        self.db
            .execute(
                "INSERT OR REPLACE INTO score(md5, ruleset, sha256, lamp, pgreat, great, good, bad, poor, maxcombo, minbp, playcount, clearcount, origin) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    hex(&r.chart.md5),
                    ruleset_code(r.ruleset),
                    r.chart.sha256.map(|s| hex(&s)),
                    lamp_code(r.lamp),
                    r.pgreat,
                    r.great,
                    r.good,
                    r.bad,
                    r.poor,
                    r.max_combo,
                    r.min_bp,
                    r.play_count,
                    r.clear_count,
                    origin_text(&r.origin),
                ],
            )
            .map_err(io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
