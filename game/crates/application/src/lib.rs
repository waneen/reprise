//! アプリケーション層：ユースケースと「ポート」（依存性逆転のためのtrait）。
//! アダプタ（bms, bmson, LR2スキン, sqlite…）はこのtraitを実装する側。
pub mod ports;
pub mod usecase;
