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
| reprise-skin-lr2csv | LR2 csv (.lr2skin / .csv) デコーダ（実装済み。方言 `Lr2` / `Standard`）と画像フォント（.lr2font）の解析 |
| reprise-skin-beatoraja | beatoraja JSON スキンデコーダ（実装済み。Lua スキンは対象外） |

方針: 乱数・ファイル選択は trait で注入。タイマー/条件/数値は番号のまま保持し、意味付けは利用側。

## game/
| crate | 役割 |
|---|---|
| domain | 確定譜面・拍⇄時刻（TimingMap）・判定/ゲージ(Ruleset)・プレイ進行（オートプレイを含む）・ノーツの流れる位置（`play::scroll`: HS-FIX）・LR2互換乱数・スコア・スキンモデル・LR2 スキンの描画計算（`skin::lr2`: 状態を問い合わせて描く四角形の列を返す） |
| application | ユースケース（譜面読み込み・記録取り込み・プレイ: 入力→フレームのレーン状態、音の出力、描画位置、プレイオプションとプレイ中の操作（`play_options`）、LR2 スキン用のタイマー・条件・数値（`PlaySkin`））、ポート（ChartLoader / SkinLoader / ScoreImporter / ScoreRepository / Clock / AudioOutput） |
| adapters/chart-loader | reprise-chart-core → domain（小節→拍、MD5/SHA-256）、LegacyMt を RandomSource に適合、音・画像ファイルの解決（LR2 の拡張子違いの探し方） |
| adapters/skin-loader | reprise-skin-core → domain（`#TRANSCOLOR` の抜き色、フォント。画像フォントの画像はスキンの画像の後ろに足す）、実ファイル用 `FsResolver`（大小無視・`*` を LegacyMt で乱択） |
| adapters/score-import-lr2 | LR2 のスコア DB（sqlite）→ ScoreRecord |
| adapters/score-import-beatoraja | beatoraja / LR2oraja の score.db（sqlite）→ ScoreRecord（MD5 は songdata.db で引く） |
| adapters/score-db | reprise 自身の記録の保存先（sqlite。ScoreRepository の実装） |
| reprise | Bevy（入力=Controller, 描画=Presenter/View）と組み立て。プレイ画面を LR2 スキン（無ければ四角形）で描く（1 ループ = 1 フレームで入力を読み、垂直同期なしで回す。スキン画像は抜き色を、BGA の画像は黒を透明にしてから Bevy に渡し、同じ画像・合成モード・フィルタが続く四角形を毎フレーム 1 つのメッシュにまとめ、合成モードはマテリアルのブレンド設定で描く。システムフォントの文字列は Bevy の Text2d（OS のフォントを探す）で描き、幅が決まってから LR2 と同じ倍率で置く） |

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
- スキン描画（`skin::lr2`・`PlaySkin`）: キーフレームの引き方（ループ・止まったタイマーは −1）と加速の種類、SRC と DST が同じタイマーならコマ送りを最初のキーフレームから数える、描画順は最初の DST 行の行番号、α < 2 は描かない、数字のコマ数による 3 種類の並べ方、ゲージ 50 区間のちらつき、NOWJUDGE の添字（5 = PG … 0 = 空POOR。OpenLR2・lunaticvibesf・beatoraja の LR2 スキン読み込みで一致）とコンボのずらし方、ノーツは DST_NOTE の位置から 1 拍 = 480 × ハイスピード / 600 ピクセル（通過したノーツは判定位置に留まる）、抜き色の既定 (0, 255, 0)、小節線は判定ラインをまだ過ぎていないものを α 255 で描く、場面の時間（`#STARTINPUT` を過ぎたらタイマー 1、`#LOADSTART`+`#LOADEND` で 40、さらに `#PLAYSTART` 後に 41 = 譜面の先頭）、曲の長さ = オブジェクトの時刻 + 音の長さ の最大で、過ぎたらタイマー 2・`#FADEOUT` 後に閉じる（途中終了はタイマー 3 から `#CLOSE`）、HP 表示は差 × 10ms・スコア表示は 500ms で直線的に追いかける、皿は回す向きに 720°/s・それ以外は 360°/s、文字列は同じ番号の画像フォント（`#LR2FONT`）が無ければシステムフォント（`#FONT`）で、高さ = h / 大きさ・幅 = 高さの倍率 × min(1, w / 文字列の幅) に合わせる（画像フォントの文字番号は Shift_JIS を 32832 / 49281 ずらした値）、BGA は譜面の時刻で 1 層目・2 層目・POOR 層（初期値は 0 番の画像）を切り替え、BD 以下の判定から 500ms 未満は POOR 層だけ・それ以外は 1 層目（後ろに黒い四角形）と 2 層目を、256 未満の画像は縮めて横は中央・縦は上に寄せて描く（アルファの無い画像は黒が透明）。オートプレイは判定で鍵盤を押して 80ms で離す（LN の間は押し続ける）。合成モードは beatoraja の対応（2 = 加算、3 = 減算、4 = 乗算、9 = 反転 など）に合わせる。タイマー（0/1/2/40/41/3/42/44/46/48/50+/70+/100+/120+/140/143）・条件（32/33/41/42/43/81/160..164/170..173/230..248/270）・数値（10/12/14/100..107/110..115/160..164）・SLIDER（2/4/6）・BARGRAPH（1）を作る
- プレイオプション（`play::scroll`・`play_options`）: ハイスピードの既定 100・範囲 10..900・刻み 10、ノーツの位置は 1 拍 = 480 で表示は 位置の差 × ハイスピード × 倍率 / 600 ピクセル、HS-FIX の倍率 = 150 / BPM（ノーツの位置の BPM の最大・最小・平均・最多）、CONSTANT は止まっていない経過時間 1ms = 1.2、START/SELECT を押しながら鍵盤 1/3/5 で −・2/4 で +・6/7 でレーンカバー（無ければハイスピード）、START/SELECT の片方の 250ms 以内の 2 度押しでレーンカバーの表示を切り替え、レーンカバーは刻み 10・ホイールで 1 ずつ、判定タイミングは −199..199ms で見た目（とオートの判定）だけをずらす、オートプレイは判定時刻以下のノーツをすべて PG（地雷は発動させない）
- 記録の保存（`ScoreRecord::update_lr2`・`save_play`）: OpenLR2 の `SaveResult` に合わせ、ランプは良い方、EX スコアが上回ったときだけ判定内訳を替える、最大コンボは大きい方、最小 BP は小さい方（途中終了は片付いていないノーツも BP に数える）、プレイ回数・クリア回数（FAILED 以外）を数える。オートプレイは保存しない。HS-FIX CONSTANT はランプを EASY までにしてランプと回数だけ保存
- 判定・ゲージ（`Lr2Ruleset`）: `#RANK` 別の判定幅、TOTAL の整数化と既定式、HARD のダメージ係数（OpenLR2 自身も要確認としている式）、HP 30% 以下の被ダメージ × 0.6、GROOVE/EASY の 2% 下限
- プレイ進行（`Lr2Play`）: キー音（最後に片付いたノーツの音、不可視ノーツで差し替え）、BGM・不可視ノーツをレーンより先に処理、フレーム単位の処理（フレーム時刻の整数 ms で判定、レーン番号順、1 レーン 1 フレームにつき先頭ノーツ 1 つ）、BD の押下が次のノーツも巻き込む、LN の始点は GD を過ぎると見逃し、LN を終端の GD 幅より前に離すと BAD、地雷は GR 幅未満の押下か押しながらの通過で発動し POOR に数える、ゲージ 2% 未満で途中終了。フレームの間隔は PC ごとに違うので再現対象外（BPM 変化直後の時刻補正も未対応）

