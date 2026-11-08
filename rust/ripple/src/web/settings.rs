//! 설정 화면(`app/(app)/settings/*`, 14단계 R12): 설정 저장(검사와 저장은 worker가 한다, D-36),
//! 키워드 다시 만들기, 이번 달 사용량. 저장하면 Next `refresh()`가 하던 사용량·예산 배너 다시 그리기를 같은 응답에 넣는다.
use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::extract::{Form, State};
use axum::response::Response;
use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::server::budget::report::ItemGroup;
use crate::server::budget::report::{BudgetLevel, UsageReport, month_usage_report};
use crate::server::db::schema::UsageProvider;
use crate::server::keywords::read::stocks_with_other_depth;
use crate::server::settings::{JEV_MODEL, JEV_USD_PER_MILLION_INPUT_TOKENS, read_num, read_str};
use crate::server::settings_form::{SettingsForm, read_settings_form};
use crate::server::stocks::messages::describe_stock_error;
use crate::server::text::{js_number, js_to_fixed};
use crate::server::time::EpochMs;
use crate::server::worker_client::WorkerResult;
use crate::web::assets::AssetUrls;
use crate::web::layout::{BannerView, budget_banner};
use crate::web::response::render;
use crate::web::state::WebState;
use crate::web::status::unreachable_result;
use crate::web::stocks::{db_error, respond};

type Fields = Vec<(String, String)>;

/// 설정 화면 입력 이름(TS `SETTINGS_FIELDS`와 같은 순서). 저장할 때 이 이름으로 값을 모은다.
const FIELDS: [&str; 12] = [
    "collectIntervalMinutes",
    "keywordsDepth",
    "keywordsRegenerateEvery",
    "minRelevance",
    "minMagnitude",
    "keywordMinRelevance",
    "keywordMinMagnitude",
    "topicIntervalHours",
    "topicTimes",
    "maxPerRun",
    "monthlyBudgetUsd",
    "llmModel",
];

fn field(fields: &[(String, String)], name: &str) -> String {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map_or_else(String::new, |(_, value)| value.clone())
}

/// 입력칸에 넣을 글: 숫자는 JS `String(x)`와 같고, 시각 목록은 `", "`로 잇는다(TS `settingsFormText`)
#[derive(Clone)]
pub struct SettingsValues {
    pub collect_interval_minutes: String,
    pub keywords_depth: String,
    pub keywords_regenerate_every: String,
    pub min_relevance: String,
    pub min_magnitude: String,
    pub keyword_min_relevance: String,
    pub keyword_min_magnitude: String,
    pub topic_interval_hours: String,
    pub topic_times: String,
    pub max_per_run: String,
    pub monthly_budget_usd: String,
    pub llm_model: String,
}

impl SettingsValues {
    fn from_form(form: &SettingsForm) -> Self {
        Self {
            collect_interval_minutes: js_number(form.collect_interval_minutes),
            keywords_depth: js_number(form.keywords_depth),
            keywords_regenerate_every: form.keywords_regenerate_every.as_str().to_string(),
            min_relevance: js_number(form.min_relevance),
            min_magnitude: js_number(form.min_magnitude),
            keyword_min_relevance: js_number(form.keyword_min_relevance),
            keyword_min_magnitude: js_number(form.keyword_min_magnitude),
            topic_interval_hours: js_number(form.topic_interval_hours),
            topic_times: form.topic_times.join(", "),
            max_per_run: js_number(form.max_per_run),
            monthly_budget_usd: js_number(form.monthly_budget_usd),
            llm_model: form.llm_model.clone(),
        }
    }

    /// 제출한 입력 그대로(Next가 저장 결과의 `values`로 돌려주던 것)
    fn from_fields(fields: &[(String, String)]) -> Self {
        Self {
            collect_interval_minutes: field(fields, FIELDS[0]),
            keywords_depth: field(fields, FIELDS[1]),
            keywords_regenerate_every: field(fields, FIELDS[2]),
            min_relevance: field(fields, FIELDS[3]),
            min_magnitude: field(fields, FIELDS[4]),
            keyword_min_relevance: field(fields, FIELDS[5]),
            keyword_min_magnitude: field(fields, FIELDS[6]),
            topic_interval_hours: field(fields, FIELDS[7]),
            topic_times: field(fields, FIELDS[8]),
            max_per_run: field(fields, FIELDS[9]),
            monthly_budget_usd: field(fields, FIELDS[10]),
            llm_model: field(fields, FIELDS[11]),
        }
    }

    /// worker `PUT /settings`에 보내는 값(항목 이름 → 글)
    fn to_json(&self) -> Map<String, Value> {
        let pairs = [
            (FIELDS[0], &self.collect_interval_minutes),
            (FIELDS[1], &self.keywords_depth),
            (FIELDS[2], &self.keywords_regenerate_every),
            (FIELDS[3], &self.min_relevance),
            (FIELDS[4], &self.min_magnitude),
            (FIELDS[5], &self.keyword_min_relevance),
            (FIELDS[6], &self.keyword_min_magnitude),
            (FIELDS[7], &self.topic_interval_hours),
            (FIELDS[8], &self.topic_times),
            (FIELDS[9], &self.max_per_run),
            (FIELDS[10], &self.monthly_budget_usd),
            (FIELDS[11], &self.llm_model),
        ];
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), Value::String(value.clone())))
            .collect()
    }
}

