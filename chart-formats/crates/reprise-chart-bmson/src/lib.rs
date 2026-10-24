//! bmson（JSON）デコーダ。分岐が無いので Block::Events のみ返す。
use reprise_chart_core::*;

pub struct BmsonDecoder;
impl ChartDecoder for BmsonDecoder {
    fn name(&self) -> &'static str {
        "bmson"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["bmson"]
    }
    fn decode(&self, _bytes: &[u8]) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        Err(DecodeError::Other("TODO: serde_json".into()))
    }
}
