//! アプリケーション層：ユースケースと「ポート」（依存性逆転のための trait）。
//! アダプタ（bms, bmson, LR2 スキン, sqlite…）はこの trait を実装する側。
//!
//! # 仕様ルール（テストと 1 対 1 対応）
//! | ID | ルール |
//! |---|---|
//! | APP-UC-01 | `load_chart` は `can_load` が真の最初のローダを使い、無ければ `Unsupported` |
//! | APP-UC-02 | `load_chart` はシードで `LegacyMt` を作り、ローダが消費した続きの乱数を返す（レーン RANDOM 用） |
//! | APP-UC-03 | `import_scores` は既存記録が無ければそのまま保存 |
//! | APP-UC-04 | `import_scores` は同じ譜面・同じルールの既存記録があれば `merge_best` して保存（ルールが違えば別記録） |
//! | APP-UC-05 | `import_scores` は取り込んだ件数を返す |
//! | APP-PLY-01 | プレイ入力: レーンは割り当てた物理キーのどれかが押されていれば押下中。割り当ての無いキーは無視 |
//! | APP-PLY-02 | プレイ入力: 1 フレームで渡すレーンの変化（押した・離した）は 1 回まで。残りは次のフレームに回す |
//! | APP-PLY-03 | プレイ: キー音と BGM はそのフレームの時刻で `AudioOutput` に渡す |
//! | APP-PLY-04 | プレイ描画: y = (ノーツの拍 − 今の拍) / 表示拍数。y ≤ 1 のものだけ返し、押下中の LN の始点は 0 で止める |
pub mod play;
pub mod ports;
pub mod usecase;
