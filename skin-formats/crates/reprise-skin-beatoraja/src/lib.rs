//! beatoraja JSON スキン（`.json`）デコーダ。Lua スキン（`.luaskin`）は対象外。
//!
//! 根拠は beatoraja（exch-bms2/beatoraja）の `skin/json/JsonSkin.java`・`JSONSkinLoader.java`・
//! `JsonSkinSerializer.java`・`SkinLoader.getPath`・`SkinObject.java`・`Skin.java` を仕様として参照した
//! （コードは転載していない）。
//!
//! 定義（`image` `value` `text` `slider` `graph`）は `destination` から id で参照されて初めて要素になる。
//! プレイ画面の `note` `judge` `gauge` `bga`、選曲の `songlist` などの複合オブジェクトは
//! `ElementKind::Other`（`command` = id）として位置だけを渡し、解釈は利用側に任せる。
//! 未対応: `imageset`・`floatvalue` の中身、`offset`、`stretch`、`mouseRect`、クリップ、`draw`、
//! 値が条件分岐の配列になった単一オブジェクト、パスの `|` 記法、Lua 式（`timer`・`op` の文字列）。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | BRJ-ENC-01 | BOM 付き/正しい UTF-8 は UTF-8（BOM は除く）、それ以外は Shift_JIS（beatoraja は JVM 既定の文字コードに依存） |
//! | BRJ-JSN-01 | libGDX と同じゆるい JSON: コメント・末尾カンマ・引用符無しのキー/値を受け付け、重複キーは後勝ち。読めなければ `Syntax` エラー |
//! | BRJ-HDR-01 | ヘッダ: `type`（既定 -1）・`name`・`author`・`w`/`h`（既定 1280x720）。`type` が負ならスキンではない |
//! | BRJ-HDR-02 | `property` は `Custom::Choice`（項目名と op）、`filepath` は `Custom::File`（`path` と `def`）。出現順 |
//! | BRJ-HDR-03 | `fadeout` `input` `scene` `close` `loadend` `playstart` `judgetimer` `finishmargin` は書かれていれば大文字名の `directives` にする |
//! | BRJ-OPT-01 | 有効な条件 = 設定の条件 ∪ 各 `property` の選択項目の op（設定が項目に無ければ `def` 名の項目、それも無ければ先頭）。文書の `options` にも残す |
//! | BRJ-IF-01 | 配列の要素 `{"if":…,"value":x}` は条件が真なら x、`{"if":…,"values":[…]}` はその位置に展開。`value(s)` が無ければ普通の要素 |
//! | BRJ-IF-02 | 条件: 正の n は n が有効、負の -n は n が無効、配列は AND、配列の中の配列は OR、それ以外は偽。ヘッダの読み取りでは有効な条件は空 |
//! | BRJ-INC-01 | `{"include":"パス"}` は配列の要素ならその位置に中身（配列）を展開、それ以外の位置なら置き換える。パスは取り込み元のフォルダ基準。読めなければ配列は空・値は null（診断）。深さ 16 まで |
//! | BRJ-SRC-01 | `source` の画像はスキンのフォルダ基準で `FileResolver` に解決させる（`*` の乱択は解決側）。解決できなければ Missing + 診断 |
//! | BRJ-CUS-01 | パスが `filepath` の `path` で始まり、選択（無ければ `def`）が `Random`・空でなければ「パスの最後の `*` の手前 + 選択ファイル名 + パスの一致部分より後」に置き換える |
//! | BRJ-FNT-01 | `font` はフォントファイル（解決できなければ Missing）。`text` の `font` はその添字 |
//! | BRJ-OBJ-01 | `destination` の id は image → value → text → slider → graph の順に引き、IMAGE/NUMBER/TEXT/SLIDER/BARGRAPH の要素にする。見つからなければ `Other`（command = id） |
//! | BRJ-OBJ-02 | 切り出し: `src` の画像、`x,y,w,h`、`divx`/`divy`（最小 1）、`cycle`、`timer`（負は無し） |
//! | BRJ-OBJ-03 | 種別ごとの引数: value（ref・align・digit）、text（font・ref・align・editable）、slider（angle→向き・range・type→値・changeable の否定→無効）、graph（type→値・angle→向き） |
//! | BRJ-DST-01 | キーフレームの補完: 最初は time/x/y/w/h/acc/angle = 0、a/r/g/b = 255。以降は直前の値。time 順（同じ time は記述順）に並べる |
//! | BRJ-DST-02 | 座標は左下原点なので上端原点に直す: y' = 高さ − (y + h) |
//! | BRJ-DST-03 | `blend` `filter` `center` `timer` `loop` は destination 単位で全キーフレームに適用。`timer` が数値でない（Lua 式）なら無し + 診断、負なら無し |
//! | BRJ-DST-04 | `op` のうち `property` の op はここで評価し、満たさなければ要素を作らない。それ以外の番号は表示条件として残す。文字列は診断して無視 |
//! | BRJ-BLD-01 | 合成: 0/1 = Alpha、2 = Add、3 = Subtract、4 = Multiply、9 = InvertDest、それ以外は Alpha + 診断 |

use reprise_skin_core::*;
use std::path::Path;

mod build;
mod lenient;

pub struct BeatorajaJsonDecoder;

impl SkinDecoder for BeatorajaJsonDecoder {
    fn name(&self) -> &'static str {
        "beatoraja json"
    }
    fn can_decode(&self, p: &Path) -> bool {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    }
    fn read_header(
        &self,
        path: &Path,
        files: &mut dyn FileResolver,
    ) -> Result<SkinHeader, SkinError> {
        let h = build::Loader::new(path, files)?.header();
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
        let mut l = build::Loader::new(path, files)?;
        let header = l.header();
        if header.kind < 0 {
            return Err(SkinError::NotASkin(path.to_path_buf()));
        }
        Ok(l.document(header, settings))
    }
}

#[cfg(test)]
mod tests;
