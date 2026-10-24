# Architecture (reprise)

3つの独立した Cargo workspace（それぞれ別リポジトリ・別公開にできる）。

```
chart-formats/   譜面の汎用ライブラリ（ゲームを知らない）
skin-formats/    スキンの汎用ライブラリ（ゲームを知らない／譜面も知らない）
game/            LR2再現プレイヤー本体（上2つにpath依存。公開後は version/git 依存に置換）
```

依存は常に内向き（`→` は「依存する」）。chart-formats と skin-formats は互いに依存しない。

```
game/reprise (Bevy, Composition Root)
 ├→ game/adapters/*  ── 腐敗防止層: 汎用モデル ⇄ domain
 │    ├→ game/application (ユースケース + ポートtrait)
 │    │    └→ game/domain (依存ゼロ)
 │    └→ reprise-chart-core / reprise-skin-core
 └→ reprise-chart-bms, reprise-chart-bmson, reprise-skin-lr2csv, reprise-skin-beatoraja
```

## chart-formats/
| crate | 役割 |
|---|---|
| reprise-chart-core | 記法共通の譜面モデル、`ChartDecoder` / `RandomSource` trait、#RANDOM 解決 |
| reprise-chart-bms | bms/bme/bml/pms デコーダ |
| reprise-chart-bmson | bmson デコーダ |

## skin-formats/
| crate | 役割 |
|---|---|
| reprise-skin-core | 記法共通のスキンモデル、`SkinDecoder` / `FileResolver` trait |
| reprise-skin-lr2csv | LR2 csv (.lr2skin) デコーダ |
| reprise-skin-beatoraja | beatoraja json/lua デコーダ |

方針: 乱数・ファイル選択は trait で注入。タイマー/条件/数値は番号のまま保持し、意味付けは利用側。

## game/
| crate | 役割 |
|---|---|
| domain | 確定譜面・判定/ゲージ(Ruleset)・LR2互換乱数・スコア・スキンモデル |
| application | ユースケース、ポート（ChartLoader / SkinLoader / ScoreImporter / ScoreRepository / Clock / AudioOutput） |
| adapters/chart-loader | reprise-chart-core → domain、LegacyMt を RandomSource に適合 |
| adapters/skin-loader | reprise-skin-core → domain、LR2番号の意味付け |
| adapters/score-import-* | 他プレイヤーの score.db → ScoreRecord |
| reprise | Bevy（入力=Controller, 描画=Presenter/View）と組み立て |

## 記法の追加手順
1. `chart-formats/crates/`（または `skin-formats/crates/`）に crate を作り `ChartDecoder`（`SkinDecoder`）を実装
2. `game/crates/reprise/Cargo.toml` に依存を追加し、`src/main.rs` の登録リストに1行追加

## ビルド
各ディレクトリで `cargo test`（game は兄弟ディレクトリを path 参照するので3つ揃えて配置）
