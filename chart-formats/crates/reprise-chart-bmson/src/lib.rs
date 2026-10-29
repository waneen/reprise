//! bmson（JSON）デコーダ。未実装（`decode` は `DecodeError::Other` を返す）。
use reprise_chart_core::*;

pub struct BmsonDecoder;
impl ChartDecoder for BmsonDecoder {
    fn name(&self) -> &'static str {
        "bmson"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["bmson"]
    }
    fn decode(
        &self,
        _bytes: &[u8],
        _hint: &DecodeHint,
    ) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        Err(DecodeError::Other("bmson は未実装".into()))
    }
}
