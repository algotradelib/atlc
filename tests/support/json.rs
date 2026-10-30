//! A strict JSON reader (RFC 8259) for the tests: the whole text must be one value, strings are decoded with their
//! escapes and surrogate pairs, and anything else (a trailing comma, a control character in a string, trailing text)
//! is an error.

/// A JSON value; an object keeps its members in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// A member of an object; panics if there is none.
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(members) => members
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("no `{key}` in {self:?}")),
            _ => panic!("not an object: {self:?}"),
        }
    }

    /// The member names of an object, in order.
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Json::Obj(members) => members.iter().map(|(k, _)| k.as_str()).collect(),
            _ => panic!("not an object: {self:?}"),
        }
    }

    /// A string's text; panics for anything else.
    pub fn str(&self) -> &str {
        match self {
            Json::Str(s) => s,
            _ => panic!("not a string: {self:?}"),
        }
    }

    /// An array's items; panics for anything else.
    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            _ => panic!("not an array: {self:?}"),
        }
    }
}

/// Reads `text` as one JSON value.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut r = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let v = r.value(0)?;
    r.space();
    if r.at != r.bytes.len() {
        return Err(format!("text after the value at byte {}", r.at));
    }
    Ok(v)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while self.at < self.bytes.len() && matches!(self.bytes[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", b as char, self.at))
        }
    }

    fn word(&mut self, w: &str, v: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(w.as_bytes()) {
            self.at += w.len();
            Ok(v)
        } else {
            Err(format!("unexpected text at byte {}", self.at))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > 64 {
            return Err("nested too deeply".into());
        }
        self.space();
        match self.peek() {
            Some(b'n') => self.word("null", Json::Null),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => {
                self.at += 1;
                let mut items = vec![];
                self.space();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Json::Arr(items));
                        }
                        _ => return Err(format!("expected `,` or `]` at byte {}", self.at)),
                    }
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut members: Vec<(String, Json)> = vec![];
                self.space();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(Json::Obj(members));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    if members.iter().any(|(k, _)| *k == key) {
                        return Err(format!("`{key}` twice"));
                    }
                    self.space();
                    self.expect(b':')?;
                    let v = self.value(depth + 1)?;
                    members.push((key, v));
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Json::Obj(members));
                        }
                        _ => return Err(format!("expected `,` or `}}` at byte {}", self.at)),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("expected a value at byte {}", self.at)),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        self.at - start
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let int_start = self.at;
        let n = self.digits();
        if n == 0 || (n > 1 && self.bytes[int_start] == b'0') {
            return Err(format!("a malformed number at byte {start}"));
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if self.digits() == 0 {
                return Err(format!("a malformed number at byte {start}"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(format!("a malformed number at byte {start}"));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap();
        text.parse().map(Json::Num).map_err(|e| format!("{text}: {e}"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let hex = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|h| std::str::from_utf8(h).ok())
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| format!("a malformed \\u escape at byte {}", self.at))?;
        self.at += 4;
        Ok(u32::from_str_radix(hex, 16).unwrap())
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = vec![];
        loop {
            let Some(b) = self.peek() else {
                return Err("a string never ends".into());
            };
            self.at += 1;
            match b {
                b'"' => return String::from_utf8(out).map_err(|e| e.to_string()),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err("a string never ends".into());
                    };
                    self.at += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&high) {
                                if !self.bytes[self.at..].starts_with(b"\\u") {
                                    return Err("a lone surrogate".into());
                                }
                                self.at += 2;
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err("a lone surrogate".into());
                                }
                                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                high
                            };
                            char::from_u32(code).ok_or("a lone surrogate")?
                        }
                        _ => return Err(format!("an unknown escape at byte {}", self.at)),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                b if b < 0x20 => return Err(format!("a control character in a string at byte {}", self.at)),
                b => out.push(b),
            }
        }
    }
}
