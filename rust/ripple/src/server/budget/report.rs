use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::server::budget::usage::month_spend_usd;
use crate::server::db::schema::UsageProvider;
use crate::server::settings::{BUDGET_MONTHLY_USD, read_num};
use crate::server::time::{EpochMs, start_of_seoul_month};

/// 이만큼 쓰면 미리 알린다(D-07, 2026-10-21 결정).
pub const BUDGET_WARN_RATIO: f64 = 0.8;

/// ok: 여유, warn: 80% 넘음, over: 한도에 닿아 AI 호출을 멈춤(D-07)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetLevel {
    Ok,
    Warn,
    Over,
}

pub fn budget_level(spent_usd: f64, limit_usd: f64) -> BudgetLevel {
    if spent_usd >= limit_usd {
        BudgetLevel::Over
    } else if spent_usd >= limit_usd * BUDGET_WARN_RATIO {
        BudgetLevel::Warn
    } else {
        BudgetLevel::Ok
    }
}

/// 사용량 기록의 용도(purpose)를 화면 이름으로. 모르는 용도는 None(용도 글자 그대로 보인다).
pub fn purpose_label(purpose: &str) -> Option<&'static str> {
    Some(match purpose {
        "stock_match" => "종목 찾기",
        "keyword_generation" => "키워드 만들기",
        "market_issues" => "시장 이슈 정리",
        "topic_refresh" => "토픽 갱신",
        "keyword_rating" => "키워드·토픽 평가",
        "article_evaluation" => "기사 평가",
        "keyword_generation_check"
        | "market_issues_check"
        | "topic_refresh_check"
        | "keyword_rating_check"
        | "article_evaluation_check"
        | "model_compare" => "품질 확인(개발 중 직접 호출)",
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub provider: UsageProvider,
    pub purpose: String,
    pub label: String,
    pub calls: i64,
    pub cost_usd: f64,
    /// 이번 달 전체 비용 중 비율 0~1
    pub share: f64,
}

/// 항목별 비용의 묶음(11단계 결정): 종목·시장·주제(사용자 주제·자동 토픽)·공용(토픽 갱신·시장 이슈·종목 찾기 등).
/// Unknown은 항목마다 쓰는 용도인데 항목을 남기지 않은 기록(11단계 전)이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemGroup {
    Stock,
    Market,
    Topic,
    Shared,
    Unknown,
}

