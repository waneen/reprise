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
| domain | 確定譜面・拍⇄時刻（TimingMap）・判定/ゲージ(Ruleset)・プレイ進行（オートプレイを含む）・ノーツの流れる位置（`play::scroll`: HS-FIX）・LR2互換乱数・スコア・選曲（`select`: フォルダと譜面の一覧の並べ方・カーソル・選曲スキンの一覧の動き `BarScroll`・LR2 のマウスの左ボタンの状態）・入力の割り当て（`input`: 1P・2P の操作ごとに 16 か所の位置に置く物理入力・軸の読み方・アナログ皿）・LR2 のキー設定画面の規則（`keyconfig`: キー番号・7 鍵の並び・BUTTON・割り当て・入力の名前）・スキンモデル・LR2 スキンの描画計算（`skin::lr2`: 状態を問い合わせて描く四角形の列を返す。プレイ画面 `draw_play`・選曲画面 `draw_select`・キー設定画面 `draw_keyconfig`・リザルト画面 `draw_result` は場面によらない要素の描き方を共有し、選曲は曲バーとマウスの当たり判定・BUTTON のクリックも、リザルトはゲージグラフも）・リザルトのゲージグラフの記録（`play::GaugeGraph`） |
| application | ユースケース（譜面読み込み・記録取り込み・曲一覧の走査（`scan_library`）・設定（`settings`）・プレイ: 入力→フレームのレーン状態、音の出力、描画位置、プレイオプションとプレイ中の操作（`play_options`）、LR2 スキン用のタイマー・条件・数値（プレイ `PlaySkin`・選曲 `SelectSkin`）、キー設定画面（`KeyConfigSkin`: タイマー・条件・文字列と 1 フレームの処理）、リザルト画面（`ResultSkin`: プレイの結果 `PlayResult` からのタイマー・条件・数値・BARGRAPH・文字列と 1 フレームの処理、プレイの後の行き先 `after_play`））、ポート（ChartLoader / SkinLoader / ScoreImporter / ScoreRepository / FileSystem / SettingsStore / Clock / AudioOutput） |
| adapters/chart-loader | reprise-chart-core → domain（小節→拍、MD5/SHA-256）、LegacyMt を RandomSource に適合、音・画像ファイルの解決（LR2 の拡張子違いの探し方） |
| adapters/skin-loader | reprise-skin-core → domain（`#TRANSCOLOR` の抜き色、場面の時間、`#BAR_CENTER`・`#BAR_AVAILABLE`、フォント。画像フォントの画像はスキンの画像の後ろに足す）、FileSystem の上の `FsResolver`（大小無視・`*` を LegacyMt で乱択） |
| adapters/score-import-lr2 | LR2 のスコア DB（sqlite）→ ScoreRecord |
| adapters/score-import-beatoraja | beatoraja / LR2oraja の score.db（sqlite）→ ScoreRecord（MD5 は songdata.db で引く） |
| adapters/score-db | reprise 自身の記録の保存先（sqlite。ScoreRepository の実装。ネイティブ版だけ） |
| adapters/settings-json | 設定の保存先（JSON ファイル。SettingsStore の実装。ネイティブ版だけ） |
| adapters/file-system | FileSystem の実装（ディスクの `StdFs`、ブラウザ版でページから渡されたファイルを置く `MemFs`） |
| reprise | Bevy（入力=Controller, 描画=Presenter/View）と組み立て。プレイ画面を LR2 スキン（無ければ四角形）で描く（1 ループ = 1 フレームで入力を読み、垂直同期なしで回す。スキン画像は抜き色を、BGA の画像は黒を透明にしてから Bevy に渡し、同じ画像・合成モード・フィルタが続く四角形を毎フレーム 1 つのメッシュにまとめ、合成モードはマテリアルのブレンド設定で描く。システムフォントの文字列は Bevy の Text2d（OS のフォントを探す）で描き、幅が決まってから LR2 と同じ倍率で置く）。プレイ画面・選曲画面・キー設定画面・リザルト画面の描画（メッシュのまとめ方・合成モード・Text2d）は `skin_render` で共用する。場面（選曲 → プレイ → リザルト → 選曲かもう一度プレイ、選曲 → キー設定 → 選曲）は Bevy の States で切り替え、選曲画面（`select_scene`）は選曲スキンがあればそれで、無ければ文字の一覧で描き、キー設定画面（`keyconfig_scene`）はキー設定スキンで描く（無ければ入らない）。リザルト画面（`result_scene`）はリザルトスキンで描く（無ければプレイの場面で結果を文字で出す）。キーボードとゲームパッドの入力は毎フレーム場面の処理より先に割り当てで操作に直し、押された物理入力も渡す（`input`）。ネイティブとブラウザ（wasm）で違うところ（起動引数・ファイル・記録の保存先・乱数の種）は `platform` にまとめる |

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
LR2 の挙動は OpenLR2（作者の説明では LR2 のソースを書き起こしたもの）を仕様として参照した（コード・データは転載していない）。方言 `Lr2` で再現している主な癖:
- BMS: `#RANDOM` は入れ子なし・`#ELSE` 非対応、`#RANDOM` 直後〜最初の `#IF` までの行は捨てる、初期 `#BPM` は先勝ち、未定義 `#BPMxx` 参照は -1、`#TOTAL` は整数、位置 0 にノーツがあると全体を 1 小節ずらす、LN の保留を LNOBJ と共有
- スキン: 5 バイト以下の行は無視、入れ子 `#IF` の内側は外側が偽でも評価、`#IF` で負値は偽、`#CUSTOMFILE` の RANDOM は 1 回だけ選ぶ、DST の第 21 引数は条件ではなく皿の回転の指定
- スキン描画（`skin::lr2`・`PlaySkin`）: キーフレームの引き方（ループ・止まったタイマーは −1）と加速の種類、SRC と DST が同じタイマーならコマ送りを最初のキーフレームから数える、描画順は最初の DST 行の行番号、α < 2 は描かない、数字のコマ数による 3 種類の並べ方、ゲージ 50 区間のちらつき、NOWJUDGE の添字（5 = PG … 0 = 空POOR。OpenLR2・lunaticvibesf・beatoraja の LR2 スキン読み込みで一致）とコンボのずらし方、ノーツは DST_NOTE の位置から 1 拍 = 480 × ハイスピード / 600 ピクセル（通過したノーツは判定位置に留まる）、抜き色の既定 (0, 255, 0)、小節線は判定ラインをまだ過ぎていないものを α 255 で描く、場面の時間（`#STARTINPUT` を過ぎたらタイマー 1、`#LOADSTART`+`#LOADEND` で 40、さらに `#PLAYSTART` 後に 41 = 譜面の先頭）、曲の長さ = オブジェクトの時刻 + 音の長さ の最大で、過ぎたらタイマー 2・`#FADEOUT` 後に閉じる（途中終了はタイマー 3 から `#CLOSE`）、HP 表示は差 × 10ms・スコア表示は 500ms で直線的に追いかける、皿は回す向きに 720°/s・それ以外は 360°/s、文字列は同じ番号の画像フォント（`#LR2FONT`）が無ければシステムフォント（`#FONT`）で、高さ = h / 大きさ・幅 = 高さの倍率 × min(1, w / 文字列の幅) に合わせる（画像フォントの文字番号は Shift_JIS を 32832 / 49281 ずらした値）、BGA は譜面の時刻で 1 層目・2 層目・POOR 層（初期値は 0 番の画像）を切り替え、BD 以下の判定から 500ms 未満は POOR 層だけ・それ以外は 1 層目（後ろに黒い四角形）と 2 層目を、256 未満の画像は縮めて横は中央・縦は上に寄せて描く（アルファの無い画像は黒が透明）。オートプレイは判定で鍵盤を押して 80ms で離す（LN の間は押し続ける）。合成モードは beatoraja の対応（2 = 加算、3 = 減算、4 = 乗算、9 = 反転 など）に合わせる。タイマー（0/1/2/40/41/3/42/44/46/48/50+/70+/100+/120+/140/143）・条件（32/33/41/42/43/81/160..164/170..173/230..248/270）・数値（10/12/14/100..107/110..115/160..164）・SLIDER（2/4/6）・BARGRAPH（1）を作る
- プレイオプション（`play::scroll`・`play_options`）: ハイスピードの既定 100・範囲 10..900・刻み 10、ノーツの位置は 1 拍 = 480 で表示は 位置の差 × ハイスピード × 倍率 / 600 ピクセル、HS-FIX の倍率 = 150 / BPM（ノーツの位置の BPM の最大・最小・平均・最多）、CONSTANT は止まっていない経過時間 1ms = 1.2、START/SELECT を押しながら鍵盤 1/3/5 で −・2/4 で +・6/7 でレーンカバー（無ければハイスピード）、START/SELECT の片方の 250ms 以内の 2 度押しでレーンカバーの表示を切り替え、レーンカバーは刻み 10・ホイールで 1 ずつ、判定タイミングは −199..199ms で見た目（とオートの判定）だけをずらす、オートプレイは判定時刻以下のノーツをすべて PG（地雷は発動させない）
- 記録の保存（`ScoreRecord::update_lr2`・`save_play`）: OpenLR2 の `SaveResult` に合わせ、ランプは良い方、EX スコアが上回ったときだけ判定内訳を替える、最大コンボは大きい方、最小 BP は小さい方（途中終了は片付いていないノーツも BP に数える）、プレイ回数・クリア回数（FAILED 以外）を数える。オートプレイと、PG・GR・GD が 1 つも無いプレイ（`Main.cpp` の場面 4 の閉じ方）は保存しない。HS-FIX CONSTANT はランプを EASY までにしてランプと回数だけ保存
- 選曲スキン（`skin::lr2::draw_select`・`select::BarScroll`・`SelectSkin`）: OpenLR2 の `Scene02_Songselect.cpp`（曲バーの描画・`GetSongCursor`・入力）・`LR2_skinload.cpp`（`#SRC_BAR_TITLE` の並び・`#BAR_CENTER` は 0..29 だけ受ける・DST_BAR_BODY_OFF/ON は添字ごと）・`LR2_skindraw.cpp`・`LR2_skinobject.cpp`・`En_value.cpp`・`En_input.cpp` に合わせた。曲バーはスロット 1..29 で OFF の i−1 と i があるところに (位置/1000 − BAR_CENTER + i) を本数で回したバーを、選んでいるスロットは ON・それ以外は OFF の DST をタイマー 0 で引き、位置の端数で i と i−1 を補間して BAR_BODY[種類（曲 0・フォルダ 1）] を描く（DST の条件は見ない）。FLASH は選んでいるスロットに、TITLE[0]・LEVEL[#DIFFICULTY]（曲だけ）・LAMP[ランプ] はバーの位置（整数に切り捨て）からの相対で描く。一覧の位置は 1 本 = 1000 で、キーは目標を ±1000（押して 300ms・押し続けて 70ms 待つ）、ホイールは 200ms、動きは `ChangeValueByTime` の線形を 0 の方へ切り捨て、カーソルは `GetSongCursor`（下向きに動いている途中は次のバー）。マウスは押し続けたバーを選び（`#BAR_AVAILABLE` の外なら (i − 端) × 2000 動かす）、選んでいるバーを押す（状態 1）と決定、BUTTON は押した位置で SRC 引数 12..14（クリック・パネル・向き）を見る（パネルは開かないので −1）。マウスの左ボタンの状態は `En_input.cpp` の 0/1/2/3（離しても押してから 32ms 以内は 2）。タイマー 0/1/10/11、条件 1..5（バーの種類）・20・50・52・100..105（ランプ）・150..155（#DIFFICULTY）・160..164（鍵盤数）・185/186（レベル）、数値 10/12/14・42/96・71/75..84、文字列 10..14/17/18/20..24/27/28、BARGRAPH 3、SLIDER 1、BUTTON 1..8/10..19 の状態値 0 を作る（絞り込み・並べ替えは無いので LR2 の 0 = すべて・ディレクトリ順）。決定・戻る・Esc は文字の一覧と同じ。BUTTON は 13 キー設定（キー設定スキンがあればキー設定画面へ。無ければ何もしない）・15 プレイ・16 オートプレイが効き、14 スキン選択はクリックを受けて何もしない。フォルダを開いた・戻ったときは選んでいるスロットのままカーソルの位置で止める（OpenLR2 も戻ったときは開いたフォルダのパスを一覧で探してそこに置く。見つからなければ先頭）
- キー設定（`domain::keyconfig`・`KeyConfigSkin`・`keyconfig_scene`）: OpenLR2 の `Scene06_Keyconfig.cpp`（`ProcI_Keyconfig`・`ProcS_Keyconfig`・`GetKeyIDname`・`ConfigButtonToKeyID7`/`FromKeyID7`）・`LR2_skinobject.cpp`（`SetObjectValue_Button` の 100..157・条件 400..402）・`Scene02_Songselect.cpp`（`is_clicked_keyconfig` → 場面 6）・`Main.cpp`（場面 6 の開始）・`LR2_configsave.cpp`・`En_input.cpp`（`InputToButton` のキー番号）・`LR2_gameloop.cpp`（`ReactInput` のタイマー 100..129）に合わせ、lunaticvibesf の `scene_keyconfig.cpp`・`skin_lr2_button_callbacks.cpp`（`enter_key_config`・`key_config_pad`・`key_config_slot`。Esc と右クリックで閉じる・Delete で外す）でも確かめた。キー設定スキンは `#INFORMATION` の種別 8（OpenLR2 `SKINTYPE_KEYCONFIG = 8`、スキンのパスの 8 番）。割り当ては LR2 と同じくキー番号（1..7 鍵盤・10/11 皿・12 START・13 SELECT・21..27/30..33 は 2P）ごとに 16 か所の位置に置き、同じ入力をいくつにも置ける。画面に入ると 7 鍵の並び 1・キー番号 1・位置なし。BUTTON 100..139 はキー番号 n−100（状態値は選んでいれば 1）、140 は 7 鍵の並びを ±1（0..22 で回る。状態値は並びの番号）、150..157 は位置（状態値は選んでいれば 1）を選ぶ。位置を選んでいる間は、押した入力（Esc・Enter・矢印・F1..F12 は除く）をその位置に置き（Delete も押していれば外す）、そのたびに保存する。位置の選択は解けない。F1 で既定の割り当て（保存する）、F2 ですべて外す（保存しない。選曲に戻ると保存した割り当てに戻る）。文字列 40..47 を `-` にし、キー番号が 0 でなければ 40..55 に位置 0..15 の名前（`GetKeyIDname` の名前。キーは DirectInput のキー番号 = キーの位置で対応させる）。条件 400（7 鍵）。タイマーは 0・1/11（`#STARTINPUT`）・2（閉じ始め）・100..107/120..127（1P の皿・鍵盤の押下と離し）。Esc か右クリックでタイマー 2 を始め、`#FADEOUT` を過ぎたら選曲に戻る（閉じ始めた後もクリックは受ける）。キー設定スキンが無ければ BUTTON 13 は何もしない（LR2 にも代わりの画面は無い）。F1/F2 は OpenLR2 では F1 = 既定・F2 = すべて外す、lunaticvibesf では逆なので、LR2 の書き起こしである OpenLR2 に合わせた
- リザルト（`ResultSkin`・`draw_result`・`GaugeGraph`・`result_scene`）: OpenLR2 の `Scene05_Result.cpp`（`ProcI_Result`・`Proc_Result`。書き起こしの元の枝 `LR2Beta3-v100201` の `LR2f.cpp` でも同じ）・`Main.cpp`（場面 4 を閉じたときの行き先・場面 5 の閉じ方・タイマー 1）・`LR2_skinload.cpp`（`#STARTINPUT` の第 2・第 3 引数 = `startinput_rank`・`startinput_update`、`#SRC_GAUGECHART_1P`）・`LR2_skinmanage.cpp`（`SKINTYPE_RESULT` = 7、スキンのパスの 7 番）・`LR2_statplay.cpp`（`LogGraphPlayData`・`LogGraphPlayerDataToEnd`・`SaveResult` の `sSelect.old`）・`Scene04_Play.cpp`（グラフの記録の時刻と曲の終わりの埋め方）・`LR2_skinobject.cpp`（条件 90/91・300..347、数値 100..116・170..184、BARGRAPH 10..27）・`LR2_skindraw.cpp`（`AddDrawingBuffer_Object`）に合わせ、lunaticvibesf の `scene_result.cpp`（DRAW → STOP → RECORD → FADEOUT の 3 段と、閉じる間の決定 + 取り消しの押し続けでもう一度）でも確かめた。プレイを閉じたら、オートプレイ・片付けたノーツ 0・PG/GR/GD が無いプレイは選曲、それ以外は記録を保存してリザルト。プレイ中の途中終了（`ProcI_Play`。リザルトスキンがあるとき）は、閉じ始める前に Esc・マウスの右の状態 2・START と SELECT の押し続け・全ノーツを片付けた後の START か SELECT の押し続けでタイマー 2 を始め、以後は判定せず、`#FADEOUT` の後に同じく場面を閉じる（片付いていないノーツがあれば HP を 0 にし、グラフの残りを 0 で埋める。`Scene04_Play.cpp` のプレイの終わり）。タイマー 0・1/11（`#STARTINPUT`）・150（グラフの伸び始め = GAUGECHART_1P[0] の SRC 引数 13）・151（伸び終わり = 引数 14、またはランクを出す入力）・152（記録更新）・2（閉じ始め）・100..107/120..127（鍵盤）。入力は鍵盤 1〜7 の押下・Esc・Enter・マウスの左右の状態 2 で、151 → （151 が第 2 引数を過ぎたら）152（保存していなければ閉じ始め）→ （152 が第 3 引数を過ぎたら）閉じ始め。`#FADEOUT` を過ぎて閉じるとき白鍵と黒鍵を押し続けていれば同じ譜面をもう一度（`flag_retry`。LR2 は譜面を読み直さず `InitGameplay_retry` で同じノーツの並びを使う。reprise は同じ乱数の種で読み直すので `#RANDOM` の選び方は同じ（ADP-CH-07）で、レーンの RANDOM は無いので並びも同じになる。スキンは LR2 と同じく場面に入るたびに読み直す）。ゲージグラフは曲の長さを 1000 区間に分けて HP を記録し（途中終了は残りを 0）、幅 = SRC 引数 11 をタイマー 0 で伸ばし、DST の幅ごとに HP まで 1 ずつ点を打つ（GROOVE・EASY の 80 未満は 0 番、それ以外は 1 番）。BP（数値 177/178・条件 332）は BD + POOR + 片付いていないノーツ、BARGRAPH 24 は POOR。LR2 の POOR（`judgecount[1]`）は空POOR（`ApplyJudgeNote` の 0）と地雷（`ApplyJudgeMine`）も数え、数値 114 は `judgecount[0]`（地雷の数）+ `judgecount[1]`（プレイ画面の 114 も同じ）。条件 32・42/43・90/91・160..164・300..308（ランク = EX × 9 / (総ノーツ × 2)）・320..327（保存前の記録のランク。総ノーツ数は記録に無いので今の譜面の数を使い、記録が無ければ LR2 の空の記録と同じくどれも偽）・330..332/335・340..347（LR2 はこのプレイの EX で決める）・350、数値 10/12/14・100..107・110..116・170..178・183/184、BARGRAPH 10/11/20..27（26 は LR2 と同じく最大 20000/10000）、文字列 10..15 を作る
- 選曲（`select`・`scan_library`）: 最初の一覧は登録したフォルダ、フォルダを開くと子フォルダの中の譜面（LR2 の `song.parent` = 譜面のフォルダの親）を、無ければ子フォルダを並べる（`curQuery` を順に試す）。譜面はディレクトリ順（`CMP_SongDataByDirectory`: フォルダ → 鍵盤数 → レベル、`#DIFFICULTY` 5 は後ろ）、フォルダはパス順（`CMP_SongDataByPath`）、どちらも大文字にして比べる。同じ MD5 は 1 つ。一覧は端で回り、スクロールは押して 300ms・押し続けて 70ms ごと（`speedfirst`・`speednext` の既定）。皿・↑↓で選び、鍵盤 1/3/5/7・Enter・→で決定、鍵盤 6 でオートプレイ、鍵盤 2・←で戻る。LR2 の `qsort` は安定でないので、同順位はパス順にした。戻ったときのカーソルは開く前の位置（OpenLR2 は開いたフォルダのパスで一覧を探してその位置に置くので、一覧が変わらなければ同じ）
- 入力（`input`）: 同じ物理入力をいくつの操作に置いてもよく、置いた操作はどれも押される（LR2 の `InputToButton` と同じ）。軸をボタンとして読む閾値 ±0.9 と、アナログ皿（動いた向きに回したとし、止まって閾値を過ぎたら離す。端で回り込む）は beatoraja の `BMControllerInputProcessor`（版 1）に倣う。beatoraja は閾値を読み取り回数で数えるが、ここでは時間（既定 100ms）で持つ。アナログ皿は LR2 には無い機能で、LR2 方式（軸をボタンとして読む）と設定で切り替える。LR2 のゲームパッドの読み方（DxLib の `GetJoypadInputState` で 3 台を区別し、方向とボタンを 32 ビットで読む）の方向の閾値は確かめていない
- 判定・ゲージ（`Lr2Ruleset`）: `#RANK` 別の判定幅、TOTAL の整数化と既定式、HARD のダメージ係数（OpenLR2 自身も要確認としている式）、HP 30% 以下の被ダメージ × 0.6、GROOVE/EASY の 2% 下限
- プレイ進行（`Lr2Play`）: キー音（最後に片付いたノーツの音、不可視ノーツで差し替え）、BGM・不可視ノーツをレーンより先に処理、フレーム単位の処理（フレーム時刻の整数 ms で判定、レーン番号順、1 レーン 1 フレームにつき先頭ノーツ 1 つ）、BD の押下が次のノーツも巻き込む、LN の始点は GD を過ぎると見逃し、LN を終端の GD 幅より前に離すと BAD、地雷は GR 幅未満の押下か押しながらの通過で発動し POOR に数える、ゲージ 2% 未満で途中終了。フレームの間隔は PC ごとに違うので再現対象外（BPM 変化直後の時刻補正も未対応）

beatoraja の判定・ゲージ（`BeatorajaRuleset`）は beatoraja の 7 鍵ルール（`JudgeProperty`・`GaugeProperty` の SEVENKEYS）を仕様として参照した。

未対応（今後）: リザルトのうち: IR（文字列 20 の IR の知らせ・条件 600..621・334・数値 92..95/179..182、IR の返事を待つ入力）・ライバル（条件 624..657・数値 270..292）・ゴースト/目標スコア（SCORECHART 1/2・数値 150..158・BARGRAPH 12..15）・SCORECHART 0（EX スコアのグラフ。記録していない）・リプレイの保存（条件 82..84）・コースのリザルト（種別 15・場面 13）・2P/バトル/`#FLIP_RESULT`（GAUGECHART_2P・条件 210..217/310..318/351..354・数値 120..136）・クリアと失敗の効果音・選曲の記録の数値と条件（70..89・100..105・110..149）・プレイ中の条件（200..207・220..227・230..248）・トライアル（333）・次のランクまでの数値（109/154）・Lunaris・GAS（ゲージの切り替え。OpenLR2 の拡張）・動画の書き出し。リザルトスキンが無いとき（プレイスキン無しで遊んだとき・ブラウザ版を含む）は結果を文字で出し、プレイ中の途中終了は Esc ですぐ選曲に戻る（保存しない。右クリック・START+SELECT の途中終了は無い）。選曲スキンのうち: 新着の題名（BAR_TITLE[1]。追加日時が無い）・ライバル/IR の部品（BAR_MY_LAMP・BAR_RIVAL_LAMP・BAR_RANK・BAR_RIVAL・ランキング表示のタイマー 175/176）・BAR_STAR・`#DST_BAR_STAGEFILE`・`#SRC_MOUSECURSOR`・ONMOUSE・README・フォルダのランプ（LR2 の `folderlamp`。既定は切）とフォルダのレベル・パネル（START で開く）と BUTTON の 1..12・17..19・20 以上の動作・SLIDER のドラッグ・ボタンや決定の効果音・場面の `#FADEOUT`、作れない数値（BPM 90/91、難易度ごとのレベル 45..49、総ノーツ数が要るスコア・率 70/72..74/85..89、日時 21..26、プレイヤーの統計 30..41、IR 92..95）・条件（10..13 のバトル、46/47 の難易度の絞り込みの設定、70..79 のレベルバー、110..149 のランク・オプション履歴、170..183 の BGA・LN・BPM 変化・ランダム・判定、190..197 のステージファイル・バナー・リプレイ）・文字列（15/25 副アーティスト、16/26 タグ、19/29 exlevel、60 以降のオプションの名前）・BARGRAPH 5..9・バナー。選曲のうち: 曲一覧のキャッシュ（song.db。今は起動のたびに読み直す）・並べ替え（レベル・題名・ランプ・スコア）・鍵盤数と難易度の絞り込み・`folderinfo.txt`・カスタムフォルダ（`.lr2folder`）・コース・検索・プレビュー音・決定画面。beatoraja の Lua スキン、`#SCROLL`・`#LNTYPE 2`・DSC/`#FP`、BGA の動画、bmson の `c`（続きから鳴らす）、判定・ゲージのうち段位（コース）ゲージ・LR2 の NONSTOP 判定幅・ロングノーツ終端の判定幅・beatoraja の 5 鍵/PMS/24 鍵ルールと `#DEFEXRANK`、beatoraja のプレイ進行（判定するノーツの選び方・LN の種類・ランプ）、キー設定のうち: 9 鍵・5 鍵の割り当てと切り替え（BUTTON 141..143。reprise は 7 鍵の割り当てを 5 鍵にも使う）・reprise に無いキー番号（9 鍵の 8・9 など）への割り当て（選べるが何も置かない）・2P の割り当てを入力として読むこと（保存はする）と 2P のタイマー 110..119/130..139・ゲームパッドの区別（どのパッドでも同じ扱い）と LR2 のパッドの名前（`1P:BUTTON1` など。reprise の名前で出す）・LR2 に名前の無いキー（reprise の名前で出す）・同じフレームに 2 つ押したときの選び方（LR2 は入力の番号の小さい方、reprise は先に届いた方）・MIDI・効果音（`option_change`）・選曲からキー設定に移るときの選曲スキンの `#FADEOUT`・パネル（無いので −1 として BUTTON を押す）、2P/DP/9 鍵・LN 終端の音。プレイオプションのうち: EFFECT の HIDDEN/SUDDEN（固定の範囲。OpenLR2 では範囲の計算に描画順の値を使っており、意図した値が確かめられないため）・オートスクラッチ（アシスト）・LIFT（OpenLR2 の拡張で LR2 には無い）・CONSTANT で BPM が変わる譜面のランプを EASY までにする扱い・スキン無しの表示での HS。記録のうち: ゴースト・オプション履歴・RANK/RATE の列・IR・プレイヤーの統計（総プレイ回数など）。LR2 スキン描画のうち: 合成は線形の色空間で混ぜる（LR2 は sRGB の値のまま混ぜるので加算などの明るさが少し違う）・システムフォントは OS のサンセリフ体で描き、LR2 の設定のフォント名・`#FONT` の種類（縁取り）・合成モードは反映しない・BGA の表示設定（大きさ・POOR 層の時間など）・プレイ画面の BUTTON・2P 側・オート用ノーツ（`#SRC_AUTO_NOTE` など）・スキンの位置調整（adjust）、`#TRANSCOLOR` は位置にかかわらず全画像に適用、beatoraja スキンでの描画

## ビルド
各ディレクトリで `cargo test`（game は兄弟ディレクトリを path 参照するので3つ揃えて配置）。
CI はテストと lint だけ。クロスビルドは手元で `bash tools/cross-build.sh <target>`、ブラウザ版は `bash tools/web-build.sh`（`web/` を配信）
