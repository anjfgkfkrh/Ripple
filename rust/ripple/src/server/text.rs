//! JS와 문자열·숫자 동작이 다른 곳을 맞추는 도구(docs/RUST_MIGRATION.md 8장).

/// JS `s.length`(UTF-16 단위 길이)
pub fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// JS `s.slice(start, end)`(UTF-16 단위, 음수 인자 없음). 서로게이트 쌍 가운데를 자르게 되면
/// 그 앞에서 자른다(TS는 깨진 글자를 남기지만 Rust 문자열에는 넣을 수 없다).
pub fn js_slice(s: &str, start: usize, end: usize) -> String {
    let mut out = String::new();
    let mut pos = 0;
    for c in s.chars() {
        let next = pos + c.len_utf16();
        if next > end {
            break;
        }
        if pos >= start {
            out.push(c);
        }
        pos = next;
    }
    out
}

/// JS `String.prototype.trim()`. JS는 BOM(U+FEFF)도 공백으로 보고 U+0085는 공백으로 보지 않는다.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{FEFF}')
}

/// JS `String(x)`(= `JSON.stringify(x)`의 숫자 부분). 정수 값은 소수점 없이, 아주 크거나 작으면 지수로 쓴다.
/// 유한하지 않은 값은 `String(x)`처럼 NaN·Infinity·-Infinity로 쓴다(JSON에는 쓰지 않는 값).
pub fn js_number(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    let abs = x.abs();
    if (1e-6..1e21).contains(&abs) {
        // Rust `{}`는 가장 짧은 표현이고 소수점 표기만 쓴다. 정수 값은 `5`로 나온다.
        return format!("{x}");
    }
    // 지수 표기: Rust `{:e}`는 "1e21", "1.5e-7" 꼴. JS는 양수 지수에 +를 붙인다.
    let text = format!("{x:e}");
    match text.split_once('e') {
        Some((mantissa, exp)) if !exp.starts_with('-') => format!("{mantissa}e+{exp}"),
        _ => text,
    }
}

/// JS `encodeURIComponent`: 영문·숫자와 `-_.!~*'()`만 그대로 두고 나머지는 UTF-8 바이트를 `%XX`(대문자)로 쓴다.
pub fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// JS `x.toFixed(digits)`. 이진수 값을 정확한 십진 값으로 보고 0에서 먼 쪽으로 반올림한다
/// (Rust `{:.n}`은 정확히 가운데인 값을 짝수 쪽으로 반올림해 `0.03125`가 달라진다). 유한한 값만 받는다.
pub fn js_to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return js_number(x);
    }
    if x.abs() >= 1e21 {
        return js_number(x);
    }
    // 정확한 십진 전개: 가운데 값 판단에 digits + 60자리면 충분하다(f64 유효 비트 53개).
    let exact = format!("{:.*}", digits + 60, x.abs());
    let (int_part, frac_part) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut kept: Vec<u8> = int_part
        .bytes()
        .chain(frac_part.bytes().take(digits))
        .collect();
    // JS toFixed: 정확히 가운데이거나 그 이상이면 올린다(n/10^f 중 더 큰 n).
    if frac_part.as_bytes().get(digits).is_some_and(|d| *d >= b'5') {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let int_len = kept.len() - digits;
    let int_text = String::from_utf8_lossy(&kept[..int_len]).into_owned();
    let frac_text = String::from_utf8_lossy(&kept[int_len..]).into_owned();
    let body = if digits == 0 {
        int_text
    } else {
        format!("{int_text}.{frac_text}")
    };
    // JS는 0으로 반올림돼도 음수 부호를 남긴다("-0.00"). -0은 음수가 아니다.
    if x < 0.0 { format!("-{body}") } else { body }
}

/// JS `Number(s)`(StringToNumber). 앞뒤 공백을 뺀 뒤 빈 글자는 0, 읽을 수 없으면 NaN.
pub fn js_parse_number(s: &str) -> f64 {
    let s = js_trim(s);
    if s.is_empty() {
        return 0.0;
    }
    let radix_digits = |prefixes: [&str; 2], radix: u32| -> Option<f64> {
        let rest = prefixes.iter().find_map(|p| s.strip_prefix(p))?;
        if rest.is_empty() {
            return Some(f64::NAN);
        }
        let mut value = 0.0_f64;
        for c in rest.chars() {
            match c.to_digit(radix) {
                Some(d) => value = value * f64::from(radix) + f64::from(d),
                None => return Some(f64::NAN),
            }
        }
        Some(value)
    };
    if let Some(v) = radix_digits(["0x", "0X"], 16)
        .or_else(|| radix_digits(["0o", "0O"], 8))
        .or_else(|| radix_digits(["0b", "0B"], 2))
    {
        return v;
    }
    let (sign, body) = match s.as_bytes()[0] {
        b'+' => (1.0, &s[1..]),
        b'-' => (-1.0, &s[1..]),
        _ => (1.0, s),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    if !is_decimal_literal(body) {
        return f64::NAN;
    }
    body.parse::<f64>().map_or(f64::NAN, |v| sign * v)
}

/// 숫자 `[.숫자] [e[+-]숫자]` 꼴만 받는다(Rust 파서가 받는 inf·nan·밑줄 등을 막는다).
fn is_decimal_literal(body: &str) -> bool {
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = match mantissa.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (mantissa, None),
    };
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() && frac.is_none_or(str::is_empty) {
        return false;
    }
    if !digits(int) || !frac.is_none_or(digits) {
        return false;
    }
    match exponent {
        None => true,
        Some(e) => {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            !e.is_empty() && digits(e)
        }
    }
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
