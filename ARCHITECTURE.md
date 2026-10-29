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
| domain | 確定譜面・拍⇄時刻（TimingMap）・判定/ゲージ(Ruleset)・LR2互換乱数・スコア・スキンモデル・LR2 スキンの描画計算（`skin::lr2`: 状態を問い合わせて描く四角形の列を返す） |
| application | ユースケース（譜面読み込み・記録取り込み・プレイ: 入力→フレームのレーン状態、音の出力、描画位置、LR2 スキン用のタイマー・条件・数値（`PlaySkin`））、ポート（ChartLoader / SkinLoader / ScoreImporter / ScoreRepository / Clock / AudioOutput） |
| adapters/chart-loader | reprise-chart-core → domain（小節→拍、MD5/SHA-256）、LegacyMt を RandomSource に適合、音ファイルの解決（LR2 の拡張子違いの探し方） |
| adapters/skin-loader | reprise-skin-core → domain（`#TRANSCOLOR` の抜き色を含む）、実ファイル用 `FsResolver`（大小無視・`*` を LegacyMt で乱択） |
| adapters/score-import-lr2 | LR2 のスコア DB（sqlite）→ ScoreRecord |
| adapters/score-import-beatoraja | beatoraja / LR2oraja の score.db（sqlite）→ ScoreRecord（MD5 は songdata.db で引く） |
| reprise | Bevy（入力=Controller, 描画=Presenter/View）と組み立て。プレイ画面を LR2 スキン（無ければ四角形）で描く（1 ループ = 1 フレームで入力を読み、垂直同期なしで回す。スキン画像は抜き色を処理してから Bevy に渡し、四角形を毎フレームスプライトにする） |

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
- スキン: 5 バイト以下の行は無視、入れ子 `#IF` の内側は外側が偽でも評価、`#IF` で負値は偽、`#CUSTOMFILE` の RANDOM は 1 回だけ選ぶ、DST の第 21 引数は条件ではなく皿の回転の指定
- スキン描画（`skin::lr2`・`PlaySkin`）: キーフレームの引き方（ループ・止まったタイマーは −1）と加速の種類、SRC と DST が同じタイマーならコマ送りを最初のキーフレームから数える、描画順は最初の DST 行の行番号、α < 2 は描かない、数字のコマ数による 3 種類の並べ方、ゲージ 50 区間のちらつき、NOWJUDGE の添字（5 = PG … 0 = 空POOR。OpenLR2・lunaticvibesf・beatoraja の LR2 スキン読み込みで一致）とコンボのずらし方、ノーツは DST_NOTE の位置から 1 拍 = 480 × ハイスピード / 600 ピクセル（通過したノーツは判定位置に留まる）、抜き色の既定 (0, 255, 0)。タイマー（0/1/40/41/3/42/44/46/48/50+/70+/100+/120+/140/143）・条件（32/40/42/43/81/160..164/170..173/230..248）・数値（100..107/110..115/160..164）を作る
- 判定・ゲージ（`Lr2Ruleset`）: `#RANK` 別の判定幅、TOTAL の整数化と既定式、HARD のダメージ係数（OpenLR2 自身も要確認としている式）、HP 30% 以下の被ダメージ × 0.6、GROOVE/EASY の 2% 下限
- プレイ進行（`Lr2Play`）: キー音（最後に片付いたノーツの音、不可視ノーツで差し替え）、BGM・不可視ノーツをレーンより先に処理、フレーム単位の処理（フレーム時刻の整数 ms で判定、レーン番号順、1 レーン 1 フレームにつき先頭ノーツ 1 つ）、BD の押下が次のノーツも巻き込む、LN の始点は GD を過ぎると見逃し、LN を終端の GD 幅より前に離すと BAD、地雷は GR 幅未満の押下か押しながらの通過で発動し POOR に数える、ゲージ 2% 未満で途中終了。フレームの間隔は PC ごとに違うので再現対象外（BPM 変化直後の時刻補正も未対応）

beatoraja の判定・ゲージ（`BeatorajaRuleset`）は beatoraja の 7 鍵ルール（`JudgeProperty`・`GaugeProperty` の SEVENKEYS）を仕様として参照した。

未対応（今後）: beatoraja の Lua スキン、`#SCROLL`・`#LNTYPE 2`・DSC/`#FP`、BGA の動画、bmson の `c`（続きから鳴らす）、判定・ゲージのうち段位（コース）ゲージ・LR2 の NONSTOP 判定幅・ロングノーツ終端の判定幅・beatoraja の 5 鍵/PMS/24 鍵ルールと `#DEFEXRANK`、beatoraja のプレイ進行（判定するノーツの選び方・LN の種類・ランプ）、ハイスピードの変更・キー設定・ゲームパッド・2P/DP/9 鍵・LN 終端の音・BGA。LR2 スキン描画のうち: 加算などアルファ以外の合成（アルファ合成で描く）・フィルタ指定（常に最近傍）・TEXT・BGA・小節線・BUTTON・2P 側・リフト/レーンカバー/HIDDEN/SUDDEN・オート用ノーツ・スコアとゲージ表示の追いかけアニメーション・`#STARTINPUT` などの時間指定（タイマー 2 = 閉幕も未対応）・スキンの位置調整（adjust）・皿を回す向きの区別（押している間は常に逆回転）・スキン描画時のリザルト表示、`#TRANSCOLOR` は位置にかかわらず全画像に適用、beatoraja スキンでの描画

## ビルド
各ディレクトリで `cargo test`（game は兄弟ディレクトリを path 参照するので３つ揃えて配置）。
CI はテストと lint だけ。クロスビルドは手元で `bash tools/cross-build.sh <target>`
