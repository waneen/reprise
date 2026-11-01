//! ランチャー（設定画面）の決まり（APP-LCH-*）。LR2 は本体の前にランチャー（`LR2.exe`）で設定を選び、
//! 「開始」で本体を起動する。画面は `reprise` の `launcher`、ここは画面によらない決まりだけ。
use crate::settings::{Screen, Settings};

/// ウィンドウの幅・高さの範囲（ピクセル）
const WIDTH: (u32, u32) = (320, 7680);
const HEIGHT: (u32, u32) = (240, 4320);

/// APP-LCH-01: 一覧（譜面のフォルダ・難易度表の URL）に足す。前後の空白を除き、空か同じものがあれば足さない。足したら true
pub fn add_entry<T: PartialEq + From<String>>(list: &mut Vec<T>, text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let item = T::from(t.to_string());
    if list.contains(&item) {
        return false;
    }
    list.push(item);
    true
}

/// 曲サーバーの URL が誤り
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadUrl;

/// APP-LCH-02: 曲サーバーの URL。前後の空白と末尾の `/` を除き、空ならつながない（None）。
/// `http://` か `https://`（大文字小文字は問わない）の後ろが空でなければよく、それ以外は誤り
pub fn server_url(text: &str) -> Result<Option<String>, BadUrl> {
    let t = text.trim().trim_end_matches('/');
    if t.is_empty() {
        return Ok(None);
    }
    let lower = t.to_ascii_lowercase();
    let rest = ["http://", "https://"]
        .iter()
        .find_map(|p| lower.strip_prefix(p))
        .ok_or(BadUrl)?;
    if rest.is_empty() || rest.starts_with('/') {
        return Err(BadUrl);
    }
    Ok(Some(t.to_string()))
}

/// APP-LCH-03: 画面と音量を範囲に収める（幅 320..=7680、高さ 240..=4320、音量 0..=100）
pub fn clamped(mut s: Settings) -> Settings {
    s.screen = Screen {
        width: s.screen.width.clamp(WIDTH.0, WIDTH.1),
        height: s.screen.height.clamp(HEIGHT.0, HEIGHT.1),
        ..s.screen
    };
    s.volume = s.volume.clamp(0, 100);
    s
}

#[cfg(test)]
mod tests;
