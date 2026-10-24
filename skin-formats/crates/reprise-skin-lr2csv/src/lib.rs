//! LR2 csvスキン（.lr2skin）。#INCLUDE・#IF/#ELSE・#SRC_*/#DST_* を SkinDocument へ。
use reprise_skin_core::*;
use std::path::Path;

pub struct Lr2CsvDecoder;
impl SkinDecoder for Lr2CsvDecoder {
    fn name(&self) -> &'static str {
        "LR2 csv"
    }
    fn can_decode(&self, p: &Path) -> bool {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lr2skin"))
    }
    fn read_header(&self, _p: &Path) -> Result<SkinHeader, SkinError> {
        Err(SkinError::Unsupported("TODO".into()))
    }
    fn decode(
        &self,
        _p: &Path,
        _s: &SkinSettings,
        _f: &mut dyn FileResolver,
    ) -> Result<SkinDocument, SkinError> {
        Err(SkinError::Unsupported("TODO".into()))
    }
}
