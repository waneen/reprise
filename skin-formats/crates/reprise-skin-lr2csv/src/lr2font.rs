//! LR2 の画像フォント（`.lr2font`）の解析（SKN-LR2F-*）。
//! OpenLR2 の `ReadImageFont` を仕様として参照した（コードは転載していない）。
use crate::csv::split;
use reprise_skin_core::Glyph;

/// 画像フォントの本文（画像はファイル名のまま）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lr2Font {
    pub size: i32,
    pub margin: i32,
    /// （フォント内の番号, ファイル名）
    pub textures: Vec<(i32, String)>,
    pub glyphs: Vec<Glyph>,
}

/// SKN-LR2F-02: 文字番号 → 文字。0..=255 はその 1 バイト、256..=8126 は +32832、8127..=15306 は +49281 した
/// Shift_JIS（CP932）の 2 バイト。それ以外・読めない番号は None
pub fn glyph_char(id: i32) -> Option<char> {
    let bytes: Vec<u8> = match id {
        0..=255 => vec![id as u8],
        256..=8126 => {
            let i = id + 32832;
            vec![(i >> 8) as u8, i as u8]
        }
        8127..=15306 => {
            let i = id + 49281;
            vec![(i >> 8) as u8, i as u8]
        }
        _ => return None,
    };
    let (s, _, bad) = encoding_rs::SHIFT_JIS.decode(&bytes);
    if bad {
        return None;
    }
    s.chars().next()
}

/// SKN-LR2F-01: `#` で始まる行のうち `#S,高さ`・`#M,間隔`・`#T,番号,ファイル名`・`#R,文字番号,画像,x,y,w,h`。
/// 命令は 2 文字目だけで見分け、同じ文字は後の行が勝つ
pub fn parse(text: &str) -> Lr2Font {
    let mut f = Lr2Font::default();
    for line in text.lines() {
        if !line.starts_with('#') {
            continue;
        }
        let row = split(line.trim());
        match line.as_bytes().get(1).map(u8::to_ascii_uppercase) {
            Some(b'S') => f.size = row.v(1),
            Some(b'M') => f.margin = row.v(1),
            Some(b'T') => {
                let id = row.v(1);
                f.textures.retain(|(i, _)| *i != id);
                f.textures.push((id, row.s(2).replace('\\', "/")));
            }
            Some(b'R') => {
                if let Some(ch) = glyph_char(row.v(1)) {
                    f.glyphs.retain(|g| g.ch != ch);
                    f.glyphs.push(Glyph {
                        ch,
                        texture: row.v(2),
                        rect: [row.v(3), row.v(4), row.v(5), row.v(6)],
                    });
                }
            }
            _ => {}
        }
    }
    f
}
