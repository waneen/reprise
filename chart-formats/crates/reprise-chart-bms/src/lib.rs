//! BMS 系（bms / bme / bml / pms）デコーダ。
//!
//! バイト列を [`ChartDocument`] に変換する。分岐の解決・LN の対応付けは
//! `reprise-chart-core` の `resolve` が行う。
//!
//! 方言（[`Dialect`]）:
//! - [`Dialect::Lr2`]（既定）：Lunatic Rave 2 (beta3 100201) の読み込み挙動に合わせる。
//!   根拠は OpenLR2（LR2 の逆コンパイル）の `LR2_bmsload.cpp` を仕様として参照した。
//! - [`Dialect::Standard`]：入れ子・`#ELSE` 等を含む一般的な解釈。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | BMS-ENC-01 | UTF-8 BOM 付きは UTF-8（BOM 除去） |
//! | BMS-ENC-02 | BOM 無しで UTF-8 として正しければ UTF-8 |
//! | BMS-ENC-03 | それ以外は Shift_JIS（CP932） |
//! | BMS-LEX-01 | 前後の空白を除いて `#` で始まらない行は無視 |
//! | BMS-LEX-02 | ヘッダ名は大文字小文字を区別せず、値は最初の空白（タブ含む）の後 |
//! | BMS-LEX-03 | `#mmmcc:data` は 2 文字ずつのオブジェクト列。小節内位置は `i/個数`、奇数長の末尾と空白は無視 |
//! | BMS-LEX-04 | ID は 36 進（大小無視）。`#BASE 62` 以降は 62 進（大小区別） |
//! | BMS-LEX-05 | `00` は空 |
//! | BMS-CH-01 | ch01 は BGM。同じ小節の複数行は積み重なる |
//! | BMS-CH-02 | ch02 は小節長（10 進小数）。0 以下は無視 |
//! | BMS-CH-03 | ch03 は 16 進の BPM 値 |
//! | BMS-CH-04 | ch08 は `#BPMxx`、ch09 は `#STOPxx` の参照 |
//! | BMS-CH-05 | ch04/06/07 は BGA の Base/Poor/Layer |
//! | BMS-CH-06 | BMS のレーン: x1..x5→1..5, x6→皿, x8→6, x9→7, x7（フリーゾーン）は破棄 |
//! | BMS-CH-07 | PMS のレーン: 11..15→1..5, 18→6, 19→7, 16→8, 17→9, 22..25→6..9 |
//! | BMS-CH-08 | 種別: 1x/2x 通常, 3x/4x 不可視, Dx/Ex 地雷（値は ID）。2P は lane+10 |
//! | BMS-CH-09 | 未対応チャンネルは診断を出して無視（チャンネルごとに 1 回） |
//! | BMS-CH-10 | 5x/6x は LN チャンネル（解決時に頭→尾で対になる） |
//! | BMS-HDR-01 | メタ情報（TITLE 等）を定義として出す |
//! | BMS-HDR-02 | `#WAVxx` `#BMPxx` `#BPMxx` `#EXBPMxx` `#STOPxx` を定義として出す（STOP は正の値のみ） |
//! | BMS-HDR-03 | `#LNOBJ xx` は LN 終端 ID の定義 |
//! | BMS-HDR-04 | `#TOTAL` は Lr2 では整数に切り捨て（`atol`）、Standard では小数 |
//! | BMS-HDR-05 | 未知のヘッダは診断のみ |
//! | BMS-RND-L2-01 | (Lr2) `#RANDOM` は新しい乱数を始め、最初の `#IF` までの行は捨てる |
//! | BMS-RND-L2-02 | (Lr2) `#IF` は腕を開き（開いている腕は閉じる）、`#ENDIF` 後の行は無条件 |
//! | BMS-RND-L2-03 | (Lr2) `#ELSE` `#ELSEIF` `#ENDRANDOM` `#SETRANDOM` は無視（診断） |
//! | BMS-RND-L2-04 | (Lr2) 入れ子は無い。`#IF` 内の `#RANDOM` は前の乱数を閉じる |
//! | BMS-RND-STD-01 | (Standard) 入れ子・`#ELSEIF`・`#ELSE`・`#ENDRANDOM`・`#SETRANDOM` に対応 |
//! | BMS-RND-STD-02 | (Standard) `#RANDOM` と最初の `#IF` の間の行は無条件 |
//! | BMS-LAY-01 | 鍵盤構成: `.pms`→9, 2P チャンネルあり→ダブル、x8/x9 あり→7鍵、それ以外→5鍵 |
//! | BMS-POL-01 | Lr2 方言は `ResolvePolicy::LR2`、Standard は既定ポリシー |

use reprise_chart_core::*;

mod build;
mod encoding;
mod lex;

pub use encoding::EncodingMode;

/// 読み込みの方言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialect {
    #[default]
    Lr2,
    Standard,
}

#[derive(Debug, Clone, Default)]
pub struct BmsOptions {
    pub dialect: Dialect,
    pub encoding: EncodingMode,
}

#[derive(Debug, Clone, Default)]
pub struct BmsDecoder {
    pub options: BmsOptions,
}

impl BmsDecoder {
    pub fn new(options: BmsOptions) -> Self {
        Self { options }
    }
}

impl ChartDecoder for BmsDecoder {
    fn name(&self) -> &'static str {
        "BMS"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["bms", "bme", "bml", "pms"]
    }
    fn decode(
        &self,
        bytes: &[u8],
        hint: &DecodeHint,
    ) -> Result<(ChartDocument, Vec<Diagnostic>), DecodeError> {
        let text = encoding::decode(bytes, self.options.encoding)?;
        let is_pms = hint.extension.as_deref() == Some("pms");
        Ok(build::build(&text, is_pms, self.options.dialect))
    }
}

#[cfg(test)]
mod tests;
