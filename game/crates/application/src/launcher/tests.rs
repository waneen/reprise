//! 仕様ルール（lib.rs の表）とテスト関数は 1 対 1。関数名の接頭辞がルール ID。
use super::*;
use crate::settings::ScreenMode;
use rstest::rstest;
use std::path::PathBuf;

/// APP-LCH-01: 一覧に足す
#[rstest]
#[case::added(&["/a"], " /b ", true, &["/a", "/b"])]
#[case::empty(&["/a"], "  ", false, &["/a"])]
#[case::same(&["/a"], "/a", false, &["/a"])]
fn app_lch_01_add_entry(
    #[case] before: &[&str],
    #[case] text: &str,
    #[case] added: bool,
    #[case] after: &[&str],
) {
    let mut paths: Vec<PathBuf> = before.iter().map(PathBuf::from).collect();
    assert_eq!(add_entry(&mut paths, text), added);
    assert_eq!(paths, after.iter().map(PathBuf::from).collect::<Vec<_>>());
    let mut urls: Vec<String> = before.iter().map(|s| s.to_string()).collect();
    assert_eq!(add_entry(&mut urls, text), added);
    assert_eq!(
        urls,
        after.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}

/// APP-LCH-02: 曲サーバーの URL
#[rstest]
#[case::empty("  ", Ok(None))]
#[case::trailing_slash(" http://pc:8770/ ", Ok(Some("http://pc:8770")))]
#[case::https_upper("HTTPS://pc", Ok(Some("HTTPS://pc")))]
#[case::no_scheme("pc:8770", Err(BadUrl))]
#[case::no_host("http://", Err(BadUrl))]
#[case::path_only("http:///x", Err(BadUrl))]
fn app_lch_02_server_url(#[case] text: &str, #[case] want: Result<Option<&str>, BadUrl>) {
    assert_eq!(server_url(text), want.map(|o| o.map(str::to_string)));
}

/// APP-LCH-03: 画面と音量を範囲に収める
#[rstest]
#[case::kept((1920, 1080), 50, (1920, 1080), 50)]
#[case::small((10, 10), -5, (320, 240), 0)]
#[case::large((99999, 99999), 300, (7680, 4320), 100)]
fn app_lch_03_clamped(
    #[case] size: (u32, u32),
    #[case] volume: i32,
    #[case] want_size: (u32, u32),
    #[case] want_volume: i32,
) {
    let mut s = Settings::default();
    s.screen.width = size.0;
    s.screen.height = size.1;
    s.screen.mode = ScreenMode::Fullscreen;
    s.volume = volume;
    let c = clamped(s);
    assert_eq!(
        (c.screen.width, c.screen.height, c.screen.mode, c.volume),
        (
            want_size.0,
            want_size.1,
            ScreenMode::Fullscreen,
            want_volume
        )
    );
}