/// 입력칸 아래 보일 항목별 오류(키는 입력 이름)
pub struct FormView {
    pub values: SettingsValues,
    pub errors: HashMap<String, String>,
    pub message: Option<String>,
    pub saved: bool,
}

pub struct RebuildView {
    pub count: usize,
    pub depth: String,
    pub message: Option<String>,
}

pub struct JevView {
    pub model: String,
    pub usd_per_million: String,
}

pub struct UsageLineView {
    pub label: String,
    pub sub: String,
    pub deleted: bool,
    pub calls: String,
    pub cost: String,
    pub share: String,
}

pub struct UsageView {
    pub oob: bool,
    pub total: String,
    pub limit: String,
    pub percent: String,
    pub width: String,
    pub level: BudgetLevel,
    pub has_rows: bool,
    pub features: Vec<UsageLineView>,
    pub groups: Vec<UsageLineView>,
    pub items: Vec<UsageLineView>,
}

// ---- 계산 ----

/// `toLocaleString("ko-KR")`의 자릿점(1,234)
fn group_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// `Math.round`(0.5는 위로)
fn round_js(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

/// 비율: 0.5% 미만은 `<1%`(TS `formatShare`)
fn format_share(share: f64) -> String {
    if share <= 0.0 {
        return "0%".to_string();
    }
    let percent = share * 100.0;
    if percent < 0.5 {
        "<1%".to_string()
    } else {
        format!("{}%", round_js(percent))
    }
}

fn provider_label(provider: UsageProvider) -> &'static str {
    match provider {
        UsageProvider::Openrouter => "OpenRouter",
        UsageProvider::Typesafe => "TypeSafe(Jev)",
    }
}

fn group_short(group: ItemGroup) -> &'static str {
    match group {
        ItemGroup::Stock => "종목",
        ItemGroup::Market => "시장",
        ItemGroup::Topic => "주제",
        ItemGroup::Shared => "공용",
        ItemGroup::Unknown => "기록 없음",
    }
}

fn line(
    label: String,
    sub: String,
    deleted: bool,
    calls: i64,
    cost_usd: f64,
    share: f64,
) -> UsageLineView {
    UsageLineView {
        label,
        sub,
        deleted,
        calls: format!("{}회", group_thousands(calls)),
        cost: format!("${}", js_to_fixed(cost_usd, 4)),
        share: format_share(share),
    }
}

fn usage_view(report: &UsageReport, oob: bool) -> UsageView {
    let percent = if report.limit_usd > 0.0 {
        (report.total_usd / report.limit_usd * 100.0).min(100.0)
    } else {
        100.0
    };
    UsageView {
        oob,
        total: js_to_fixed(report.total_usd, 4),
        limit: js_number(report.limit_usd),
        percent: round_js(percent).to_string(),
        width: js_number(percent),
        level: report.level,
        has_rows: !report.rows.is_empty(),
        features: report
            .rows
            .iter()
            .map(|row| {
                line(
                    row.label.clone(),
                    provider_label(row.provider).to_string(),
                    false,
                    row.calls,
                    row.cost_usd,
                    row.share,
                )
            })
            .collect(),
        groups: report
            .groups
            .iter()
            .map(|row| {
                line(
                    row.label.to_string(),
                    String::new(),
                    false,
                    row.calls,
                    row.cost_usd,
                    row.share,
                )
            })
            .collect(),
        items: report
            .items
            .iter()
            .map(|row| {
                line(
                    row.name.clone(),
                    group_short(row.group).to_string(),
                    row.deleted,
                    row.calls,
                    row.cost_usd,
                    row.share,
                )
            })
            .collect(),
    }
}

fn rebuild_view(conn: &Connection, message: Option<String>) -> rusqlite::Result<RebuildView> {
    let form = read_settings_form(conn)?;
    let count = stocks_with_other_depth(conn)?.len();
    Ok(RebuildView {
        count,
        depth: js_number(form.keywords_depth),
        message,
    })
}

// ---- 템플릿 ----

#[derive(Template)]
#[template(path = "settings/page.html")]
struct SettingsPageTemplate {
    assets: AssetUrls,
    banner: Option<BannerView>,
    member: bool,
    form: FormView,
    rebuild: RebuildView,
    jev: JevView,
    usage: UsageView,
}

#[derive(Template)]
#[template(path = "settings/form.html")]
struct FormTemplate {
    form: FormView,
}

#[derive(Template)]
#[template(path = "settings/rebuild.html")]
struct RebuildTemplate {
    rebuild: RebuildView,
}

#[derive(Template)]
#[template(path = "settings/usage.html")]
struct UsageTemplate {
    usage: UsageView,
}

#[derive(Template)]
#[template(path = "budget_oob.html")]
struct BudgetOobTemplate {
    banner: Option<BannerView>,
}

// ---- 화면 ----

