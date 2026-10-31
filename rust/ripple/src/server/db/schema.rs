//! `src/server/db/schema.ts`의 글자 enum들. 표 모양은 `migrations/*.sql`(drizzle 마이그레이션 그대로)이 정한다.
//! DB에는 글자 그대로 저장하므로 `as_str`·`FromStr`의 글자는 TS의 값과 같아야 한다.

use std::str::FromStr;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok($name::$variant),)+
                    other => Err(format!("{}: 알 수 없는 값 {other}", stringify!($name))),
                }
            }
        }
    };
}

string_enum!(Market { Kr => "KR", Us => "US" });

string_enum!(
    /// 키워드 생성 상태 (D-37: 등록 직후 worker가 비동기로 만든다)
    KeywordStatus { Pending => "pending", Generating => "generating", Ready => "ready", Failed => "failed" }
);

string_enum!(
    /// 항목 종류: 종목 / 시장(한국·미국 증시) / 주제(사용자 주제·자동 토픽) (11단계)
    ItemKind { Stock => "stock", Market => "market", Topic => "topic" }
);

string_enum!(
    /// 키워드 종류 (10단계): 구조 / 시사
    KeywordKind { Structure => "structure", Current => "current" }
);

string_enum!(TopicSource { Auto => "auto", User => "user" });
string_enum!(TopicStatus { Active => "active", Ended => "ended" });
string_enum!(ArticleType { News => "news", Report => "report" });
string_enum!(MatchKind { Direct => "direct", Name => "name", Keyword => "keyword" });
string_enum!(RunStatus { Running => "running", Success => "success", Partial => "partial", Failed => "failed" });
string_enum!(
    /// 실행 사유: 직접 실행 / 정기 / 놓친 회차 보정(6단계)
    RunTrigger { Manual => "manual", Scheduled => "scheduled", CatchUp => "catch_up" }
);
string_enum!(Impact { Positive => "positive", Negative => "negative", Neutral => "neutral" });
string_enum!(PriceSource { Kis => "kis", Finnhub => "finnhub" });
string_enum!(
    /// `usage_logs.provider`(schema.ts의 `enum: ["openrouter", "typesafe"]`)
    UsageProvider { Openrouter => "openrouter", Typesafe => "typesafe" }
);

/// 소스 하나의 수집 결과(`collection_runs.sources` JSON의 한 칸)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceResult {
    /// 피드 id
    pub id: String,
    /// 화면 표시용(예: 조선비즈 증권)
    pub label: String,
    /// 화면에서 묶어 보여 줄 출처(예: 조선비즈, Finnhub)
    pub group: String,
    /// 받은 기사 수
    pub fetched: i64,
    /// 기준 시각 이후 기사 수
    pub fresh: i64,
    /// 새로 저장한 기사 수
    pub saved: i64,
    /// 새로 만든 기사–종목 연결 수
    pub matched: i64,
    /// 이 소스만 회차 기준 시각과 다른 기준 시각(ms)을 썼을 때(지난번에 실패한 소스, D-24)
    pub since: Option<i64>,
    pub error: Option<String>,
    pub ms: i64,
}

impl SourceResult {
    /// TS 객체의 키 순서(`ms` 뒤에 `error`)와 같은 JSON
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), self.id.clone().into());
        map.insert("label".into(), self.label.clone().into());
        map.insert("group".into(), self.group.clone().into());
        map.insert("fetched".into(), self.fetched.into());
        map.insert("fresh".into(), self.fresh.into());
        map.insert("saved".into(), self.saved.into());
        map.insert("matched".into(), self.matched.into());
        if let Some(since) = self.since {
            map.insert("since".into(), since.into());
        }
        map.insert("ms".into(), self.ms.into());
        if let Some(error) = &self.error {
            map.insert("error".into(), error.clone().into());
        }
        serde_json::Value::Object(map)
    }

    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| value.get(key)?.as_str().map(str::to_string);
        let int = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_f64)
                .map(|v| v as i64)
        };
        Some(Self {
            id: text("id")?,
            label: text("label")?,
            group: text("group")?,
            fetched: int("fetched")?,
            fresh: int("fresh")?,
            saved: int("saved")?,
            matched: int("matched")?,
            since: int("since"),
            error: text("error"),
            ms: int("ms")?,
        })
    }
}

/// 수집 회차 안에서 한 Jev 평가 요약(`collection_runs.evaluation` JSON)
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationSummary {
    /// 새로 평가한 기사–종목 연결 수
    pub evaluated: i64,
    /// 그중 표시 기준(관련도·영향도, 11단계) 이상
    pub relevant: i64,
    /// 같은 소식(비슷한 제목)의 앞선 평가를 그대로 쓴 수(Jev 호출 없음, 11단계)
    pub copied: i64,
    /// 실패해 다음 회차로 미룬 수
    pub failed: i64,
    /// 예산·키 문제 등으로 손대지 않고 남긴 수
    pub remaining: i64,
    /// 이 회차 평가 비용(달러)
    pub cost_usd: f64,
    pub ms: i64,
    /// 멈춘 이유(예산 초과, 키 없음 등)
    pub note: Option<String>,
}

impl EvaluationSummary {
    pub fn empty() -> Self {
        Self {
            evaluated: 0,
            relevant: 0,
            copied: 0,
            failed: 0,
            remaining: 0,
            cost_usd: 0.0,
            ms: 0,
            note: None,
        }
    }

    /// TS 객체의 키 순서(`evaluated, relevant, copied, failed, remaining, costUsd, ms, note`)와 같은 JSON
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("evaluated".into(), self.evaluated.into());
        map.insert("relevant".into(), self.relevant.into());
        map.insert("copied".into(), self.copied.into());
        map.insert("failed".into(), self.failed.into());
        map.insert("remaining".into(), self.remaining.into());
        map.insert(
            "costUsd".into(),
            crate::server::text::json_number(self.cost_usd),
        );
        map.insert("ms".into(), self.ms.into());
        if let Some(note) = &self.note {
            map.insert("note".into(), note.clone().into());
        }
        serde_json::Value::Object(map)
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
