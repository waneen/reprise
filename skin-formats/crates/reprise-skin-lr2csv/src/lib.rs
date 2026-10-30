//! LR2 csv スキン（`.lr2skin` / `.csv`）デコーダ。
//!
//! 根拠は OpenLR2（作者の説明では LR2 のソースを書き起こしたもの）の `LR2_skinload.cpp` / `LR2_skinmanage.cpp`
//! を仕様として参照した（コードは転載していない）。
//!
//! 方言（[`Dialect`]）:
//! - [`Dialect::Lr2`]（既定）：LR2 の癖（短い行の無視、入れ子 `#IF` の評価の仕方、否定条件なし）を再現
//! - [`Dialect::Standard`]：入れ子を正しく扱い、`!番号`（負値）を否定として扱う
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | SKN-ENC-01 | BOM 付き/正しい UTF-8 は UTF-8、それ以外は Shift_JIS（CP932） |
//! | SKN-CSV-01 | `,` 区切り最大 30 項目（引用符内の `,` も区切る）。前後が `"` の項目は引用符を外す |
//! | SKN-CSV-02 | 前後の空白を除いて `#` で始まらない行は無視 |
//! | SKN-CSV-03 | (Lr2) 改行を除いて 5 バイト以下の行は無視（例: `#ELSE` 単独行） |
//! | SKN-CSV-04 | 数値は `atol`（先頭から読める分だけ、読めなければ 0）、`!` 始まりは負値 |
//! | SKN-HDR-01 | `#INFORMATION,種別,題名,作者,サムネ,_,幅,高さ`。解像度は既定 640x480、指定は 640x480 未満を切り上げ |
//! | SKN-HDR-02 | `#RESOLUTION,n`: 0=640x480, 1=1280x720, 2=1920x1080, 3=3840x2160, それ以外は `n,高さ` |
//! | SKN-HDR-03 | `#CUSTOMOPTION,名前,開始番号,ラベル…`（開始番号 1000 以上は無視、ラベルは空項目まで） |
//! | SKN-HDR-04 | `#CUSTOMFILE,名前,パターン,既定値` |
//! | SKN-HDR-05 | ヘッダの読み取りは `#ENDOFHEADER` で終わる |
//! | SKN-HDR-06 | `#INFORMATION` が無い、または種別が負ならスキンではない（`read_header` が `NotASkin`） |
//! | SKN-OPT-01 | 読み込み時の真の条件 = 設定の条件 ∪ 各 CUSTOMOPTION の選択（範囲外なら先頭）。0 は常に真、999 は常に偽 |
//! | SKN-OPT-02 | 文書の `options` は読み込み終了時点で真の条件（CUSTOMOPTION の選択と `#SETOPTION` を含む） |
//! | SKN-IF-01 | `#IF,a,b,…` は列挙した条件（最大 9 個）がすべて真なら真。範囲は 0..=999（Lr2 では負値は偽） |
//! | SKN-IF-02 | `#ELSEIF` `#ELSE` `#ENDIF` は最初に真になった腕だけを採る |
//! | SKN-IF-03 | (Lr2) 入れ子の内側は外側が偽でも自分の条件だけで判定する（LR2 の挙動） |
//! | SKN-IF-04 | (Standard) 外側が偽なら内側も偽。負値 `-n`（`!n`）は「n が偽」 |
//! | SKN-SET-01 | `#SETOPTION,番号,値` は以降の `#IF` 判定用に条件を設定する（1000 以上は無視） |
//! | SKN-CUS-01 | パスが CUSTOMFILE のパターンで始まり、選択が `RANDOM`/`ERROR`/空 以外なら `*` を選択で置換 |
//! | SKN-CUS-02 | 本文の `#CUSTOMFILE` で選択が `RANDOM` なら、その場で 1 回だけ乱択して以後に使う |
//! | SKN-IMG-01 | `#IMAGE,パス`。`CONTINUE` は継続、解決できなければ Missing + 診断 |
//! | SKN-IMG-02 | パス中の `\` は `/` にする |
//! | SKN-FNT-01 | `#FONT,大きさ,太さ,種類` はシステムフォントとして `fonts` に、`#LR2FONT,パス` は画像フォントとして `image_fonts` に、それぞれ別に番号を振って並べる |
//! | SKN-FNT-02 | `#LR2FONT` は `.lr2font` を読み、画像はフォントファイルと同じフォルダから探す。フォント・画像が無ければ Missing + 診断 |
//! | SKN-LR2F-01 | `.lr2font`: `#` で始まる行の 2 文字目で `#S,高さ` `#M,間隔` `#T,番号,ファイル名` `#R,文字番号,画像,x,y,w,h` を見分ける。同じ番号・文字は後の行が勝つ |
//! | SKN-LR2F-02 | `.lr2font` の文字番号: 0..=255 はその 1 バイト、256..=8126 は +32832、8127..=15306 は +49281 した Shift_JIS の 2 バイト。それ以外は捨てる |
//! | SKN-INC-01 | `#INCLUDE,パス` はその場に展開する。`#IF` の状態は独立、条件（`#SETOPTION`）は共有。深さ 16 まで |
//! | SKN-SRC-01 | `#SRC_x,添字,画像,x,y,w,h,分割x,分割y,周期,タイマー,引数×5` で要素を作る（分割は最小 1、x/y は負なら 0） |
//! | SKN-SRC-02 | 種別ごとの引数: NUMBER(値,揃え,桁) SLIDER(向き,範囲,値,無効) BARGRAPH(値,向き) BUTTON(動作,クリック,パネル,加算のみ) |
//! | SKN-SRC-03 | `#SRC_TEXT,添字,フォント,値,揃え,編集可` は画像を持たない。`#SRC_BAR_TITLE` も同じ並び（OpenLR2 `ReadSRC_BAR_TITLE`）の文字列要素 |
//! | SKN-DST-01 | `#DST_x` は同名の直近 SRC（同じ添字があればそれ）にキーフレームを追加。同名の SRC が無ければ SKN-DST-04、それも無ければ診断して無視 |
//! | SKN-DST-02 | 最初の DST の ループ(16)・タイマー(17)・条件(18..20, 0 以外)・追加引数(21, 22) が要素の属性になる |
//! | SKN-DST-03 | キーフレーム: 時刻(2) x,y,w,h(3..6) 加速(7) a,r,g,b(8..11) 合成(12) フィルタ(13) 角度(14) 中心(15) |
//! | SKN-DST-04 | 同名 SRC が無く、`名前_…` の形なら接頭辞が一致する直近 SRC を複製して使う（例: BAR_BODY_ON → BAR_BODY）。複製した要素は添字ごとに別（`#DST_BAR_BODY_OFF,1` は `,0` の要素に足さない。OpenLR2 の `dst_BAR_BODY_OFF[添字]`） |
//! | SKN-DST-05 | 描画順 = 最初の DST 行が何番目に処理した命令行か（`#IF` で飛ばした行・`#IF` 系の行は数えない） |
//! | SKN-BLD-01 | 合成: 0=None 1=Alpha 2=Add 3=Sub 4=Mul 5=Sub2 9=InvDest 10=InvSrc 11=MulAlpha、それ以外は Alpha + 診断 |
//! | SKN-DIR-01 | 上記以外の命令は `directives` に生のまま残す |

use reprise_skin_core::*;
use std::path::Path;

mod csv;
pub mod lr2font;
mod parse;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialect {
    #[default]
    Lr2,
    Standard,
}

#[derive(Debug, Clone, Default)]
pub struct Lr2CsvDecoder {
    pub dialect: Dialect,
}

impl SkinDecoder for Lr2CsvDecoder {
    fn name(&self) -> &'static str {
        "LR2 csv"
    }
    fn can_decode(&self, p: &Path) -> bool {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lr2skin") || e.eq_ignore_ascii_case("csv"))
    }
    fn read_header(
        &self,
        path: &Path,
        files: &mut dyn FileResolver,
    ) -> Result<SkinHeader, SkinError> {
        let text = parse::read_text(path, files)?;
        let h = parse::header(&text);
        if h.kind < 0 {
            return Err(SkinError::NotASkin(path.to_path_buf()));
        }
        Ok(h)
    }
    fn decode(
        &self,
        path: &Path,
        settings: &SkinSettings,
        files: &mut dyn FileResolver,
    ) -> Result<SkinDocument, SkinError> {
        parse::decode(path, settings, files, self.dialect)
    }
}

#[cfg(test)]
mod tests;
