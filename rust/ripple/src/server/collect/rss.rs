//! RSS 2.0·Atom 피드를 읽는다. 라이브러리 없이 필요한 태그만 뽑는다
//! (2026-10-20 수집 대상 피드 56개를 모두 읽는 것을 확인).
use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::{DateTime, Datelike, NaiveDate};
use regex::{Captures, Regex};

use crate::server::text::js_trim;
use crate::server::time::{DAY_MS, EpochMs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    /// HTML을 걷어 낸 요약(자르기는 저장할 때 한다)
    pub summary: String,
    /// 피드에 없거나 읽을 수 없으면 None
    pub published_at: Option<EpochMs>,
}

/// JS `\s`(BOM 포함, U+0085 제외)
const SP: &str = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";

fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        "middot" => "·",
        "hellip" => "…",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "ndash" => "–",
        "mdash" => "—",
        "bull" => "•",
        "copy" => "©",
        _ => return None,
    })
}

/// 태그 이름마다 한 번만 만드는 정규식(`<name\b[^>]*>(...)</name>`, 대소문자 무시)
static TAGS: LazyLock<HashMap<&'static str, Regex>> = LazyLock::new(|| {
    [
        "item",
        "entry",
        "title",
        "link",
        "description",
        "summary",
        "content",
        "pubDate",
        "dc:date",
        "published",
        "updated",
        "guid",
    ]
    .into_iter()
    .map(|name| {
        let source = format!(r"(?i)<{name}(?-u:\b)[^>]*>([\s\S]*?)</{name}>");
        (name, Regex::new(&source).expect("정규식이 올바르다"))
    })
    .collect()
});

static ATOM_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<link(?-u:\b)([^>]*)/?>").expect("정규식이 올바르다"));
static REL_ALTERNATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)rel=["']alternate["']"#).expect("정규식이 올바르다"));
static REL_ANY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)rel=").expect("정규식이 올바르다"));
static HREF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)href=["']([^"']+)["']"#).expect("정규식이 올바르다"));
static HTTP_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^https?://").expect("정규식이 올바르다"));
static CDATA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!\[CDATA\[([\s\S]*?)\]\]>").expect("정규식이 올바르다"));
static ANY_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]*>").expect("정규식이 올바르다"));
static WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("{SP}+")).expect("정규식이 올바르다"));
static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&(#x[0-9a-f]+|#[0-9]+|[a-z]+);").expect("정규식이 올바르다"));

pub fn parse_feed(xml: &str, now: EpochMs) -> Vec<FeedItem> {
    let items = blocks(xml, "item");
    let atom = items.is_empty();
    let entries = if atom { blocks(xml, "entry") } else { items };
    let mut out = Vec::new();
    for block in entries {
        let title = to_text(tag(block, "title"));
        let link = if atom {
            atom_link(block)
        } else {
            let link = to_text(tag(block, "link"));
            if link.is_empty() {
                guid_link(block)
            } else {
                link
            }
        };
        if title.is_empty() || link.is_empty() {
            continue;
        }
        let summary = to_text(
            tag(block, "description")
                .or_else(|| tag(block, "summary"))
                .or_else(|| tag(block, "content")),
        );
        let published = tag(block, "pubDate")
            .or_else(|| tag(block, "dc:date"))
            .or_else(|| tag(block, "published"))
            .or_else(|| tag(block, "updated"));
        out.push(FeedItem {
            title,
            link,
            summary,
            published_at: parse_feed_date(published, now),
        });
    }
    out
}

fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    TAGS[name]
        .captures_iter(xml)
        .filter_map(|m| m.get(1))
        .map(|m| m.as_str())
        .collect()
}

fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    TAGS[name]
        .captures(block)
        .and_then(|m| m.get(1))
        .map(|m| m.as_str())
}

