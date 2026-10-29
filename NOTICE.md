# Notices

## 本プロジェクトのライセンス
MIT OR Apache-2.0（LICENSE-MIT / LICENSE-APACHE）。

## 参照した資料（コード・データは同梱していません）
- **Lunatic Rave 2**（lavalse）: 挙動の互換対象。本プロジェクトは非公式であり、lavalse 氏および LR2 とは無関係です。LR2 本体・同梱スキン・フォント・効果音は一切含みません。
- **OpenLR2**（https://github.com/GOMazk/OpenLR2）: LR2 の挙動（乱数のシード範囲・消費順、BMS とスキンの読み込み規則、判定幅とゲージ、スコア DB の形式など）を調べる際の参考資料。コードおよびデータ（例: シード対応表）は転載していません。照合テストは利用者が手元の対応表を指定して任意実行します。
- **bmson format specification**（https://bmson-spec.readthedocs.io/）: bmson の読み込み規則。
- **beatoraja**（https://github.com/exch-bms2/beatoraja、GPL-3.0）: JSON スキン・score.db の形式と判定・ゲージの仕様を調べる際の参考資料。コードは転載していません。
- **Mersenne Twister**（Makoto Matsumoto, Takuji Nishimura）: アルゴリズムを本プロジェクトで独自に Rust 実装。
- **DxLib**（山田 巧）: LR2 が使用していた乱数 API の挙動を互換対象として参照。コードは含みません。

## 依存ライブラリ
実行時に使う外部 crate は MIT / Apache-2.0 などの寛容なライセンスのみです（`cargo metadata` で確認）。
- **encoding_rs**: `(Apache-2.0 OR MIT) AND BSD-3-Clause`。バイナリを配布する際は BSD-3-Clause の著作権表示を同梱すること
- **md-5 / sha2**（RustCrypto）: MIT OR Apache-2.0
- **serde_json**: MIT OR Apache-2.0
- **rusqlite / libsqlite3-sys**: MIT。同梱してビルドする **SQLite** 本体はパブリックドメイン
- その他の間接依存（hashbrown, foldhash（Zlib）, memchr（Unlicense OR MIT）など）も寛容なライセンス
- テスト専用: rstest, tempfile（MIT OR Apache-2.0）

## 商標
beatmania / beatmania IIDX は株式会社コナミデジタルエンタテインメントの商標です。本プロジェクトはコナミとは無関係です。
その他の名称（beatoraja, LR2oraja 等）は各権利者に帰属し、互換性の説明のためにのみ言及しています。
