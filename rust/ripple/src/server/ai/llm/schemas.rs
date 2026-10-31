//! OpenRouter 구조화 출력 JSON Schema. TS의 zod 스키마를 `rust/tools/dump-schemas.ts`가 내보낸 파일 그대로 쓴다
//! (같은 파일을 쓰므로 TS와 글자까지 같다. 파일이 TS와 어긋나지 않는지는 테스트가 TS를 다시 돌려 확인한다).
use std::sync::LazyLock;

use serde_json::Value;

fn load(text: &str) -> Value {
    serde_json::from_str(text).expect("스키마 파일은 올바른 JSON이다")
}

static STOCK_CANDIDATES: LazyLock<Value> =
    LazyLock::new(|| load(include_str!("schemas/stock_candidates.json")));
static KEYWORD_TREE: LazyLock<Value> =
    LazyLock::new(|| load(include_str!("schemas/keyword_tree.json")));
static TOPIC_REFRESH: LazyLock<Value> =
    LazyLock::new(|| load(include_str!("schemas/topic_refresh.json")));

/// 종목 매칭 후보(`stocks/match.ts`의 `matchOutputSchema`)
pub fn stock_candidates() -> Value {
    STOCK_CANDIDATES.clone()
}

/// 키워드 트리(`keywords/tree.ts`의 `keywordTreeSchema`)
pub fn keyword_tree() -> Value {
    KEYWORD_TREE.clone()
}

/// 토픽 갱신(`topics/refresh.ts`의 `topicRefreshSchema`)
pub fn topic_refresh() -> Value {
    TOPIC_REFRESH.clone()
}

#[cfg(test)]
#[path = "schemas_tests.rs"]
mod tests;
