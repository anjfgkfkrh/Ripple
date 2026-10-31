use std::sync::LazyLock;

use regex::Regex;

use crate::server::db::schema::{ItemKind, Market, MatchKind};

/// 기사와 맞춰 볼 항목: 종목 이름과 키워드 트리. 시장·주제 항목은 이름으로 걸지 않는다(11단계).
#[derive(Debug, Clone)]
pub struct MatchStock {
    pub id: i64,
    pub kind: ItemKind,
    pub market: Market,
    pub code: String,
    pub name: String,
    pub name_ko: Option<String>,
    pub keywords: Vec<MatchKeyword>,
}

#[derive(Debug, Clone)]
pub struct MatchKeyword {
    pub level: i64,
    pub text_ko: String,
    pub text_en: Option<String>,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockMatch {
    pub stock_id: i64,
    /// D-16: 당시 경로를 글자로 남긴다. 종목 자체로 걸리면 종목 이름
    pub keyword_path: String,
    /// 키워드 연결일 때 그 키워드의 이유(5단계 Jev 입력)
    pub keyword_reason: Option<String>,
    pub match_kind: MatchKind,
    /// 0이면 종목 자체, 1~4는 키워드 단계
    pub level: i64,
}

struct Term {
    words: Vec<String>,
    level: i64,
    path: String,
    reason: Option<String>,
    kind: MatchKind,
}

struct Prepared {
    id: i64,
    terms: Vec<Term>,
}

/// 제목+요약에 종목 이름이나 키워드의 낱말이 모두 들어 있으면 그 종목에 건다(4단계 결정).
/// 종목마다 가장 가까운 경로 하나만 고른다: 종목 이름 > 1단계 > 2단계 …(같은 단계는 먼저 만든 키워드).
/// 영문·숫자 낱말은 낱말 경계를 지켜 찾는다("AI"가 "said"에 걸리지 않게).
pub struct Matcher {
    prepared: Vec<Prepared>,
}

pub fn create_matcher(stocks: &[MatchStock]) -> Matcher {
    let prepared = stocks
        .iter()
        .map(|stock| {
            let display = stock.name_ko.as_deref().unwrap_or(&stock.name);
            // 시장(한국 증시)·주제는 이름이 아니라 키워드로만 건다(11단계 결정).
            let mut terms: Vec<Term> = Vec::new();
            if stock.kind == ItemKind::Stock {
                terms.push(Term {
                    words: to_words(display),
                    level: 0,
                    path: display.to_string(),
                    reason: None,
                    kind: MatchKind::Name,
                });
            }
            for keyword in &stock.keywords {
                let reason = Some(keyword.reason.clone()).filter(|r| !r.is_empty());
                let mut push = |text: &str| {
                    terms.push(Term {
                        words: to_words(text),
                        level: keyword.level,
                        path: keyword.path.clone(),
                        reason: reason.clone(),
                        kind: MatchKind::Keyword,
                    });
                };
                push(&keyword.text_ko);
                if let Some(en) = keyword.text_en.as_deref().filter(|t| !t.is_empty()) {
                    push(en);
                }
            }
            // 가까운 경로가 먼저 오게 한다(정렬은 안정적이라 같은 단계는 원래 순서).
            terms.sort_by_key(|t| t.level);
            terms.retain(|t| !t.words.is_empty());
            Prepared {
                id: stock.id,
                terms,
            }
        })
        .collect();
    Matcher { prepared }
}

impl Matcher {
    pub fn find(&self, text: &str) -> Vec<StockMatch> {
        let haystack = normalize(text);
        let mut matches = Vec::new();
        for stock in &self.prepared {
            let hit = stock
                .terms
                .iter()
                .find(|term| term.words.iter().all(|word| contains_word(&haystack, word)));
            if let Some(hit) = hit {
                matches.push(StockMatch {
                    stock_id: stock.id,
                    keyword_path: hit.path.clone(),
                    keyword_reason: hit.reason.clone(),
                    match_kind: hit.kind,
                    level: hit.level,
                });
            }
        }
        matches
    }
}

static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]+")
        .expect("정규식이 올바르다")
});
static EDGE_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$").expect("정규식이 올바르다"));
static ASCII_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9.&+'-]*$").expect("정규식이 올바르다"));

fn normalize(text: &str) -> String {
    WHITESPACE
        .replace_all(&text.to_lowercase(), " ")
        .into_owned()
}

fn to_words(text: &str) -> Vec<String> {
    normalize(text)
        .split(' ')
        .map(|word| EDGE_PUNCTUATION.replace_all(word, "").into_owned())
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

/// 영문·숫자 낱말은 앞뒤가 영문·숫자가 아닐 때만 찾는다(JS `(?<![a-z0-9])word(?![a-z0-9])`). 그 밖의 낱말은 포함 여부만 본다.
fn contains_word(haystack: &str, word: &str) -> bool {
    if !ASCII_WORD.is_match(word) {
        return haystack.contains(word);
    }
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(word) {
        let start = from + offset;
        let end = start + word.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok {
            return true;
        }
        // 겹치는 위치도 찾는다(정규식은 다음 시작 위치부터 다시 시도한다).
        from = start + haystack[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

#[cfg(test)]
#[path = "match_tests.rs"]
mod tests;
