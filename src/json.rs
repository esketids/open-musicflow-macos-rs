//! Lightweight, zero-dependency recursive JSON parser and serializer for LG TCP 9741 & HTTP API.

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        if let JsonValue::Object(entries) = self {
            for (k, v) in entries {
                if k == key {
                    return Some(v);
                }
            }
        }
        None
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            JsonValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            JsonValue::Number(n) => Some(*n as i64),
            JsonValue::Str(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            JsonValue::Bool(b) => Some(*b),
            JsonValue::Number(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    pub fn get_arr(&self, key: &str) -> Option<&[JsonValue]> {
        match self.get(key)? {
            JsonValue::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            JsonValue::Null => out.push_str("null"),
            JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JsonValue::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            JsonValue::Str(s) => {
                out.push('"');
                escape_json_str(s, out);
                out.push('"');
            }
            JsonValue::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            JsonValue::Object(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    escape_json_str(k, out);
                    out.push_str("\":");
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }
}

pub fn escape_json_str(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    escape_json_str(s, &mut out);
    out.push('"');
    out
}

pub fn parse_json(input: &str) -> Option<JsonValue> {
    let chars: Vec<char> = input.chars().collect();
    let mut pos = 0;
    let val = parse_value(&chars, &mut pos)?;
    Some(val)
}

fn skip_ws(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
}

fn parse_value(chars: &[char], pos: &mut usize) -> Option<JsonValue> {
    skip_ws(chars, pos);
    if *pos >= chars.len() {
        return None;
    }
    match chars[*pos] {
        '{' => parse_object(chars, pos),
        '[' => parse_array(chars, pos),
        '"' => parse_string(chars, pos).map(JsonValue::Str),
        't' => {
            if chars.get(*pos..*pos + 4)? == ['t', 'r', 'u', 'e'] {
                *pos += 4;
                Some(JsonValue::Bool(true))
            } else {
                None
            }
        }
        'f' => {
            if chars.get(*pos..*pos + 5)? == ['f', 'a', 'l', 's', 'e'] {
                *pos += 5;
                Some(JsonValue::Bool(false))
            } else {
                None
            }
        }
        'n' => {
            if chars.get(*pos..*pos + 4)? == ['n', 'u', 'l', 'l'] {
                *pos += 4;
                Some(JsonValue::Null)
            } else {
                None
            }
        }
        c if c == '-' || c.is_ascii_digit() => parse_number(chars, pos),
        _ => None,
    }
}

fn parse_object(chars: &[char], pos: &mut usize) -> Option<JsonValue> {
    *pos += 1; // '{'
    skip_ws(chars, pos);
    let mut entries = Vec::new();
    if *pos < chars.len() && chars[*pos] == '}' {
        *pos += 1;
        return Some(JsonValue::Object(entries));
    }
    loop {
        skip_ws(chars, pos);
        let key = parse_string(chars, pos)?;
        skip_ws(chars, pos);
        if *pos >= chars.len() || chars[*pos] != ':' {
            return None;
        }
        *pos += 1;
        let val = parse_value(chars, pos)?;
        entries.push((key, val));
        skip_ws(chars, pos);
        if *pos >= chars.len() {
            return None;
        }
        if chars[*pos] == '}' {
            *pos += 1;
            break;
        } else if chars[*pos] == ',' {
            *pos += 1;
        } else {
            return None;
        }
    }
    Some(JsonValue::Object(entries))
}

fn parse_array(chars: &[char], pos: &mut usize) -> Option<JsonValue> {
    *pos += 1; // '['
    skip_ws(chars, pos);
    let mut items = Vec::new();
    if *pos < chars.len() && chars[*pos] == ']' {
        *pos += 1;
        return Some(JsonValue::Array(items));
    }
    loop {
        let val = parse_value(chars, pos)?;
        items.push(val);
        skip_ws(chars, pos);
        if *pos >= chars.len() {
            return None;
        }
        if chars[*pos] == ']' {
            *pos += 1;
            break;
        } else if chars[*pos] == ',' {
            *pos += 1;
        } else {
            return None;
        }
    }
    Some(JsonValue::Array(items))
}

fn parse_string(chars: &[char], pos: &mut usize) -> Option<String> {
    if *pos >= chars.len() || chars[*pos] != '"' {
        return None;
    }
    *pos += 1;
    let mut out = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        *pos += 1;
        if c == '"' {
            return Some(out);
        } else if c == '\\' {
            if *pos >= chars.len() {
                return None;
            }
            let esc = chars[*pos];
            *pos += 1;
            match esc {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    if *pos + 4 <= chars.len() {
                        let hex: String = chars[*pos..*pos + 4].iter().collect();
                        *pos += 4;
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(code) {
                                out.push(ch);
                            }
                        }
                    }
                }
                other => out.push(other),
            }
        } else {
            out.push(c);
        }
    }
    None
}

fn parse_number(chars: &[char], pos: &mut usize) -> Option<JsonValue> {
    let start = *pos;
    if chars[*pos] == '-' {
        *pos += 1;
    }
    while *pos < chars.len()
        && (chars[*pos].is_ascii_digit()
            || chars[*pos] == '.'
            || chars[*pos] == 'e'
            || chars[*pos] == 'E'
            || chars[*pos] == '+'
            || chars[*pos] == '-')
    {
        *pos += 1;
    }
    let s: String = chars[start..*pos].iter().collect();
    let n = s.parse::<f64>().ok()?;
    Some(JsonValue::Number(n))
}
