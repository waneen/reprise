//! LR2 のオプションの名前の一覧（`LR2files/Config/optionstr.csv`）。OpenLR2 `LR2_skinobject.cpp` の `ReadOptionstrFile` に合わせる
use crate::parse::decode_text;
use std::collections::BTreeMap;

/// 1 つの項目の名前の数（LR2 の `OptionString::str[10]`）
pub const NAMES: usize = 10;

/// SKN-OST-01: `#` で始まる行を `,` で分け、先頭（`#MODE` など）を項目、続く 10 個を名前にする（足りなければ空。前後の `"` は外す）。
/// 同じ項目が何度もあれば後の行
pub fn parse_option_strings(bytes: &[u8]) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for line in decode_text(bytes).lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if !line.starts_with('#') {
            continue;
        }
        let mut cols = line.split(',');
        let head = cols.next().unwrap_or_default().to_string();
        let names = (0..NAMES)
            .map(|_| {
                let c = cols.next().unwrap_or_default();
                c.strip_prefix('"')
                    .and_then(|c| c.strip_suffix('"'))
                    .unwrap_or(c)
                    .to_string()
            })
            .collect();
        out.insert(head, names);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// SKN-OST-01: オプションの名前の一覧
    #[rstest]
    #[case::mode(b"#MODE,ALL,SINGLE,7KEYS\r\n".to_vec(), "#MODE", vec!["ALL", "SINGLE", "7KEYS", "", "", "", "", "", "", ""])]
    #[case::quoted(b"#SORT,\"DIRECTORY\",LEVEL\n".to_vec(), "#SORT", vec!["DIRECTORY", "LEVEL", "", "", "", "", "", "", "", ""])]
    #[case::shift_jis(b"// x\n#DIFFICULTY,\x91\x53\x82\xC4\n".to_vec(), "#DIFFICULTY", vec!["全て", "", "", "", "", "", "", "", "", ""])]
    #[case::later_wins(b"#MODE,A\n#MODE,B\n".to_vec(), "#MODE", vec!["B", "", "", "", "", "", "", "", "", ""])]
    fn skn_ost_01_option_strings(
        #[case] bytes: Vec<u8>,
        #[case] item: &str,
        #[case] want: Vec<&str>,
    ) {
        let got = parse_option_strings(&bytes);
        assert_eq!(
            got.get(item).cloned(),
            Some(want.into_iter().map(String::from).collect())
        );
    }
}