impl ItemGroup {
    pub const ALL: [ItemGroup; 5] = [
        ItemGroup::Stock,
        ItemGroup::Market,
        ItemGroup::Topic,
        ItemGroup::Shared,
        ItemGroup::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ItemGroup::Stock => "종목",
            ItemGroup::Market => "시장",
            ItemGroup::Topic => "주제·토픽",
            ItemGroup::Shared => "공용",
            ItemGroup::Unknown => "항목 기록 없음(11단계 전)",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemUsageRow {
    /// 화면 key: 종목·시장은 항목 id, 사용자 주제는 토픽 id, 자동 토픽은 하나로 묶음
    pub key: String,
    pub group: ItemGroup,
    pub name: String,
    /// 지운 항목
    pub deleted: bool,
    pub calls: i64,
    pub cost_usd: f64,
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupUsageRow {
    pub group: ItemGroup,
    pub label: &'static str,
    pub calls: i64,
    pub cost_usd: f64,
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageReport {
    pub rows: Vec<UsageRow>,
    pub groups: Vec<GroupUsageRow>,
    pub items: Vec<ItemUsageRow>,
    pub total_usd: f64,
    pub limit_usd: f64,
    pub level: BudgetLevel,
}

/// 항목마다 쓰는 용도: 항목이 없으면 11단계 전 기록이다.
fn is_per_item_purpose(purpose: &str) -> bool {
    matches!(
        purpose,
        "keyword_generation" | "keyword_rating" | "article_evaluation"
    )
}

/// 이번 달(한국 시간) 제공자·용도별 호출 수와 비용, 한도(SPEC FR-8, 2026-10-21 결정).
/// 11단계: 기능별 비율과, 항목별(종목·시장·주제·공용) 비용·비율도 낸다.
pub fn month_usage_report(conn: &Connection, now: EpochMs) -> rusqlite::Result<UsageReport> {
    let month_start = start_of_seoul_month(now);
    let mut stmt = conn.prepare(
        "SELECT provider, purpose, count(*), coalesce(sum(cost_usd), 0) FROM usage_logs \
         WHERE at >= ?1 GROUP BY provider, purpose ORDER BY sum(cost_usd) DESC",
    )?;
    let raw: Vec<(String, String, i64, f64)> = stmt
        .query_map(params![month_start], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;

    // 화면 이름이 같은 용도(개발 중 품질 확인 여러 가지)는 제공자마다 한 줄로 합친다(처음 나온 순서를 지킨다).
    let mut merged: Vec<UsageRow> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    for (provider, purpose, calls, cost) in raw {
        let provider: UsageProvider = provider.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
        let label = purpose_label(&purpose).map_or_else(|| purpose.clone(), str::to_string);
        let key = (provider.as_str().to_string(), label.clone());
        match index.get(&key) {
            Some(&i) => {
                merged[i].calls += calls;
                merged[i].cost_usd += cost;
            }
            None => {
                index.insert(key, merged.len());
                merged.push(UsageRow {
                    provider,
                    purpose,
                    label,
                    calls,
                    cost_usd: cost,
                    share: 0.0,
                });
            }
        }
    }
    let total_usd = month_spend_usd(conn, now)?;
    let limit_usd = read_num(conn, &BUDGET_MONTHLY_USD)?;
    let share = |cost: f64| {
        if total_usd > 0.0 {
            cost / total_usd
        } else {
            0.0
        }
    };
    let (groups, items) = usage_by_item(conn, month_start)?;

    let mut rows: Vec<UsageRow> = merged
        .into_iter()
        .map(|row| UsageRow {
            share: share(row.cost_usd),
            ..row
        })
        .collect();
    rows.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd));
    Ok(UsageReport {
        rows,
        groups: groups
            .into_iter()
            .map(|g| GroupUsageRow {
                share: share(g.cost_usd),
                ..g
            })
            .collect(),
        items: items
            .into_iter()
            .map(|i| ItemUsageRow {
                share: share(i.cost_usd),
                ..i
            })
            .collect(),
        total_usd,
        limit_usd,
        level: budget_level(total_usd, limit_usd),
    })
}

struct ItemQueryRow {
    stock_id: Option<i64>,
    purpose: String,
    kind: Option<String>,
    name: Option<String>,
    name_ko: Option<String>,
    deleted_at: Option<i64>,
    topic_id: Option<i64>,
    topic_source: Option<String>,
    topic_name: Option<String>,
    topic_deleted_at: Option<i64>,
    calls: i64,
    cost_usd: f64,
}

/// 이번 달 비용을 항목별로 모은다. 같은 주제의 증시별 항목은 하나로, 자동 토픽은 모두 한 줄로 묶는다.
fn usage_by_item(
    conn: &Connection,
    month_start: EpochMs,
) -> rusqlite::Result<(Vec<GroupUsageRow>, Vec<ItemUsageRow>)> {
    let mut stmt = conn.prepare(
        "SELECT usage_logs.stock_id, usage_logs.purpose, stocks.kind, stocks.name, stocks.name_ko, \
                stocks.deleted_at, stocks.topic_id, topics.source, topics.name, topics.deleted_at, \
                count(*), coalesce(sum(usage_logs.cost_usd), 0) \
         FROM usage_logs \
         LEFT JOIN stocks ON stocks.id = usage_logs.stock_id \
         LEFT JOIN topics ON topics.id = stocks.topic_id \
         WHERE usage_logs.at >= ?1 \
         GROUP BY usage_logs.stock_id, usage_logs.purpose",
    )?;
    let rows: Vec<ItemQueryRow> = stmt
        .query_map(params![month_start], |r| {
            Ok(ItemQueryRow {
                stock_id: r.get(0)?,
                purpose: r.get(1)?,
                kind: r.get(2)?,
                name: r.get(3)?,
                name_ko: r.get(4)?,
                deleted_at: r.get(5)?,
                topic_id: r.get(6)?,
                topic_source: r.get(7)?,
                topic_name: r.get(8)?,
                topic_deleted_at: r.get(9)?,
                calls: r.get(10)?,
                cost_usd: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    // 처음 나온 순서를 지키는 모음(JS Map 대응)
    let mut items: Vec<ItemUsageRow> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut auto_topics: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for row in rows {
        let (key, group, name, deleted) = match (&row.stock_id, row.kind.as_deref()) {
            (Some(_), Some(kind)) => {
                if kind == "topic" && row.topic_source.as_deref() == Some("auto") {
                    if let Some(topic_id) = row.topic_id {
                        auto_topics.insert(topic_id);
                    }
                    (
                        "auto-topics".to_string(),
                        ItemGroup::Topic,
                        "자동 토픽".to_string(),
                        false,
                    )
                } else if kind == "topic" {
                    (
                        format!(
                            "topic-{}",
                            row.topic_id
                                .map_or_else(|| "null".to_string(), |id| id.to_string())
                        ),
                        ItemGroup::Topic,
                        row.topic_name
                            .clone()
                            .or(row.name_ko.clone())
                            .or(row.name.clone())
                            .unwrap_or_default(),
                        row.topic_deleted_at.is_some(),
                    )
                } else {
                    let group = if kind == "market" {
                        ItemGroup::Market
                    } else {
                        ItemGroup::Stock
                    };
                    (
                        format!("item-{}", row.stock_id.unwrap_or_default()),
                        group,
                        row.name_ko.clone().or(row.name.clone()).unwrap_or_default(),
                        row.deleted_at.is_some(),
                    )
                }
            }
            _ => {
                let group = if row.stock_id.is_none() && !is_per_item_purpose(&row.purpose) {
                    ItemGroup::Shared
                } else {
                    ItemGroup::Unknown
                };
                let name = if group == ItemGroup::Shared {
                    "공용(토픽 갱신·시장 이슈·종목 찾기 등)".to_string()
                } else {
                    ItemGroup::Unknown.label().to_string()
                };
                (
                    if group == ItemGroup::Shared {
                        "shared"
                    } else {
                        "unknown"
                    }
                    .to_string(),
                    group,
                    name,
                    false,
                )
            }
        };
        match index.get(&key) {
            Some(&i) => {
                items[i].calls += row.calls;
                items[i].cost_usd += row.cost_usd;
            }
            None => {
                index.insert(key.clone(), items.len());
                items.push(ItemUsageRow {
                    key,
                    group,
                    name,
                    deleted,
                    calls: row.calls,
                    cost_usd: row.cost_usd,
                    share: 0.0,
                });
            }
        }
    }
    if let Some(&i) = index.get("auto-topics") {
        items[i].name = format!("자동 토픽({}개)", auto_topics.len());
    }

    let groups = ItemGroup::ALL
        .iter()
        .filter_map(|&group| {
            let mine: Vec<&ItemUsageRow> =
                items.iter().filter(|item| item.group == group).collect();
            if mine.is_empty() {
                return None;
            }
            Some(GroupUsageRow {
                group,
                label: group.label(),
                calls: mine.iter().map(|item| item.calls).sum(),
                cost_usd: mine.iter().fold(0.0, |sum, item| sum + item.cost_usd),
                share: 0.0,
            })
        })
        .collect();
    // 비용이 큰 것부터, 같으면 이름 순(한국어 정렬은 유니코드 순서로 바꿨다: 설계 8장)
    items.sort_by(|a, b| {
        b.cost_usd
            .total_cmp(&a.cost_usd)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok((groups, items))
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
