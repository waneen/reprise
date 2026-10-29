//! beatoraja JSON / Lua スキン。未実装（`decode` は `SkinError::NotASkin` を返す）。
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
    fn read_header(&self, p: &Path, _f: &mut dyn FileResolver) -> Result<SkinHeader, SkinError> {
        Err(SkinError::NotASkin(p.to_path_buf()))
    }
    fn decode(
        &self,
        p: &Path,
        _s: &SkinSettings,
        _f: &mut dyn FileResolver,
    ) -> Result<SkinDocument, SkinError> {
        Err(SkinError::NotASkin(p.to_path_buf()))
    }
}
