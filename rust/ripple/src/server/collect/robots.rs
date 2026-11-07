use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use regex::Regex;

use crate::server::collect::http::HttpClient;
use crate::server::text::js_trim;
use crate::server::time::{EpochMs, now_ms};

/// robots.txt에서 우리를 가리키는 이름(User-agent 줄과 대소문자 무시 비교)
pub const ROBOTS_AGENT: &str = "ripple";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub allow: bool,
    pub pattern: String,
}

static LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([A-Za-z-]+)[\s\u{FEFF}]*:[\s\u{FEFF}]*(.*)$").expect("정규식이 올바르다")
});
static LINE_BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\r?\n").expect("정규식이 올바르다"));

struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
}

/// RFC 9309 방식으로 우리에게 적용되는 규칙을 고른다.
/// 'ripple'을 가리키는 그룹이 있으면 그것을, 없으면 '*' 그룹을 쓴다.
pub fn parse_robots(text: &str, agent: &str) -> Vec<Rule> {
    let mut groups: Vec<Group> = Vec::new();
    let mut last_was_agent = false;
    for raw_line in LINE_BREAK.split(text) {
        let without_comment = raw_line.split('#').next().unwrap_or("");
        let line = js_trim(without_comment);
        let Some(caps) = LINE.captures(line) else {
            continue;
        };
        let key = caps[1].to_lowercase();
        let value = js_trim(&caps[2]).to_string();
        if key == "user-agent" {
            if groups.is_empty() || !last_was_agent {
                groups.push(Group {
                    agents: Vec::new(),
                    rules: Vec::new(),
                });
            }
            if let Some(group) = groups.last_mut() {
                group.agents.push(value.to_lowercase());
            }
            last_was_agent = true;
        } else if (key == "allow" || key == "disallow") && !groups.is_empty() {
            if let Some(group) = groups.last_mut() {
                group.rules.push(Rule {
                    allow: key == "allow",
                    pattern: value,
                });
            }
            last_was_agent = false;
        } else {
            last_was_agent = false;
        }
    }
    let agent = agent.to_lowercase();
    let mine: Vec<&Group> = groups
        .iter()
        .filter(|g| g.agents.contains(&agent))
        .collect();
    let chosen = if mine.is_empty() {
        groups
            .iter()
            .filter(|g| g.agents.iter().any(|a| a == "*"))
            .collect()
    } else {
        mine
    };
    chosen
        .into_iter()
        .flat_map(|g| g.rules.iter().cloned())
        .collect()
}

/// 가장 길게 맞는 규칙을 따른다. 길이가 같으면 허용. 맞는 규칙이 없으면 허용.
pub fn is_allowed(rules: &[Rule], path_with_query: &str) -> bool {
    let mut best: Option<(usize, bool)> = None;
    for rule in rules {
        if rule.pattern.is_empty() || !pattern_matches(&rule.pattern, path_with_query) {
            continue;
        }
        // JS `pattern.length`는 UTF-16 단위
        let length = rule.pattern.encode_utf16().count();
        if best.is_none_or(|(best_length, _)| {
            length > best_length || (length == best_length && rule.allow)
        }) {
            best = Some((length, rule.allow));
        }
    }
    best.is_none_or(|(_, allow)| allow)
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let body = if anchored {
        &pattern[..pattern.len() - 1]
    } else {
        pattern
    };
    let source = body
        .split('*')
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(".*");
    let source = format!("^{source}{}", if anchored { "$" } else { "" });
    // 올바르지 않은 정규식이 나오는 일은 없다(모든 조각을 이스케이프했다). 만들지 못하면 맞지 않는 것으로 본다.
    Regex::new(&source).is_ok_and(|re| re.is_match(path))
}

/// 이 주소를 가져가도 되는지에 대한 답
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RobotsVerdict {
    Allowed,
    Disallowed,
    /// robots.txt를 확인하지 못했다(이번에는 가져가지 않는다). 값은 원인.
    Unknown(String),
}

/// 이 주소를 가져가도 되는지 묻는 쪽(`RobotsChecker`, 테스트의 가짜)
pub trait RobotsCheck: Send + Sync {
    fn check(&self, url: &str) -> impl std::future::Future<Output = RobotsVerdict> + Send;
}

impl<H: HttpClient> RobotsCheck for RobotsChecker<H> {
    fn check(&self, url: &str) -> impl std::future::Future<Output = RobotsVerdict> + Send {
        RobotsChecker::check(self, url)
    }
}

struct CacheEntry {
    rules: Vec<Rule>,
    at: EpochMs,
}

/// 사이트마다 robots.txt를 하루 동안 기억한다. 없으면(4xx) 모두 허용, 서버 오류·연결 실패면 막힌 것으로 본다(RFC 9309).
pub struct RobotsChecker<H> {
    http: H,
    ttl_ms: i64,
    now: Box<dyn Fn() -> EpochMs + Send + Sync>,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl<H: HttpClient> RobotsChecker<H> {
    pub fn new(http: H) -> Self {
        Self::with_options(http, 24 * 60 * 60 * 1000, Box::new(now_ms))
    }

    pub fn with_options(http: H, ttl_ms: i64, now: Box<dyn Fn() -> EpochMs + Send + Sync>) -> Self {
        Self {
            http,
            ttl_ms,
            now,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// 확인하지 못했으면 그 원인
    async fn rules_for(&self, origin: &str) -> Result<Vec<Rule>, String> {
        {
            let cache = self.cache.lock().expect("robots 캐시 잠금이 깨졌습니다");
            if let Some(hit) = cache.get(origin)
                && (self.now)() - hit.at < self.ttl_ms
            {
                return Ok(hit.rules.clone());
            }
        }
        let rules = match self.http.get(&format!("{origin}/robots.txt"), &[]).await {
            Ok(response) if (200..300).contains(&response.status) => {
                Ok(parse_robots(&response.text, ROBOTS_AGENT))
            }
            Ok(response) if (400..500).contains(&response.status) => Ok(Vec::new()),
            Ok(response) => Err(format!("HTTP {}", response.status)),
            Err(error) => Err(error.message),
        };
        // 확인하지 못한 경우는 기억하지 않고 다음 회차에 다시 본다.
        if let Ok(rules) = &rules {
            self.cache
                .lock()
                .expect("robots 캐시 잠금이 깨졌습니다")
                .insert(
                    origin.to_string(),
                    CacheEntry {
                        rules: rules.clone(),
                        at: (self.now)(),
                    },
                );
        }
        rules
    }

    /// 이 주소를 가져가도 되는지. robots.txt를 확인하지 못하면 이번에는 가져가지 않는다.
    pub async fn check(&self, url: &str) -> RobotsVerdict {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(error) => {
                return RobotsVerdict::Unknown(format!("주소를 읽지 못했습니다: {error}"));
            }
        };
        let origin = parsed.origin().ascii_serialization();
        let rules = match self.rules_for(&origin).await {
            Ok(rules) => rules,
            Err(cause) => return RobotsVerdict::Unknown(cause),
        };
        let search = parsed
            .query()
            .filter(|q| !q.is_empty())
            .map_or(String::new(), |q| format!("?{q}"));
        if is_allowed(&rules, &format!("{}{search}", parsed.path())) {
            RobotsVerdict::Allowed
        } else {
            RobotsVerdict::Disallowed
        }
    }
}

#[cfg(test)]
#[path = "robots_tests.rs"]
mod tests;
