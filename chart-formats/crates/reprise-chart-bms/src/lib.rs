//! BMS系（bms/bme/bml/pms）デコーダ。Shift_JIS/UTF-8判定、#RANDOM構文→Block化。
use reprise_chart_core::*;

#[derive(Debug, Default, Clone)]
pub struct BmsOptions {
    /// プレイヤーごとの方言差（例: LNTYPE既定値、0小節の扱い）はここで切り替え
    pub allow_measure_zero: bool,
}

#[derive(Default)]
pub struct BmsDecoder {
    pub options: BmsOptions,
}

impl ChartDecoder for BmsDecoder {
    fn name(&self) -> &'static str {
        "BMS"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["bms", "bme", "bml", "pms"]
    }
    fn decode(&self, _bytes: &[u8]) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        Err(DecodeError::Other(
            "TODO: lexer (encoding_rs で Shift_JIS 対応)".into(),
        ))
    }
}