beatoraja の判定・ゲージ（`BeatorajaRuleset`）は beatoraja の 7 鍵ルール（`JudgeProperty`・`GaugeProperty` の SEVENKEYS）を仕様として参照した。

未対応（今後）: beatoraja の Lua スキン、`#SCROLL`・`#LNTYPE 2`・DSC/`#FP`、BGA の動画、bmson の `c`（続きから鳴らす）、判定・ゲージのうち段位（コース）ゲージ・LR2 の NONSTOP 判定幅・ロングノーツ終端の判定幅・beatoraja の 5 鍵/PMS/24 鍵ルールと `#DEFEXRANK`、beatoraja のプレイ進行（判定するノーツの選び方・LN の種類・ランプ）、キー設定・ゲームパッド・2P/DP/9 鍵・LN 終端の音。プレイオプションのうち: EFFECT の HIDDEN/SUDDEN（固定の範囲。OpenLR2 では範囲の計算に描画順の値を使っており、意図した値が確かめられないため）・オートスクラッチ（アシスト）・LIFT（OpenLR2 の拡張で LR2 には無い）・CONSTANT で BPM が変わる譜面のランプを EASY までにする扱い・スキン無しの表示での HS。記録のうち: ゴースト・オプション履歴・RANK/RATE の列・IR・プレイヤーの統計（総プレイ回数など）。LR2 スキン描画のうち: 合成は線形の色空間で混ぜる（LR2 は sRGB の値のまま混ぜるので加算などの明るさが少し違う）・システムフォントは OS のサンセリフ体で描き、LR2 の設定のフォント名・`#FONT` の種類（縁取り）・合成モードは反映しない・BGA の表示設定（大きさ・POOR 層の時間など）・BUTTON・2P 側・オート用ノーツ（`#SRC_AUTO_NOTE` など）・スキンの位置調整（adjust）・リザルト画面（場面が閉じたら結果を文字で出すだけ）、`#TRANSCOLOR` は位置にかかわらず全画像に適用、beatoraja スキンでの描画

## ビルド
各ディレクトリで `cargo test`（game は兄弟ディレクトリを path 参照するので3つ揃えて配置）。
CI はテストと lint だけ。クロスビルドは手元で `bash tools/cross-build.sh <target>`
