//! beatoraja JSON / Lua スキン（Luaは feature で mlua を有効化する想定）。
use reprise_skin_core::*;
use std::path::Path;

pub struct BeatorajaJsonDecoder;
impl SkinDecoder for BeatorajaJsonDecoder {
    fn name(&self) -> &'static str {
        "beatoraja json"
    }
    fn can_decode(&self, p: &Path) -> bool {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
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
