//! カスタムフォルダ（LR2 の `.lr2folder`）。ファイルの読み方は OpenLR2 の `LR2_songmanage.cpp`（`ParseBMSMETA`・
//! `ParseTextCommand`・`GetFolderDataFromPath`・`SearchSongsFromPath`）・`En_fileutil.cpp`（`IsLR2Folder`・`GetStringBodyStr`・
//! `GetDifficulty`・`GetDifficultyFromToken`・`DealWhiteSpace`）・`strclass.cpp`（`trimWhiteSpace`）、
//! 開き方は `Scene02_Songselect.cpp`（`curQuery[0]`）と `LR2_songmanage.cpp`（`LoadFilteredBmsListFromDB`）に合わせた（APP-CF-*）。
use crate::ports::SongQuery;
use domain::select::{CustomFolder, CustomResult};
use std::path::Path;

/// LR2 の `config.jukebox.titleflash` の既定（時間。`__NEWSONG__` に使う）
pub const TITLE_FLASH_HOURS: i64 = 24;

/// APP-CF-01: `.lr2folder` のファイルか（LR2 `IsLR2Folder`: 名前が 10 バイトより長く、小文字にして `.lr2folder` で終わる）
pub fn is_lr2folder(name: &str) -> bool {
    name.len() > 10 && name.to_ascii_lowercase().ends_with(".lr2folder")
}

/// 大文字小文字を無視して `head` で始まり、`head` より長いか（LR2 `CheckStringHead`）
fn has_head(line: &str, head: &str) -> bool {
    line.len() > head.len() && line.as_bytes()[..head.len()].eq_ignore_ascii_case(head.as_bytes())
}

/// `head` の後ろの 1 文字を飛ばした残り（LR2 `GetStringBodyStr`）
fn body<'a>(line: &'a str, head: &str) -> Option<&'a str> {
    if !has_head(line, head) {
        return None;
    }
    let rest = &line[head.len()..];
    let mut it = rest.chars();
    it.next();
    Some(it.as_str())
}

/// LR2 `GetDifficultyFromToken` の左側（`tokenL` を右から探し、全体が `tokenR` で終われば その前を空白を除いて返す）
fn split_by_token(s: &[u8], l: &[u8], r: &[u8]) -> Option<String> {
    // 末尾の 1 バイトを除いた範囲で、2 バイト以上の長さまで右から探す
    let mut pos = None;
    let mut n = s.len().checked_sub(1)?;
    while n > 1 {
        if s[..n].ends_with(l) {
            pos = Some(n - l.len());
            break;
        }
        n -= 1;
    }
    let pos = pos.filter(|p| *p >= 1)?;
    if !s.ends_with(r) {
        return None;
    }
    Some(
        String::from_utf8_lossy(&s[..pos])
            .trim_matches([' ', '\t'])
            .to_string(),
    )
}

/// `#TITLE` の題名（LR2 `GetDifficulty`: 括弧などで囲んだ末尾は副題として外す）
fn title_of(s: &str) -> String {
    let tokens: [(&str, &str); 7] = [
        ("(", ")"),
        ("[", "]"),
        ("-", "-"),
        ("\"", "\""),
        ("<", ">"),
        ("～", "～"),
        ("【", "】"),
    ];
    tokens
        .iter()
        .find_map(|(l, r)| split_by_token(s.as_bytes(), l.as_bytes(), r.as_bytes()))
        .unwrap_or_else(|| s.to_string())
}

/// LR2 `GetRawBodyInt`（`head` の後ろの 1 文字を飛ばしたところから整数を読む。読めなければ None）
fn raw_int(line: &str, head: &str) -> Option<i32> {
    if !has_head(line, head) || line.len() <= head.len() + 1 {
        return None;
    }
    let rest = line.get(head.len() + 1..)?;
    let end = rest
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && c == '-')))
        .map_or(rest.len(), |(i, _)| i);
    rest[..end].parse().ok()
}

const TEXT_HEADS: [&str; 13] = [
    "#TITLE",
    "#GENRE",
    "#CATEGORY",
    "#SUBTITLE",
    "#ARTIST",
    "#SUBARTIST",
    "#INFORMATION_A",
    "#INFORMATION_B",
    "#TAG",
    "#COMMAND",
    "#STAGEFILE",
    "#BANNER",
    "#BACKBMP",
];

/// `.lr2folder` の中身（Shift_JIS を読んだもの）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub folder: CustomFolder,
    /// `#BANNER`
    pub banner: String,
}

