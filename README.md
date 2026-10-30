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
cd game && cargo run -p reprise --release -- [--songs path/to/BMS...] [path/to/chart.bms] [--gauge groove|easy|hard|death|pattack|gattack] [--skin path/to/play.csv [--skin-root path/to/LR2]] [--hs 100] [--hsfix off|max|min|avg|const|main] [--cover 0..100] [--offset ms] [--auto] [--data path/to/folder]
```
`--songs` に譜面のフォルダ（何個でも）を渡すと選曲画面から始まります。起動のたびにフォルダの中を読み直します。フォルダを開くと、その中の曲フォルダの譜面が並びます（曲フォルダが無ければ中のフォルダが並びます。LR2 と同じ）。皿（左 Shift = 上・左 Ctrl = 下）か ↑↓ で選び、鍵盤 1/3/5/7・Enter・→ で決定、鍵盤 6 でオートプレイ、鍵盤 2・← で戻り、最初の一覧で Esc なら終了します。プレイ中に Esc で選曲に戻ります。譜面ファイルを直接渡すとその譜面のプレイだけをして、Esc で終了します。

設定は `--data` のフォルダの `config.json` に保存し、次からは引数無しで起動できます（起動引数で渡した `--songs`・`--skin`・`--skin-root`・`--gauge`・`--hs`・`--hsfix`・`--cover`・`--offset` も保存します）。キーの割り当ては今は `config.json` の `keys` を書き換えて変えます（LR2 と同じく選曲スキンのボタンからキー設定スキンの画面に入る作りは、選曲スキンの描画のあとに入れます）。ゲームパッドの軸は ±0.9 でボタンとして読み、`analog_scratch_ms` に数値（ms）を入れると皿に割り当てた軸を回した向きで読むアナログ皿になります（beatoraja と同じ方式。LR2 には無い）。

1P の 7 鍵（5 鍵）のプレイ画面です。`--skin` に LR2 のプレイスキン（.lr2skin / .csv）を渡すとそれで描き、無ければ四角形だけで描きます。`--skin-root` はスキン中のパスの基準（LR2 本体のフォルダ。既定は今のフォルダ）。キーの既定は皿 = 左 Shift / 左 Ctrl、鍵盤 1〜7 = Z S X D C F V、START = Q、SELECT = W。

プレイオプション（スキンで描くときに効きます）: `--hs` はハイスピード ×100（LR2 の既定 100）、`--hsfix` は HS-FIX、`--cover` はレーンカバー（量 0〜100）、`--offset` は判定タイミング（ms。ノーツの見た目をずらす）、`--auto` はオートプレイ。プレイ中は START か SELECT を押しながら 鍵盤 1/3/5 でハイスピード −0.1・2/4 で +0.1、6/7 でレーンカバーの上げ下げ（レーンカバーが無ければハイスピード）、START か SELECT の 2 度押しでレーンカバーの表示を切り替え、マウスホイールでレーンカバーを動かせます。

遊び終えると記録を `--data` のフォルダ（既定は今のフォルダの `reprise-data`）の `score.db` に LR2 と同じ規則で保存し、結果に保存前後の記録を出します（オートプレイは保存せず、HS-FIX CONSTANT はランプを EASY までにして回数だけ保存）。

スキン描画は画像・数字・ゲージ・判定とコンボ・ノーツ（LN・地雷）・小節線・BARGRAPH・SLIDER・皿の回転・合成モード（加算・減算・乗算など）・フィルタ・場面の時間（`#STARTINPUT`・`#FADEOUT` など）・文字列（画像フォント `.lr2font` と OS のフォント）・BGA（画像。1 層目・2 層目・POOR 層）に対応し、曲が終わるか途中終了すると結果を文字で出します。BGA の動画・2P 側・EFFECT の HIDDEN/SUDDEN などは描きません（[ARCHITECTURE.md](ARCHITECTURE.md#lr2-互換の根拠)）。

### ブラウザ版
`bash tools/web-build.sh` で `web/pkg/` に wasm を作り、`web/` を静的に配信して開きます（例: `python3 -m http.server -d web 8000`）。ページで譜面のフォルダ（と、使うなら LR2 本体のフォルダ）を選ぶと、そのファイルをページの中だけで読んで遊べます（選曲画面は使わず、ページで譜面を選びます）。ネイティブ版との違い: 画面の更新に合わせてしかフレームが回らない（判定の細かさは約 1 フレーム）、記録と設定は保存しない（キーは既定の割り当て）、OS のフォントは使えず、フォント指定の文字は内蔵の英数字フォントで出す（日本語は表示できない。LR2FONT の画像フォントは使える）、音の遅延が大きいことがある。

Linux では Bevy のために `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev libfontconfig-dev` が必要です。

各 workspace で `cargo test`。CI は Linux / Windows / macOS でのテストと lint だけを行います。他アーキテクチャ向けのクロスビルドは手元で `bash tools/cross-build.sh <target>` を実行します（準備はスクリプト冒頭に記載）。`v*` タグで6ターゲットのバイナリを Release に添付します。

テストは rstest で「仕様ルール 1 つ = テスト関数 1 つ」に揃えています（[ARCHITECTURE.md](ARCHITECTURE.md#テスト方針)）。対応は `python3 tools/check_rules.py` で検査できます。

LR2互換乱数を OpenLR2 の対応表と全件照合する場合（任意）:
```sh
cd game && OPENLR2_SEEDMAP7K=/path/to/Unrandomizer_SeedMap7K.h cargo test -p domain -- --ignored
```

## ライセンス
MIT または Apache-2.0 のデュアルライセンス。