fn atom_link(block: &str) -> String {
    let links: Vec<&str> = ATOM_LINK
        .captures_iter(block)
        .filter_map(|m| m.get(1))
        .map(|m| m.as_str())
        .collect();
    let pick = links
        .iter()
        .find(|attrs| REL_ALTERNATE.is_match(attrs))
        .or_else(|| links.iter().find(|attrs| !REL_ANY.is_match(attrs)))
        .or_else(|| links.first());
    pick.map_or(String::new(), |attrs| {
        js_trim(&decode_entities(
            HREF.captures(attrs)
                .map_or("", |m| m.get(1).map_or("", |g| g.as_str())),
        ))
        .to_string()
    })
}

fn guid_link(block: &str) -> String {
    let guid = to_text(tag(block, "guid"));
    if HTTP_URL.is_match(&guid) {
        guid
    } else {
        String::new()
    }
}

/// CDATA를 풀고, 엔티티를 풀고, HTML 태그를 걷어 낸 글자
pub fn to_text(raw: Option<&str>) -> String {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return String::new();
    };
    let text = CDATA.replace_all(raw, |caps: &Captures| caps[1].to_string());
    let text = decode_entities(&text);
    let text = ANY_TAG.replace_all(&text, " ");
    let text = decode_entities(&text);
    js_trim(&WHITESPACE.replace_all(&text, " ")).to_string()
}

pub fn decode_entities(text: &str) -> String {
    ENTITY
        .replace_all(text, |caps: &Captures| {
            let whole = &caps[0];
            let body = &caps[1];
            if let Some(rest) = body.strip_prefix('#') {
                let code = match rest.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => rest.parse::<u32>().ok(),
                };
                // 0보다 크고 0x10FFFF 이하. 서로게이트는 Rust 문자로 만들 수 없어 그대로 둔다.
                return code
                    .filter(|c| *c > 0 && *c <= 0x10FFFF)
                    .and_then(char::from_u32)
                    .map_or_else(|| whole.to_string(), |c| c.to_string());
            }
            named_entity(&body.to_lowercase()).map_or_else(|| whole.to_string(), str::to_string)
        })
        .into_owned()
}

// ---- 피드 날짜 ----

const SEOUL_OFFSET_MINUTES: i64 = 9 * 60;

static NUMERIC_MONTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^[A-Za-z]{{3}},{SP}*([0-9]{{1,2}}){SP}+([0-9]{{1,2}}){SP}+([0-9]{{4}}){SP}+([0-9]{{2}}):([0-9]{{2}})(?::([0-9]{{2}}))?{SP}*([+-][0-9]{{4}})?$"
    ))
    .expect("정규식이 올바르다")
});
static NAIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{4})[-./]([0-9]{2})[-./]([0-9]{2})[ T]([0-9]{2}):([0-9]{2})(?::([0-9]{2}))?(?:\.[0-9]+)?$")
        .expect("정규식이 올바르다")
});
static ISO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^([0-9]{{4}})(?:-([0-9]{{2}})(?:-([0-9]{{2}}))?)?(?:([T ])([0-9]{{2}}):([0-9]{{2}})(?::([0-9]{{2}})(?:\.([0-9]+))?)?(?:(Z)|({SP}?)([+-])([0-9]{{2}}):?([0-9]{{2}}))?)?$"
    ))
    .expect("정규식이 올바르다")
});
static RFC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^(?:[a-z]{{3,}}\.?,?{SP}*)?([0-9]{{1,2}}){SP}+([a-z]{{3,}})\.?{SP}+([0-9]{{2}}|[0-9]{{4}}){SP}+([0-9]{{1,2}}):([0-9]{{2}})(?::([0-9]{{2}}))?{SP}*(Z|UT|UTC|GMT|EST|EDT|CST|CDT|MST|MDT|PST|PDT|[+-][0-9]{{2}}:?[0-9]{{2}})?{SP}*(?:\([^)]*\))?$"
    ))
    .expect("정규식이 올바르다")
});
static US_STYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^([a-z]{{3,}})\.?{SP}+([0-9]{{1,2}}),?{SP}+([0-9]{{4}}){SP}+([0-9]{{1,2}}):([0-9]{{2}})(?::([0-9]{{2}}))?{SP}*(Z|UT|UTC|GMT|EST|EDT|CST|CDT|MST|MDT|PST|PDT|[+-][0-9]{{2}}:?[0-9]{{2}})?$"
    ))
    .expect("정규식이 올바르다")
});

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month_number(word: &str) -> Option<i64> {
    let word = word.to_lowercase();
    MONTHS
        .iter()
        .position(|m| m.starts_with(&word))
        .map(|i| i as i64 + 1)
}

