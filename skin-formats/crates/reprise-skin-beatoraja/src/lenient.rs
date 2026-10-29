//! beatoraja が使う libGDX `JsonReader` 相当のゆるい JSON（BRJ-JSN-01）。
//! コメント（`//` `/* */`）、末尾カンマ、引用符無しのキー・値を受け付ける。
//! 引用符無しの値は数値・`true`/`false`/`null` として読めればその型、それ以外は文字列。
use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

pub fn parse(src: &str) -> Result<Value, ParseError> {
    let mut p = Parser {
        s: src.as_bytes(),
        src,
        i: 0,
    };
    p.ws()?;
    let v = p.value()?;
    p.ws()?;
    if p.i < p.s.len() {
        return Err(p.err("値の後に余分な文字がある"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn err(&self, m: &str) -> ParseError {
        ParseError {
            line: self.s[..self.i.min(self.s.len())]
                .iter()
                .filter(|c| **c == b'\n')
                .count()
                + 1,
            message: m.into(),
        }
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    /// 空白とコメントを読み飛ばす
    fn ws(&mut self) -> Result<(), ParseError> {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => self.i += 1,
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'/') => {
                    while !matches!(self.peek(), None | Some(b'\n')) {
                        self.i += 1;
                    }
                }
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'*') => {
                    let start = self.i;
                    self.i += 2;
                    loop {
                        match self.peek() {
                            None => {
                                self.i = start;
                                return Err(self.err("閉じていないコメント"));
                            }
                            Some(b'*') if self.s.get(self.i + 1) == Some(&b'/') => {
                                self.i += 2;
                                break;
                            }
                            _ => self.i += 1,
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }
    fn value(&mut self) -> Result<Value, ParseError> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::String),
            Some(_) => {
                let t = self.bare();
                if t.is_empty() {
                    return Err(self.err("値が無い"));
                }
                Ok(bare_value(t))
            }
            None => Err(self.err("入力が途中で終わっている")),
        }
    }
    /// 引用符無しの語（区切り文字・改行・コメントの手前まで、前後の空白は除く）
    fn bare(&mut self) -> &str {
        let start = self.i;
        while let Some(c) = self.peek() {
            let comment = c == b'/' && matches!(self.s.get(self.i + 1), Some(b'/') | Some(b'*'));
            if matches!(c, b',' | b':' | b'}' | b']' | b'\n' | b'\r') || comment {
                break;
            }
            self.i += 1;
        }
        self.src[start..self.i].trim()
    }
    fn string(&mut self) -> Result<String, ParseError> {
        self.i += 1; // "
        let mut out = String::new();
        loop {
            let start = self.i;
            while !matches!(self.peek(), None | Some(b'"') | Some(b'\\')) {
                self.i += 1;
            }
            out.push_str(&self.src[start..self.i]);
            match self.peek() {
                None => return Err(self.err("閉じていない文字列")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                _ => {
                    self.i += 1;
                    let c = self.peek().ok_or_else(|| self.err("不正なエスケープ"))?;
                    self.i += 1;
                    match c {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => {
                            let hex = self.src.get(self.i..self.i + 4).unwrap_or("");
                            let ch = u32::from_str_radix(hex, 16)
                                .ok()
                                .and_then(char::from_u32)
                                .ok_or_else(|| self.err("不正な \\u エスケープ"))?;
                            self.i += 4;
                            out.push(ch);
                        }
                        other => out.push(other as char),
                    }
                }
            }
        }
    }
    fn key(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(b'"') => self.string(),
            _ => {
                let k = self.bare().to_string();
                if k.is_empty() {
                    Err(self.err("キーが無い"))
                } else {
                    Ok(k)
                }
            }
        }
    }
    fn object(&mut self) -> Result<Value, ParseError> {
        self.i += 1;
        let mut m = Map::new();
        loop {
            self.ws()?;
            match self.peek() {
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Object(m));
                }
                None => return Err(self.err("閉じていないオブジェクト")),
                _ => {}
            }
            let k = self.key()?;
            self.ws()?;
            if self.peek() != Some(b':') {
                return Err(self.err("`:` が無い"));
            }
            self.i += 1;
            self.ws()?;
            let v = self.value()?;
            m.insert(k, v); // 重複キーは後勝ち
            self.ws()?;
            if self.peek() == Some(b',') {
                self.i += 1;
            }
        }
    }
    fn array(&mut self) -> Result<Value, ParseError> {
        self.i += 1;
        let mut a = vec![];
        loop {
            self.ws()?;
            match self.peek() {
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Array(a));
                }
                None => return Err(self.err("閉じていない配列")),
                _ => {}
            }
            a.push(self.value()?);
            self.ws()?;
            if self.peek() == Some(b',') {
                self.i += 1;
            }
        }
    }
}

fn bare_value(t: &str) -> Value {
    match t {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" => return Value::Null,
        _ => {}
    }
    if let Ok(i) = t.parse::<i64>() {
        return Value::Number(i.into());
    }
    if let Some(n) = t.parse::<f64>().ok().and_then(Number::from_f64) {
        return Value::Number(n);
    }
    Value::String(t.to_string())
}
