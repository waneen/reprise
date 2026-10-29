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
| reprise-chart-core | 記法共通の譜面モデル、`ChartDecoder` / `RandomSource` trait、#RANDOM 解決・定義適用・LN 対応付け（`ResolvePolicy` でプレイヤー差を切替） |
| reprise-chart-bms | bms/bme/bml/pms デコーダ（実装済み。方言 `Lr2` / `Standard`） |
| reprise-chart-bmson | bmson デコーダ（実装済み。bmson 1.0.0） |

## skin-formats/
| crate | 役割 |
|---|---|
| reprise-skin-core | 記法共通のスキンモデル、`SkinDecoder` / `FileResolver` trait |
| reprise-skin-lr2csv | LR2 csv (.lr2skin / .csv) デコーダ（実装済み。方言 `Lr2` / `Standard`） |
| reprise-skin-beatoraja | beatoraja JSON スキンデコーダ（実装済み。Lua スキンは対象外） |

方針: 乱数・ファイル選択は trait で注入。タイマー/条件/数値は番号のまま保持し、意味付けは利用側。

## game/
| crate | 役割 |
|---|---|
| domain | 確定譜面・拍⇄時刻（TimingMap）・判定/ゲージ(Ruleset)・LR2互換乱数・スコア・スキンモデル |
| application | ユースケース、ポート（ChartLoader / SkinLoader / ScoreImporter / ScoreRepository / Clock / AudioOutput） |
| adapters/chart-loader | reprise-chart-core → domain（小節→拍、MD5/SHA-256）、LegacyMt を RandomSource に適合 |
| adapters/skin-loader | reprise-skin-core → domain、実ファイル用 `FsResolver`（大小無視・`*` を LegacyMt で乱択） |
| adapters/score-import-lr2 | LR2 のスコア DB（sqlite）→ ScoreRecord |
| adapters/score-import-beatoraja | beatoraja / LR2oraja の score.db（sqlite）→ ScoreRecord（MD5 は songdata.db で引く） |
| reprise | Bevy（入力=Controller, 描画=Presenter/View）と組み立て |

## 記法の追加手順
1. `chart-formats/crates/`（または `skin-formats/crates/`）に crate を作り `ChartDecoder`（`SkinDecoder`）を実装
2. `game/crates/reprise/Cargo.toml` に依存を追加し、`src/main.rs` の登録リストに1行追加

## テスト方針
- 各 crate の `lib.rs` 冒頭に「仕様ルール」表（ID とルール）を置く
- ルール 1 つにつき rstest のテスト関数 1 つ。関数名はルール ID で始め（例: `BMS-RND-L2-01` → `bms_rnd_l2_01_…`）、`#[case::名前]` でケースを明示する
- `python3 tools/check_rules.py` が表とテストの 1 対 1 を検査する（CI でも実行）
- 第三者の素材（LR2 本体・スキン・譜面、OpenLR2 のデータ）は同梱しない。テストの入力はテスト内で組み立てる

## 仕様の根拠
- bmson: bmson format specification 1.0.0（https://bmson-spec.readthedocs.io/）
- beatoraja の JSON スキン・score.db: beatoraja のソース（exch-bms2/beatoraja）を仕様として参照
- LR2 のスコア DB: OpenLR2 の `score` テーブル定義とランプ値

## LR2 互換の根拠
LR2 の挙動は OpenLR2（LR2 の逆コンパイル）を仕様として参照した。方言 `Lr2` で再現している主な癖:
- BMS: `#RANDOM` は入れ子なし・`#ELSE` 非対応、`#RANDOM` 直後〜最初の `#IF` までの行は捨てる、初期 `#BPM` は先勝ち、未定義 `#BPMxx` 参照は -1、`#TOTAL` は整数、位置 0 にノーツがあると全体を 1 小節ずらす、LN の保留を LNOBJ と共有
- スキン: 5 バイト以下の行は無視、入れ子 `#IF` の内側は外側が偽でも評価、`#IF` で負値は偽、`#CUSTOMFILE` の RANDOM は 1 回だけ選ぶ
- 判定・ゲージ（`Lr2Ruleset`）: `#RANK` 別の判定幅、TOTAL の整数化と既定式、HARD のダメージ係数（OpenLR2 自身も要確認としている式）、HP 30% 以下の被ダメージ × 0.6、GROOVE/EASY の 2% 下限
- プレイ進行（`Lr2Play`）: フレーム単位の処理（フレーム時刻の整数 ms で判定、レーン番号順、1 レーン 1 フレームにつき先頭ノーツ 1 つ）、BD の押下が次のノーツも巻き込む、LN の始点は GD を過ぎると見逃し、LN を終端の GD 幅より前に離すと BAD、地雷は GR 幅未満の押下か押しながらの通過で発動し POOR に数える、ゲージ 2% 未満で途中終了。フレームの間隔は PC ごとに違うので再現対象外（BPM 変化直後の時刻補正も未対応）

beatoraja の判定・ゲージ（`BeatorajaRuleset`）は beatoraja の 7 鍵ルール（`JudgeProperty`・`GaugeProperty` の SEVENKEYS）を仕様として参照した。

未対応（今後）: beatoraja の Lua スキン、`#SCROLL`・`#LNTYPE 2`・DSC/`#FP`、BGA の動画、bmson の `c`（続きから鳴らす）、判定・ゲージのうち段位（コース）ゲージ・LR2 の NONSTOP 判定幅・ロングノーツ終端の判定幅・beatoraja の 5 鍵/PMS/24 鍵ルールと `#DEFEXRANK`、beatoraja のプレイ進行（判定するノーツの選び方・LN の種類・ランプ）、描画（Bevy）

## ビルド
各ディレクトリで `cargo test`（game は兄弟ディレクトリを path 参照するので3つ揃えて配置）