fn zone_minutes(zone: &str) -> Option<i64> {
    let upper = zone.to_uppercase();
    Some(match upper.as_str() {
        "Z" | "UT" | "UTC" | "GMT" => 0,
        "EST" => -5 * 60,
        "EDT" => -4 * 60,
        "CST" => -6 * 60,
        "CDT" => -5 * 60,
        "MST" => -7 * 60,
        "MDT" => -6 * 60,
        "PST" => -8 * 60,
        "PDT" => -7 * 60,
        _ => {
            let sign = if upper.starts_with('-') { -1 } else { 1 };
            let digits: String = upper[1..].chars().filter(char::is_ascii_digit).collect();
            let (hours, minutes) = (
                digits[..2].parse::<i64>().ok()?,
                digits[2..].parse::<i64>().ok()?,
            );
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 60 + minutes)
        }
    })
}

/// V8의 날짜 조립 규칙: 월 1~12, 일 1~31(그 달보다 길면 다음 달로 넘어감), 시 0~23(24는 24:00:00만),
/// 분·초 0~59. 시간대는 분 단위 오프셋.
// `Date.UTC`처럼 연·월·일·시·분·초·밀리초·오프셋을 따로 받는다.
#[allow(clippy::too_many_arguments)]
fn make_ms(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    millis: i64,
    offset_minutes: i64,
) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || minute > 59 || second > 59 {
        return None;
    }
    if hour > 24 || (hour == 24 && (minute != 0 || second != 0 || millis != 0)) {
        return None;
    }
    let first = NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, u32::try_from(month).ok()?, 1)?;
    let days = first.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis() / DAY_MS + (day - 1);
    Some(
        days * DAY_MS + hour * 3_600_000 + minute * 60_000 + second * 1000 + millis
            - offset_minutes * 60_000,
    )
}

fn num(caps: &Captures, index: usize) -> i64 {
    caps.get(index)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0)
}