/// APP-CF-02: `.lr2folder` を読む（LR2 `ParseBMSMETA`）。行は `#` で始まるものだけ見て、`#IF`（`#IF 1` を除く）から
/// `#ENDIF` までは読まない。`#TITLE` は題名（括弧などで囲んだ末尾は外す）、`#GENRE`・`#CATEGORY` は `category`、
/// `#ARTIST`・`#INFORMATION_A` は `info_a`、`#SUBARTIST`・`#INFORMATION_B` は `info_b`、`#TAG`・`#COMMAND` は `command`、
/// `#BANNER`、`#PLAYLEVEL`・`#MAXTRACKS` は `max`。名前は大文字小文字を無視し、値は名前の後ろの 1 文字を飛ばした残り
/// （行の前後の空白は除く）。同じものは後に書いたもの。`kind` は 2
pub fn parse(path: &Path, text: &str) -> Parsed {
    let mut p = Parsed::default();
    p.folder.path = path.to_path_buf();
    p.folder.kind = 2;
    let mut skip = false;
    for raw in text.split('\n') {
        if raw.starts_with("#IF") && !raw.starts_with("#IF 1") {
            skip = true;
            continue;
        }
        if raw.starts_with("#ENDIF") {
            skip = false;
            continue;
        }
        if !raw.starts_with('#') || skip {
            continue;
        }
        if TEXT_HEADS.iter().any(|h| {
            raw.len() >= h.len() && raw.as_bytes()[..h.len()].eq_ignore_ascii_case(h.as_bytes())
        }) {
            let line = raw
                .trim_start_matches([' ', '\t'])
                .trim_end_matches([' ', '\t', '\r', '\n']);
            let f = &mut p.folder;
            if let Some(b) = body(line, "#TITLE") {
                f.title = title_of(b);
            } else if let Some(b) = body(line, "#GENRE").or_else(|| body(line, "#CATEGORY")) {
                f.category = b.to_string();
            } else if body(line, "#SUBTITLE").is_some() {
            } else if let Some(b) = body(line, "#ARTIST") {
                f.info_a = b.to_string();
            } else if let Some(b) = body(line, "#SUBARTIST") {
                f.info_b = b.to_string();
            } else if let Some(b) = body(line, "#INFORMATION_A") {
                f.info_a = b.to_string();
            } else if let Some(b) = body(line, "#INFORMATION_B") {
                f.info_b = b.to_string();
            } else if let Some(b) = body(line, "#TAG").or_else(|| body(line, "#COMMAND")) {
                f.command = b.to_string();
            } else if let Some(b) = body(line, "#BANNER") {
                p.banner = b.to_string();
            }
        } else if let Some(n) = raw_int(raw, "#PLAYLEVEL").or_else(|| raw_int(raw, "#MAXTRACKS")) {
            p.folder.max = n;
        }
    }
    p
}

/// APP-CF-03: 登録したフォルダが `.lr2folder` のときの種類（LR2 `GetFolderDataFromPath`: `command` が `__NEWSONG__` なら 3、
/// `__RIVAL__` なら 4、`__EXPERT__`・`__NONSTOP__`・`__GRADE__` なら 6、それ以外は 2）
pub fn root_kind(command: &str) -> i32 {
    match command {
        "__NEWSONG__" => 3,
        "__RIVAL__" => 4,
        "__EXPERT__" | "__NONSTOP__" | "__GRADE__" => 6,
        _ => 2,
    }
}

/// APP-CF-04: 開くときの SQL と、並びが決まっているか（LR2 `curQuery[0]` と `LoadFilteredBmsListFromDB`）。
/// `SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE <command>`。`__NEWSONG__` を含めば
/// `WHERE adddate > <今 − 24 時間>`。上限が 1 で `random()` を含まなければ ` ORDER BY random()` を足して決まっている。
/// `ORDER BY` を含めば決まっている。決まっていなければ ` ORDER BY folder ASC,mode ASC,difficulty ASC,level ASC` を足す。
/// `command` が使えない（DOM-SEL-24）か、コース・ライバル（`__EXPERT__`・`__NONSTOP__`・`__GRADE__`・`__RIVAL__`）なら None
pub fn query(folder: &CustomFolder, now: i64) -> Option<(String, bool)> {
    if !folder.has_command() {
        return None;
    }
    let cmd = &folder.command;
    if ["__EXPERT__", "__NONSTOP__", "__GRADE__", "__RIVAL__"]
        .iter()
        .any(|t| cmd.contains(t))
    {
        return None;
    }
    let head = "SELECT * FROM song LEFT JOIN score ON song.hash = score.hash WHERE";
    let mut q = if cmd.contains("__NEWSONG__") {
        format!("{head} adddate > {}", now - TITLE_FLASH_HOURS * 3600)
    } else {
        format!("{head} {cmd}")
    };
    let mut ordered = false;
    if folder.max == 1 && !q.contains("random()") {
        q.push_str(" ORDER BY random()");
        ordered = true;
    }
    if q.contains("ORDER BY") {
        ordered = true;
    }
    if !ordered {
        q.push_str(" ORDER BY folder ASC,mode ASC,difficulty ASC,level ASC");
    }
    Some((q, ordered))
}

/// APP-CF-05: カスタムフォルダの譜面を引く（SQL を引けなければ None。LR2 も失敗した SQL は 0 行）
pub fn evaluate(db: &dyn SongQuery, folder: &CustomFolder, now: i64) -> Option<CustomResult> {
    let (sql, ordered) = query(folder, now)?;
    match db.song_paths(&sql) {
        Ok(paths) => Some(CustomResult { paths, ordered }),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests;
