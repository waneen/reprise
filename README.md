# reprise

Lunatic Rave 2 (LR2) のプレイ感の再現を目指す、Rust製のクロスプラットフォームBMSプレイヤー（開発初期）。

> **非公式プロジェクトです。** lavalse 氏および LR2 とは無関係です。LR2 本体・スキン・フォント・効果音などの素材は一切含みません。詳細は [NOTICE.md](NOTICE.md)。

## 構成
| ディレクトリ | 内容 |
|---|---|
| `chart-formats/` | 譜面記法の汎用ライブラリ（reprise-chart-*） |
| `skin-formats/` | スキン記法の汎用ライブラリ（reprise-skin-*） |
| `game/` | プレイヤー本体（DDD / クリーンアーキテクチャ） |

設計は [ARCHITECTURE.md](ARCHITECTURE.md)。

## ビルド
```sh
cd game && cargo run -p reprise
```
各 workspace で `cargo test`。CI は Linux / Windows / macOS でテストし、aarch64・x86_64 の各ターゲットをビルドします。`v*` タグで6ターゲットのバイナリを Release に添付します。

LR2互換乱数を OpenLR2 の対応表と全件照合する場合（任意）:
```sh
cd game && OPENLR2_SEEDMAP7K=/path/to/Unrandomizer_SeedMap7K.h cargo test -p domain -- --ignored
```

## ライセンス
MIT または Apache-2.0 のデュアルライセンス。