/// 표준 형식(결정 14단계 R5): ISO 8601, RFC 2822(요일 선택, 영문 월), "Oct 20, 2026 09:00 GMT".
/// 시간대가 없으면 한국 시간(compose의 TZ=Asia/Seoul에서 `new Date`가 지역 시간으로 읽는 것과 같다).
fn parse_standard_date(text: &str) -> Option<i64> {
    if let Some(c) = ISO.captures(text) {
        let year = num(&c, 1);
        let month = c.get(2).map_or(1, |_| num(&c, 2));
        let day = c.get(3).map_or(1, |_| num(&c, 3));
        if c.get(5).is_none() {
            // 날짜만: UTC
            return make_ms(year, month, day, 0, 0, 0, 0, 0);
        }
        // 시간이 있으면 일까지 있어야 한다.
        c.get(3)?;
        let separator = c.get(4).map_or("", |m| m.as_str());
        let (hour, minute) = (num(&c, 5), num(&c, 6));
        let second = c.get(7).map_or(0, |_| num(&c, 7));
        let millis: i64 = c.get(8).map_or(0, |m| {
            format!("{:0<3}", &m.as_str()[..m.as_str().len().min(3)])
                .parse()
                .unwrap_or(0)
        });
        let offset = if c.get(9).is_some() {
            0
        } else if let (Some(sign), Some(hh), Some(mm)) = (c.get(11), c.get(12), c.get(13)) {
            // 'T' 뒤에서는 시간대 앞에 공백을 둘 수 없다(V8).
            if separator == "T" && c.get(10).is_some_and(|m| !m.as_str().is_empty()) {
                return None;
            }
            zone_minutes(&format!("{}{}{}", sign.as_str(), hh.as_str(), mm.as_str()))?
        } else {
            SEOUL_OFFSET_MINUTES
        };
        return make_ms(year, month, day, hour, minute, second, millis, offset);
    }
    if let Some(c) = RFC.captures(text) {
        let month = month_number(&c[2])?;
        let mut year = num(&c, 3);
        if c[3].len() == 2 {
            year += if year < 50 { 2000 } else { 1900 };
        }
        let offset = match c.get(7) {
            Some(zone) => zone_minutes(zone.as_str())?,
            None => SEOUL_OFFSET_MINUTES,
        };
        return make_ms(
            year,
            month,
            num(&c, 1),
            num(&c, 4),
            num(&c, 5),
            c.get(6).map_or(0, |_| num(&c, 6)),
            0,
            offset,
        );
    }
    if let Some(c) = US_STYLE.captures(text) {
        let month = month_number(&c[1])?;
        let offset = match c.get(7) {
            Some(zone) => zone_minutes(zone.as_str())?,
            None => SEOUL_OFFSET_MINUTES,
        };
        return make_ms(
            num(&c, 3),
            month,
            num(&c, 2),
            num(&c, 4),
            num(&c, 5),
            c.get(6).map_or(0, |_| num(&c, 6)),
            0,
            offset,
        );
    }
    None
}

/// 글자에 적힌 4자리 연도(앞뒤가 숫자가 아닌 첫 4자리 숫자). JS `/(?<!\d)(\d{4})(?!\d)/`
fn written_year(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 4 {
                return text[start..i].parse().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

/// 피드 날짜를 읽는다. 표준(RFC 822·ISO 8601) 외에
/// 월이 숫자인 형식("Tue, 20 10 2026 17:36:24 +0900", 노컷뉴스)과
/// 시간대 없는 형식("2026-10-20 17:36", 한국 시간으로 본다)도 받는다.
/// 2000년 이전이나 하루 넘게 미래인 값은 버린다.
pub fn parse_feed_date(raw: Option<&str>, now: EpochMs) -> Option<EpochMs> {
    let text = to_text(raw);
    if text.is_empty() {
        return None;
    }
    // V8은 "0001"년을 2001년으로 읽는다. 글자에 적힌 연도가 2000년 전이면 바로 버린다.
    if written_year(&text).is_some_and(|year| year < 2000) {
        return None;
    }
    let date = if let Some(c) = NUMERIC_MONTH.captures(&text) {
        let zone = c.get(7).map_or("+0900", |m| m.as_str());
        make_ms(
            num(&c, 3),
            num(&c, 2),
            num(&c, 1),
            num(&c, 4),
            num(&c, 5),
            c.get(6).map_or(0, |_| num(&c, 6)),
            0,
            zone_minutes(zone)?,
        )
    } else if let Some(c) = NAIVE.captures(&text) {
        make_ms(
            num(&c, 1),
            num(&c, 2),
            num(&c, 3),
            num(&c, 4),
            num(&c, 5),
            c.get(6).map_or(0, |_| num(&c, 6)),
            0,
            SEOUL_OFFSET_MINUTES,
        )
    } else {
        parse_standard_date(&text)
    }?;
    let year = DateTime::from_timestamp_millis(date)?.year();
    if year < 2000 || date > now + DAY_MS {
        return None;
    }
    Some(date)
}

#[cfg(test)]
#[path = "rss_tests.rs"]
mod tests;
