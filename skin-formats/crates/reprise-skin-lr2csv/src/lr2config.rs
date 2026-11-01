//! LR2 本体の設定（`LR2files/Config/config.xml`）のうち、選んでいるスキンのパス。
//! OpenLR2 `LR2_configsave.cpp` の `ReadConfig`（`config` → 最初の `skin` の中の `play_7`・`select`・`result`・`keyconfig` など）に合わせる
use crate::parse::decode_text;
use std::collections::BTreeMap;

/// SKN-CFG-01: 最初の `<skin>` の中の `<名前>パス</名前>` を 名前 → パス にする（中身の無い要素は除く。`\` は `/` に、
/// `&amp;` などの実体参照は文字に戻す）。`<skin>` が無ければ空。文字コードは SKN-ENC-01（LR2 は Shift_JIS で書く）
pub fn skin_paths(bytes: &[u8]) -> BTreeMap<String, String> {
    let text = decode_text(bytes);
    let mut out = BTreeMap::new();
    let Some(start) = text.find("<skin>") else {
        return out;
    };
    let body = &text[start + "<skin>".len()..];
    let body = &body[..body.find("</skin>").unwrap_or(body.len())];
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('>') else { break };
        let name = &rest[..close];
        rest = &rest[close + 1..];
        if name.starts_with('/') || name.ends_with('/') || name.starts_with('!') {
            continue;
        }
        let end = format!("</{name}>");
        let Some(e) = rest.find(&end) else { continue };
        let value = unescape(rest[..e].trim()).replace('\\', "/");
        rest = &rest[e + end.len()..];
        if !value.is_empty() {
            out.entry(name.to_string()).or_insert(value);
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// SKN-CFG-01: config.xml の選んでいるスキン
    #[rstest]
    #[case::paths(
        b"<?xml version=\"1.0\" encoding=\"shift_jis\"?>\n<config>\n\t<play>\n\t\t<select>9</select>\n\t</play>\n\t<skin>\n\t\t<play_7>LR2files\\Theme\\a\\play.lr2skin</play_7>\n\t\t<select>LR2files\\Theme\\a\\select.lr2skin</select>\n\t\t<decide></decide>\n\t</skin>\n</config>\n".to_vec(),
        vec![("play_7", "LR2files/Theme/a/play.lr2skin"), ("select", "LR2files/Theme/a/select.lr2skin")]
    )]
    #[case::first_skin_only(b"<skin><result>a&amp;b</result></skin><skin><result>c</result><keyconfig>k</keyconfig></skin>".to_vec(), vec![("result", "a&b")])]
    #[case::shift_jis(b"<skin><select>\x91\x53.csv</select></skin>".to_vec(), vec![("select", "全.csv")])]
    #[case::no_skin(b"<config><play></play></config>".to_vec(), vec![])]
    fn skn_cfg_01_skin_paths(#[case] bytes: Vec<u8>, #[case] want: Vec<(&str, &str)>) {
        let want: BTreeMap<String, String> = want
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(skin_paths(&bytes), want);
    }
}
