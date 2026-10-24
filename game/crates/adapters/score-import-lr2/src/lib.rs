//! LR2 score.db（sqlite）からの取り込み。LR2固有のclear値→ClearLamp写像はここだけが知る。
use application::ports::{PortError, ScoreImporter};
use domain::score::ScoreRecord;
use std::path::Path;

pub struct Lr2ScoreDb;
impl ScoreImporter for Lr2ScoreDb {
    fn source_name(&self) -> &'static str {
        "LR2"
    }
    fn import(&self, _src: &Path) -> Result<Vec<ScoreRecord>, PortError> {
        Err(PortError::Io("TODO: rusqlite".into()))
    }
}
