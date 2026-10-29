//! LR2 の CSV 分割（SKN-CSV-01）

pub const MAX_FIELDS: usize = 30;

/// 1 行を最大 30 項目に分ける。足りない項目は空文字列・0
#[derive(Debug, Clone)]
pub struct Row {
    pub str: Vec<String>,
    pub val: Vec<i32>,
}

impl Row {
    pub fn s(&self, i: usize) -> &str {
        self.str.get(i).map(String::as_str).unwrap_or("")
    }
    pub fn v(&self, i: usize) -> i32 {
        self.val.get(i).copied().unwrap_or(0)
    }
    /// 先頭項目（命令名）を大文字で
    pub fn command(&self) -> String {
        self.s(0).trim().to_ascii_uppercase()
    }
}

pub fn split(line: &str) -> Row {
    let mut str = Vec::new();
    let mut val = Vec::new();
    for raw in line.split(',').take(MAX_FIELDS) {
        let mut f = raw.trim().to_string();
        if f.len() >= 2 && f.starts_with('"') && f.ends_with('"') {
            f = f[1..f.len() - 1].to_string();
        }
        let v = match f.strip_prefix('!') {
            Some(rest) => -atol(rest),
            None => atol(&f),
        };
        str.push(f);
        val.push(v);
    }
    Row { str, val }
}

/// C の `atol` 相当（32bit に丸める）
pub fn atol(s: &str) -> i32 {
    let s = s.trim_start();
    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1i64, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n = digits.parse::<i64>().unwrap_or(0) * sign;
    n.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}
