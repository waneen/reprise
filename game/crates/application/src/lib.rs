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
pub mod ports;
pub mod usecase;
