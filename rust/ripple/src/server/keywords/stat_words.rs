use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, params};

use crate::server::evaluate::run::meets_display_rule;
use crate::server::time::{DAY_MS, EpochMs};

/// 통계 낱말을 셀 기사 기간
pub const STAT_DAYS: i64 = 14;
/// 프롬프트에 넣는 낱말 수
pub const STAT_TOP: usize = 30;
/// 낱말이 나와야 하는 최소 기사 수(또는 기사 수의 3% 중 큰 쪽)
const MIN_COUNT: f64 = 3.0;
const MIN_SHARE: f64 = 0.03;

/// 종목의 통과 기사 제목에 유난히 자주 나온 낱말(키워드 트리를 새로 만들 때 참고 자료, 17단계 결정)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatWords {
    /// 센 기사(서로 다른 제목) 수
    pub articles: usize,
    /// (낱말, 그 낱말이 든 기사 수), 점수 순서
    pub words: Vec<(String, usize)>,
}

static SPLIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[\s·…⋯,/'"‘’“”\[\]()]+"#).expect("정규식이 올바르다"));
static EDGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\W+|\W+$").expect("정규식이 올바르다"));
static JOSA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("(으로|에서|에게|까지|부터|이라|라는|은|는|이|가|을|를|의|에|도|와|과|로|만)$")
        .expect("정규식이 올바르다")
});

/// 제목을 낱말 집합으로 쪼갠다: 구분 글자로 나누고, 앞뒤 기호를 떼고, 3자 이상이면 끝의 조사 하나를 떼고,
/// 2자 미만이거나 숫자만인 것은 버린다.
pub fn tokenize(title: &str) -> HashSet<String> {
    let lower = title.to_lowercase();
    let mut out = HashSet::new();
    for part in SPLIT.split(&lower) {
        let mut word = EDGE.replace_all(part, "").into_owned();
        if word.chars().count() >= 3 {
            word = JOSA.replace(&word, "").into_owned();
        }
        if word.chars().count() >= 2 && !word.chars().all(char::is_numeric) {
            out.insert(word);
        }
    }
    out
}

/// 순수 계산: 통과 기사 제목들(`titles`, 서로 다른 제목)과 배경 제목들로 점수 높은 낱말을 고른다.
/// 문턱은 `max(3, 기사 수 × 0.03)`, 점수는 `(c/n) / ((배경 수 + 1)/배경 전체) × ln(c)`, 위에서 30개.
/// 기사나 배경이 없으면 빈 목록.
pub fn score_words(titles: &[String], background: &[String]) -> StatWords {
    let n = titles.len();
    if n == 0 || background.is_empty() {
        return StatWords {
            articles: n,
            words: Vec::new(),
        };
    }
    let mut bg: HashMap<String, usize> = HashMap::new();
    for title in background {
        for word in tokenize(title) {
            *bg.entry(word).or_default() += 1;
        }
    }
    let mut fg: HashMap<String, usize> = HashMap::new();
    for title in titles {
        for word in tokenize(title) {
            *fg.entry(word).or_default() += 1;
        }
    }
    let threshold = MIN_COUNT.max(n as f64 * MIN_SHARE);
    let background_total = background.len() as f64;
    let mut scored: Vec<(f64, String, usize)> = fg
        .into_iter()
        .filter(|(_, count)| *count as f64 >= threshold)
        .map(|(word, count)| {
            let c = count as f64;
            let in_bg = bg.get(&word).copied().unwrap_or(0) as f64;
            let score = (c / n as f64) / ((in_bg + 1.0) / background_total) * c.ln();
            (score, word, count)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.cmp(&a.1))
    });
    StatWords {
        articles: n,
        words: scored
            .into_iter()
            .take(STAT_TOP)
            .map(|(_, word, count)| (word, count))
            .collect(),
    }
}

/// 종목의 통계 낱말. 대상은 그 종목의 이름 연결(`match_kind = 'name'`) 평가 가운데 최근 14일에 평가했고
/// 화면 표시 기준을 통과한 것의 서로 다른 제목, 배경은 `feed_headlines`의 모든 제목이다.
pub fn stat_words(conn: &Connection, stock_id: i64, now: EpochMs) -> rusqlite::Result<StatWords> {
    let mut stmt = conn.prepare(
        "SELECT articles.title, evaluations.relevance, evaluations.magnitude FROM evaluations \
         INNER JOIN article_matches ON article_matches.id = evaluations.match_id \
         INNER JOIN articles ON articles.id = evaluations.article_id \
         WHERE evaluations.stock_id = ?1 AND article_matches.match_kind = 'name' \
           AND evaluations.evaluated_at >= ?2",
    )?;
    let rows = stmt
        .query_map(params![stock_id, now - STAT_DAYS * DAY_MS], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, Option<f64>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut titles: Vec<String> = Vec::new();
    for (title, relevance, magnitude) in rows {
        if meets_display_rule(conn, relevance, magnitude)? && seen.insert(title.clone()) {
            titles.push(title);
        }
    }
    if titles.is_empty() {
        return Ok(StatWords::default());
    }
    let mut stmt = conn.prepare("SELECT title FROM feed_headlines")?;
    let background = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(score_words(&titles, &background))
}

#[cfg(test)]
#[path = "stat_words_tests.rs"]
mod tests;
