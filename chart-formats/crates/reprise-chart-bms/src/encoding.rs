//! 文字コード判定（BMS-ENC-01..03）
use reprise_chart_core::DecodeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncodingMode {
    /// BOM → UTF-8 判定 → Shift_JIS の順で決める
    #[default]
    Auto,
    ShiftJis,
    Utf8,
}

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

pub fn decode(bytes: &[u8], mode: EncodingMode) -> Result<String, DecodeError> {
    match mode {
        EncodingMode::Utf8 => {
            let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
            String::from_utf8(body.to_vec()).map_err(|e| DecodeError::Encoding(e.to_string()))
        }
        EncodingMode::ShiftJis => Ok(shift_jis(bytes)),
        EncodingMode::Auto => {
            if let Some(body) = bytes.strip_prefix(UTF8_BOM) {
                return Ok(String::from_utf8_lossy(body).into_owned());
            }
            match std::str::from_utf8(bytes) {
                Ok(s) => Ok(s.to_owned()),
                Err(_) => Ok(shift_jis(bytes)),
            }
        }
    }
}

fn shift_jis(bytes: &[u8]) -> String {
    // encoding_rs の Shift_JIS は WHATWG 仕様（CP932 相当の拡張を含む）
    let (s, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    s.into_owned()
}
