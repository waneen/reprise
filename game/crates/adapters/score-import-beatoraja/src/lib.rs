//! beatoraja / LR2oraja score.db からの取り込み（ruleset = Beatoraja / Lr2 を出自で振り分け）。
use application::ports::{PortError, ScoreImporter};
use domain::score::ScoreRecord;
use std::path::Path;

pub struct BeatorajaScoreDb;
impl ScoreImporter for BeatorajaScoreDb {
    fn source_name(&self) -> &'static str {
        "beatoraja"
    }
    fn import(&self, _src: &Path) -> Result<Vec<ScoreRecord>, PortError> {
        Err(PortError::Io("TODO".into()))
    }
}
