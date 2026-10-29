//! 行の字句解析（BMS-LEX-01..05）
//!
//! 数値は C の `atol` / `atof` と同じく「先頭から読めるところまで」を値とする。

/// 1 行の分類
#[derive(Debug, Clone, PartialEq)]
pub enum Line<'a> {
    Ignored,
    Random(i64),
    SetRandom(i64),
    If(i64),
    ElseIf(i64),
    Else,
    EndIf,
    EndRandom,
    /// `#KEY value`（KEY は大文字化済み）
    Header {
        key: String,
        value: &'a str,
    },
    /// `#mmmcc:data`
    Message {
        measure: u32,
        channel: [u8; 2],
        data: String,
    },
}

pub fn classify(raw: &str) -> Line<'_> {
    let line = raw.trim();
    let Some(body) = line.strip_prefix('#') else {
        return Line::Ignored;
    };
    if let Some(m) = message(body) {
        return m;
    }
    let (key, value) = match body.find(|c: char| c.is_whitespace()) {
        Some(i) => (&body[..i], body[i..].trim()),
        None => (body, ""),
    };
    let key = key.to_ascii_uppercase();
    match key.as_str() {
        "RANDOM" => Line::Random(atol(value)),
        "SETRANDOM" => Line::SetRandom(atol(value)),
        "IF" => Line::If(atol(value)),
        "ELSEIF" => Line::ElseIf(atol(value)),
        "ELSE" => Line::Else,
        "ENDIF" => Line::EndIf,
        "ENDRANDOM" => Line::EndRandom,
        _ => Line::Header { key, value },
    }
}

fn message(body: &str) -> Option<Line<'_>> {
    let b = body.as_bytes();
    if b.len() < 6 || b[5] != b':' || !b[..3].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let ch = [b[3].to_ascii_uppercase(), b[4].to_ascii_uppercase()];
    if !ch.iter().all(u8::is_ascii_alphanumeric) {
        return None;
    }
    let measure = body[..3].parse().ok()?;
    let data: String = body[6..].chars().filter(|c| !c.is_whitespace()).collect();
    Some(Line::Message {
        measure,
        channel: ch,
        data,
    })
}

/// 2 文字の ID を数値に（36 進は大小無視、62 進は大小区別）。不正なら `None`
pub fn parse_id(pair: &[u8], base62: bool) -> Option<u32> {
    let d = |c: u8| -> Option<u32> {
        match c {
            b'0'..=b'9' => Some((c - b'0') as u32),
            b'A'..=b'Z' => Some((c - b'A') as u32 + 10),
            b'a'..=b'z' if base62 => Some((c - b'a') as u32 + 36),
            b'a'..=b'z' => Some((c - b'a') as u32 + 10),
            _ => None,
        }
    };
    let base = if base62 { 62 } else { 36 };
    Some(d(pair[0])? * base + d(pair[1])?)
}

/// 2 文字の 16 進
pub fn parse_hex(pair: &[u8]) -> Option<u32> {
    let s = std::str::from_utf8(pair).ok()?;
    u32::from_str_radix(s, 16).ok()
}

/// C の `atol` 相当
pub fn atol(s: &str) -> i64 {
    let s = s.trim_start();
    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    sign * digits.parse::<i64>().unwrap_or(0)
}

/// C の `atof` 相当（指数表記は扱わない）
pub fn atof(s: &str) -> f64 {
    let s = s.trim_start();
    let mut end = 0;
    let b = s.as_bytes();
    if matches!(b.first(), Some(b'-' | b'+')) {
        end = 1;
    }
    let mut seen_dot = false;
    while end < b.len() {
        match b[end] {
            b'0'..=b'9' => {}
            b'.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        end += 1;
    }
    s[..end].parse().unwrap_or(0.0)
}