/// `/settings`: 설정 화면 전체
pub async fn page(State(state): State<Arc<WebState>>) -> Response {
    let now = state.now();
    let data = state.db.with(|conn| page_data(conn, now));
    match data {
        Ok(data) => render(&SettingsPageTemplate {
            assets: state.assets.urls.clone(),
            banner: budget_banner(&state),
            member: true,
            form: FormView {
                values: data.values,
                errors: HashMap::new(),
                message: None,
                saved: false,
            },
            rebuild: data.rebuild,
            jev: data.jev,
            usage: data.usage,
        }),
        Err(reason) => db_error(&reason),
    }
}

struct PageData {
    values: SettingsValues,
    rebuild: RebuildView,
    jev: JevView,
    usage: UsageView,
}

fn page_data(conn: &Connection, now: EpochMs) -> rusqlite::Result<PageData> {
    let form = read_settings_form(conn)?;
    Ok(PageData {
        values: SettingsValues::from_form(&form),
        rebuild: rebuild_view(conn, None)?,
        jev: JevView {
            model: read_str(conn, &JEV_MODEL)?,
            usd_per_million: js_number(read_num(conn, &JEV_USD_PER_MILLION_INPUT_TOKENS)?),
        },
        usage: usage_view(&month_usage_report(conn, now)?, false),
    })
}

/// 설정 저장(SPEC FR-8, `saveSettingsAction`). 검사와 저장은 worker가 한다(D-36).
pub async fn save(State(state): State<Arc<WebState>>, Form(fields): Form<Fields>) -> Response {
    let values = SettingsValues::from_fields(&fields);
    let result = match state.worker_api() {
        Some(api) => api.update_settings(Value::Object(values.to_json())).await,
        None => unreachable_result(),
    };
    let mut form = FormView {
        values,
        errors: HashMap::new(),
        message: None,
        saved: false,
    };
    let mut saved = false;
    match result {
        WorkerResult::Ok { data, .. } => {
            saved = true;
            let changed = data["changed"].as_array().map_or(0, Vec::len);
            form.saved = true;
            form.message = Some(
                if changed > 0 {
                    "저장했습니다."
                } else {
                    "바뀐 값이 없습니다."
                }
                .to_string(),
            );
        }
        WorkerResult::Err { error, body, .. } => match error.as_str() {
            "invalid_settings" => {
                if let Some(errors) = body["fields"].as_object() {
                    for (key, message) in errors {
                        if let Some(text) = message.as_str() {
                            form.errors.insert(key.clone(), text.to_string());
                        }
                    }
                }
                form.message = Some("고칠 항목이 있어 저장하지 않았습니다.".to_string());
            }
            "unknown_model" => {
                form.errors.insert(
                    "llmModel".to_string(),
                    "OpenRouter에 없는 모델 ID입니다.".to_string(),
                );
                form.message = Some("고칠 항목이 있어 저장하지 않았습니다.".to_string());
            }
            "model_check_failed" => {
                form.errors.insert(
                    "llmModel".to_string(),
                    "OpenRouter 모델 목록을 확인하지 못했습니다. 잠시 뒤 다시 저장하세요."
                        .to_string(),
                );
                form.message = Some("저장하지 않았습니다.".to_string());
            }
            _ => form.message = Some(describe_stock_error(&error, &body)),
        },
    }
    if !saved {
        return render_form(form, vec![]);
    }
    // 저장했으면 사용량과 예산 배너를 새 값으로 다시 그린다(Next `refresh()`와 같다)
    let now = state.now();
    let refreshed = state.db.with(|conn| month_usage_report(conn, now));
    let usage = match refreshed {
        Ok(report) => usage_view(&report, true),
        Err(reason) => return db_error(&reason),
    };
    let banner = budget_banner(&state);
    respond_form(form, usage, banner)
}

fn render_form(form: FormView, oob: Vec<Result<String, askama::Error>>) -> Response {
    respond(FormTemplate { form }.render(), oob)
}

fn respond_form(form: FormView, usage: UsageView, banner: Option<BannerView>) -> Response {
    render_form(
        form,
        vec![
            UsageTemplate { usage }.render(),
            BudgetOobTemplate { banner }.render(),
        ],
    )
}

/// 저장한 깊이와 다르게 만든 종목을 지금 다시 만든다(2026-10-21 결정)
pub async fn rebuild(State(state): State<Arc<WebState>>) -> Response {
    let result = match state.worker_api() {
        Some(api) => api.rebuild_keywords().await,
        None => unreachable_result(),
    };
    let message = match result {
        WorkerResult::Ok { data, .. } => {
            let count = data["count"].as_u64().unwrap_or(0);
            if count > 0 {
                format!(
                    "{count}개 종목의 키워드를 다시 만들기 시작했습니다. 종목 화면에서 진행을 볼 수 있습니다."
                )
            } else {
                "다시 만들 종목이 없습니다.".to_string()
            }
        }
        WorkerResult::Err { error, body, .. } => describe_stock_error(&error, &body),
    };
    match state.db.with(|conn| rebuild_view(conn, Some(message))) {
        Ok(rebuild) => render(&RebuildTemplate { rebuild }),
        Err(reason) => db_error(&reason),
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
