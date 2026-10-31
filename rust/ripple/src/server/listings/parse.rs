use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::server::db::schema::Market;
use crate::server::text::js_trim;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingRow {
    pub market: Market,
    pub code: String,
    pub name: String,
    pub exchange: String,
    pub is_etf: bool,
}

/// 목록 형식이 바뀌어 읽지 못할 때(메시지는 화면·로그에 그대로 나간다)
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ListingParseError(pub String);

/// KIND 시장구분 → 거래소
fn kr_exchange(market: &str) -> &str {
    match market {
        "유가" => "KOSPI",
        "코스닥" => "KOSDAQ",
        "코넥스" => "KONEX",
        other => other,
    }
}

/// otherlisted.txt의 Exchange 값 (NASDAQ Trader 정의 페이지 기준).
/// NYSE MKT는 지금의 NYSE American, BATS는 지금의 Cboe BZX다.
fn us_exchange(code: &str) -> Option<&'static str> {
    Some(match code {
        "A" => "NYSE American",
        "N" => "NYSE",
        "P" => "NYSE Arca",
        "Z" => "Cboe BZX",
        "V" => "IEX",
        _ => return None,
    })
}

/// 한국 종목코드: 6자리. 2024년부터 영문이 섞인 코드도 있다(예: 0158S0).
pub static KR_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9A-Z]{6}$").expect("정규식이 올바르다"));

static ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<tr[^>]*>([\s\S]*?)</tr>").expect("정규식이 올바르다"));
static CELL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<t[dh][^>]*>([\s\S]*?)</t[dh]>").expect("정규식이 올바르다"));
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("정규식이 올바르다"));
static NUMERIC_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#(\d+);").expect("정규식이 올바르다"));
static HEX_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&#x([0-9a-f]+);").expect("정규식이 올바르다"));
static SPACES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\u{FEFF}]+").expect("정규식이 올바르다"));
static LINE_BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\r?\n").expect("정규식이 올바르다"));

/// KIND 상장법인목록. 확장자는 .xls지만 실제로는 HTML 표다(EUC-KR은 받는 쪽에서 풂).
pub fn parse_kind_list(html: &str) -> Result<Vec<ListingRow>, ListingParseError> {
    let rows: Vec<Vec<String>> = ROW
        .captures_iter(html)
        .map(|row| {
            CELL.captures_iter(&row[1])
                .map(|cell| {
                    let text = TAG.replace_all(&cell[1], "");
                    js_trim(&decode_html(&text)).to_string()
                })
                .collect()
        })
        .collect();
    let mut rows = rows.into_iter();
    let header = rows.next().unwrap_or_default();
    let at = |name: &str| header.iter().position(|cell| cell == name);
    let (Some(name_at), Some(market_at), Some(code_at)) =
        (at("회사명"), at("시장구분"), at("종목코드"))
    else {
        return Err(ListingParseError(
            "KIND 상장법인목록의 형식이 바뀌었습니다(열 이름).".to_string(),
        ));
    };
    Ok(rows
        .filter_map(|cells| {
            let cell = |i: usize| cells.get(i).map_or("", String::as_str);
            let code = cell(code_at).to_uppercase();
            let name = cell(name_at);
            if !KR_CODE.is_match(&code) || name.is_empty() {
                return None;
            }
            Some(ListingRow {
                market: Market::Kr,
                code,
                name: name.to_string(),
                exchange: kr_exchange(cell(market_at)).to_string(),
                is_etf: false,
            })
        })
        .collect())
}

/// NASDAQ Trader nasdaqlisted.txt (NASDAQ 상장)
pub fn parse_nasdaq_listed(text: &str) -> Result<Vec<ListingRow>, ListingParseError> {
    parse_pipe_file(text, "Symbol", |_| "NASDAQ".to_string())
}

/// NASDAQ Trader otherlisted.txt (NYSE 등 다른 거래소 상장)
pub fn parse_other_listed(text: &str) -> Result<Vec<ListingRow>, ListingParseError> {
    parse_pipe_file(text, "ACT Symbol", |row| {
        let exchange = row.get("Exchange").map_or("", String::as_str);
        us_exchange(exchange).unwrap_or(exchange).to_string()
    })
}

fn parse_pipe_file(
    text: &str,
    symbol: &str,
    exchange_of: impl Fn(&HashMap<String, String>) -> String,
) -> Result<Vec<ListingRow>, ListingParseError> {
    let mut lines = LINE_BREAK
        .split(text)
        .filter(|line| !js_trim(line).is_empty());
    let header: Vec<&str> = lines.next().unwrap_or("").split('|').collect();
    for column in [symbol, "Security Name", "ETF", "Test Issue"] {
        if !header.contains(&column) {
            return Err(ListingParseError(format!(
                "NASDAQ 종목 목록의 형식이 바뀌었습니다({column} 열 없음)."
            )));
        }
    }
    Ok(lines
        .filter_map(|line| {
            // 마지막 줄은 "File Creation Time: ..." 이다.
            if line.starts_with("File Creation Time") {
                return None;
            }
            let values: Vec<&str> = line.split('|').collect();
            // 같은 이름의 열이 겹치면 뒤의 값이 이긴다(JS Object.fromEntries).
            let row: HashMap<String, String> = header
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    (
                        (*key).to_string(),
                        values.get(i).copied().unwrap_or("").to_string(),
                    )
                })
                .collect();
            let code = js_trim(&row[symbol]).to_uppercase();
            let name = js_trim(&SPACES.replace_all(&row["Security Name"], " ")).to_string();
            if code.is_empty() || name.is_empty() || row["Test Issue"] == "Y" {
                return None;
            }
            Some(ListingRow {
                market: Market::Us,
                code,
                name,
                exchange: exchange_of(&row),
                is_etf: row["ETF"] == "Y",
            })
        })
        .collect())
}

/// JS `String.fromCodePoint`는 올바르지 않은 값이면 오류를 내지만 여기서는 원문을 그대로 둔다
/// (서로게이트·범위 밖 값은 Rust 문자로 만들 수 없다).
fn decode_html(text: &str) -> String {
    let numeric = NUMERIC_ENTITY.replace_all(text, |caps: &regex::Captures| {
        caps[1]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| caps[0].to_string(), |c| c.to_string())
    });
    let hex = HEX_ENTITY.replace_all(&numeric, |caps: &regex::Captures| {
        u32::from_str_radix(&caps[1], 16)
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| caps[0].to_string(), |c| c.to_string())
    });
    hex.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
