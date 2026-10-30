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

## ビルドと実行
```sh
cd game && cargo run -p reprise --release -- path/to/chart.bms [--gauge groove|easy|hard|death|pattack|gattack] [--skin path/to/play.csv [--skin-root path/to/LR2]]
```
1P の 7 鍵（5 鍵）のプレイ画面です。`--skin` に LR2 のプレイスキン（.lr2skin / .csv）を渡すとそれで描き、無ければ四角形だけで描きます。`--skin-root` はスキン中のパスの基準（LR2 本体のフォルダ。既定は今のフォルダ）。キーは皿 = 左 Shift / 左 Ctrl、鍵盤 1〜7 = Z S X D C F V、Esc で終了。

スキン描画は画像・数字・ゲージ・判定とコンボ・ノーツ（LN・地雷）・小節線・BARGRAPH・SLIDER・皿の回転・合成モード（加算・減算・乗算など）・フィルタ・場面の時間（`#STARTINPUT`・`#FADEOUT` など）に対応し、曲が終わるか途中終了すると結果を文字で出します。TEXT・BGA・2P 側・レーンカバーなどは描きません（[ARCHITECTURE.md](ARCHITECTURE.md#lr2-互換の根拠)）。

Linux では Bevy のために `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev` が必要です。

各 workspace で `cargo test`。CI は Linux / Windows / macOS でのテストと lint だけを行います。他アーキテクチャ向けのクロスビルドは手元で `bash tools/cross-build.sh <target>` を実行します（準備はスクリプト冒頭に記載）。`v*` タグで6ターゲットのバイナリを Release に添付します。

テストは rstest で「仕様ルール 1 つ = テスト関数 1 つ」に揃えています（[ARCHITECTURE.md](ARCHITECTURE.md#テスト方針)）。対応は `python3 tools/check_rules.py` で検査できます。

LR2互換乱数を OpenLR2 の対応表と全件照合する場合（任意）:
```sh
cd game && OPENLR2_SEEDMAP7K=/path/to/Unrandomizer_SeedMap7K.h cargo test -p domain -- --ignored
```

## ライセンス
MIT または Apache-2.0 のデュアルライセンス。
