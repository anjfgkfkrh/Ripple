//! 같은 소식을 다른 언론사가 낸 기사인지 제목으로 본다(11단계, 2026-10-22 실측으로 정함).
//! 머리말([속보] 등)·기호를 뺀 제목을 두 글자씩 잘라, '…' 앞부분의 겹침 비율이 0.5 이상이거나
//! 제목 전체가 0.4 이상이면 같은 소식이다. 같은 항목·24시간 안에서만 비교한다.
//! 평가 점수 복사(같은 소식은 한 번만 Jev에 묻는다)와 타임라인 묶기에 같이 쓴다.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

/// 같은 소식으로 볼 수 있는 발행 시각 차이
pub const SAME_NEWS_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const LEAD_THRESHOLD: f64 = 0.5;
const ALL_THRESHOLD: f64 = 0.4;

/// 제목 앞의 [속보]·(종합)·【단독】 같은 머리말. JS `\s`는 BOM도 공백이라 `\u{FEFF}`를 더한다(설계 8장).
static PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s\u{FEFF}]*(?:[\[【(][^\]】)]{1,12}[\]】)][\s\u{FEFF}]*)+")
        .expect("정규식이 올바르다")
});
/// 핵심 내용과 덧붙인 내용을 가르는 표시
static CLAUSE_BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"…|⋯|\.\.\.|···| - | \| |—").expect("정규식이 올바르다"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleFeatures {
    pub all: HashSet<String>,
    pub lead: HashSet<String>,
}

pub fn title_features(title: &str) -> TitleFeatures {
    let body = PREFIX.replace(title, "");
    let lead = CLAUSE_BREAK.split(&body).next().unwrap_or("");
    TitleFeatures {
        all: bigrams(&normalize(&body)),
        lead: bigrams(&normalize(lead)),
    }
}

pub fn is_same_news(a: &TitleFeatures, b: &TitleFeatures) -> bool {
    jaccard(&a.lead, &b.lead) >= LEAD_THRESHOLD || jaccard(&a.all, &b.all) >= ALL_THRESHOLD
}

/// 숫자·영문·한글만 남기고 소문자로. 남는 글자는 모두 UTF-16 한 단위라 `chars()`가 JS와 같다.
fn normalize(text: &str) -> Vec<char> {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || ('가'..='힣').contains(c))
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn bigrams(chars: &[char]) -> HashSet<String> {
    chars.windows(2).map(|pair| pair.iter().collect()).collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|gram| b.contains(*gram)).count();
    shared as f64 / (a.len() + b.len() - shared) as f64
}

#[cfg(test)]
#[path = "similar_title_tests.rs"]
mod tests;
