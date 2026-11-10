use std::collections::HashSet;

use rusqlite::Connection;

use super::repository::Stock;

/// 한 종목에 둘 수 있는 별칭의 최대 개수
pub const MAX_ALIASES: usize = 5;

/// 별칭 최소 글자 수
const MIN_ALIAS_CHARS: usize = 2;

/// 공백을 빼고 소문자로 바꾼 비교용 글자
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// LLM이 낸 별칭 후보에서 쓸 수 없는 것을 버린다(최대 5개).
/// 버리는 것: 2자 미만, 종목 자기 이름·코드와 같은 것, `listings`의 다른 종목 이름과 같거나 포함하거나 포함되는 것,
/// 이미 지운 별칭, 앞에서 이미 고른 것. 비교는 공백을 빼고 소문자로 한다.
pub fn validate_aliases(
    conn: &Connection,
    stock: &Stock,
    candidates: &[String],
) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM listings WHERE NOT (market = ?1 AND code = ?2)")?;
    let others: Vec<String> = stmt
        .query_map([stock.market.as_str(), stock.code.as_str()], |r| {
            r.get::<_, String>(0)
        })?
        .map(|name| name.map(|n| normalize(&n)))
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|n| !n.is_empty())
        .collect();

    let mut banned: HashSet<String> = stock.aliases_removed.iter().map(|a| normalize(a)).collect();
    banned.insert(normalize(&stock.name));
    banned.insert(normalize(&stock.code));
    if let Some(name_ko) = &stock.name_ko {
        banned.insert(normalize(name_ko));
    }

    let mut chosen: Vec<String> = Vec::new();
    for candidate in candidates {
        if chosen.len() >= MAX_ALIASES {
            break;
        }
        let alias = candidate.trim();
        let key = normalize(alias);
        if alias.chars().count() < MIN_ALIAS_CHARS || banned.contains(&key) {
            continue;
        }
        if others
            .iter()
            .any(|other| *other == key || other.contains(&key) || key.contains(other.as_str()))
        {
            continue;
        }
        banned.insert(key);
        chosen.push(alias.to_string());
    }
    Ok(chosen)
}

#[cfg(test)]
#[path = "aliases_tests.rs"]
mod tests;
