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

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
