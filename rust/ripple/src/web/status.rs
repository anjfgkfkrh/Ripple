//! 상태 화면(`app/(app)/status/*`): 시스템 상태, 기사 수집, 시장 토픽.
use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use rusqlite::Connection;
use serde_json::Value;

use crate::server::collect::read::{
    CollectionRun, RunEvaluation, RunSource, collection_totals, group_failures, latest_run,
};
use crate::server::db::schema::RunStatus;
use crate::server::evaluate::read::evaluation_totals;
use crate::server::schedule::plan::{
    CollectionPlan, KeywordPlan, RegenerateEvery, collection_plan, keyword_plan,
};
use crate::server::settings::{
    BACKUP_LAST_AT, BACKUP_LAST_FILE, PRICES_CHECKED_AT, PRICES_ERROR, PRICES_NOTE,
    TOPICS_INTERVAL_HOURS, TOPICS_TIMES, read_list, read_num, read_str,
};
use crate::server::status::{DatabaseStatus, get_system_status};
use crate::server::stocks::messages::describe_stock_error;
use crate::server::text::{js_number, js_to_fixed};
use crate::server::time::{EpochMs, format_seoul_date_time};
use crate::server::topics::plan::{parse_topic_times, topic_plan};
use crate::server::topics::store::active_auto_topics;
use crate::server::tunnel::{
    ReadyStatus, TUNNEL_TIMEOUT_MS, TunnelStatus, fetch_ready_status, fetch_tunnel_status,
    parse_public_url,
};
use crate::server::worker_client::{WorkerHealth, WorkerResult};
use crate::web::assets::AssetUrls;
use crate::web::layout::{BannerView, budget_banner};
use crate::web::response::{html, render};
use crate::web::state::WebState;

// ---- 화면에 쓰는 값 ----

pub struct SystemView {
    pub db_ok: bool,
    pub db_text: String,
    pub worker_ok: bool,
    pub worker_text: String,
    pub tunnel_url: Option<String>,
    pub tunnel_text: String,
    /// 고정 주소 모드(`PUBLIC_URL`이 있음)
    pub tunnel_fixed: bool,
    pub backup_text: String,
    pub prices_text: String,
    pub checked_text: String,
}

pub struct EvaluationView {
    pub evaluated: i64,
    pub relevant: i64,
    pub copied: i64,
    pub failed: i64,
    pub remaining: i64,
    pub cost_text: String,
    pub note: Option<String>,
}

pub struct GroupView {
    pub name: String,
    pub fetched: i64,
    pub saved: i64,
    pub matched: i64,
    pub errors: i64,
}

pub struct RunView {
    pub running: bool,
    pub status_label: &'static str,
    pub badge: &'static str,
    pub started_text: String,
    pub seconds: Option<i64>,
    pub articles_new: i64,
    pub matches_new: i64,
    pub since_text: String,
    pub note: Option<String>,
    pub evaluation: Option<EvaluationView>,
    pub failures: Vec<String>,
    pub groups: Vec<GroupView>,
}

pub struct CollectionView {
    /// 방금 동작을 시작했으면 아직 '진행 중' 기록이 없어도 한 번 더 읽어 본다
    pub poll: bool,
    pub articles: i64,
    pub matches: i64,
    pub evaluated: i64,
    pub relevant: i64,
    pub running: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub collect_text: String,
    pub show_retry: bool,
    pub keywords_text: String,
    pub run: Option<RunView>,
}

pub struct TopicsView {
    pub poll: bool,
    pub count: usize,
    pub last_text: String,
    pub running: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub schedule_text: String,
    pub failure_text: Option<String>,
}

// ---- 계산 ----

/// 이미 지났으면 '곧 시작'(worker가 1분 안에 시작), 오늘이면 시각만, 아니면 날짜와 시각(한국 시간)
pub fn next_text(at: EpochMs, now: EpochMs) -> String {
    if at <= now {
        return "곧 시작".to_string();
    }
    let target = format_seoul_date_time(at);
    let today = format_seoul_date_time(now);
    let shown = if target[..10] == today[..10] {
        &target[11..16]
    } else {
        &target[5..16]
    };
    format!("다음 {shown}")
}

fn minute_text(ms: EpochMs) -> String {
    format_seoul_date_time(ms)[..16].to_string()
}

fn month_minute_text(ms: EpochMs) -> String {
    format_seoul_date_time(ms)[5..16].to_string()
}

fn run_status_label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "수집 중",
        RunStatus::Success => "완료",
        RunStatus::Partial => "일부 실패",
        RunStatus::Failed => "실패",
    }
}

/// 피드 결과를 출처(언론사·Finnhub)별로 묶는다. 새 기사가 많은 곳부터.
fn group_sources(sources: &[RunSource]) -> Vec<GroupView> {
    let mut groups: Vec<GroupView> = Vec::new();
    for source in sources {
        let name = source.group.as_deref().unwrap_or(&source.label);
        let index = match groups.iter().position(|g| g.name == name) {
            Some(index) => index,
            None => {
                groups.push(GroupView {
                    name: name.to_string(),
                    fetched: 0,
                    saved: 0,
                    matched: 0,
                    errors: 0,
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        group.fetched += source.fetched;
        group.saved += source.saved;
        group.matched += source.matched;
        if source.error.as_deref().is_some_and(|e| !e.is_empty()) {
            group.errors += 1;
        }
    }
    // localeCompare(…, "ko") 대신 유니코드 순서(설계 8장)
    groups.sort_by(|a, b| {
        b.saved
            .cmp(&a.saved)
            .then(b.matched.cmp(&a.matched))
            .then_with(|| a.name.cmp(&b.name))
    });
    groups
}

fn evaluation_view(e: &RunEvaluation) -> EvaluationView {
    EvaluationView {
        evaluated: e.evaluated,
        relevant: e.relevant,
        copied: e.copied,
        failed: e.failed,
        remaining: e.remaining,
        cost_text: js_to_fixed(e.cost_usd, 5),
        note: e.note.clone().filter(|n| !n.is_empty()),
    }
}

fn run_view(run: &CollectionRun) -> RunView {
    // Math.max(0, Math.round(ms / 1000))
    let seconds = run.finished_at.map(|finished| {
        (((finished - run.started_at) as f64 / 1000.0 + 0.5).floor() as i64).max(0)
    });
    let failures = group_failures(&run.sources)
        .into_iter()
        .map(|group| {
            if group.labels.len() == 1 {
                format!("{}: {}", group.labels[0], group.reason)
            } else {
                format!(
                    "{} 등 {}곳: {}",
                    group.labels[0],
                    group.labels.len(),
                    group.reason
                )
            }
        })
        .collect();
    RunView {
        running: run.status == RunStatus::Running,
        status_label: run_status_label(run.status),
        badge: match run.status {
            RunStatus::Failed => "destructive",
            RunStatus::Partial => "outline",
            _ => "secondary",
        },
        started_text: minute_text(run.started_at),
        seconds,
        articles_new: run.articles_new,
        matches_new: run.matches_new,
        since_text: minute_text(run.since),
        note: run.note.clone().filter(|n| !n.is_empty()),
        evaluation: run.evaluation.as_ref().map(evaluation_view),
        failures,
        groups: group_sources(&run.sources),
    }
}

fn collect_schedule_text(plan: &CollectionPlan, now: EpochMs) -> String {
    if plan.idle {
        "등록한 종목이 없어 쉬는 중".to_string()
    } else if plan.running {
        format!("{}분마다 · 지금 수집 중", plan.interval_minutes)
    } else {
        format!(
            "{}분마다 · {}",
            plan.interval_minutes,
            next_text(plan.next_at, now)
        )
    }
}

fn keywords_schedule_text(plan: &KeywordPlan, now: EpochMs) -> String {
    let every = if plan.every == RegenerateEvery::Weekly {
        "매일 06:00 확인(주 1회 점검)"
    } else {
        "매일 06:00 확인(매일 점검)"
    };
    if plan.idle {
        every.to_string()
    } else {
        format!("{every} · {}", next_text(plan.next_at, now))
    }
}

pub fn collection_view(
    conn: &Connection,
    now: EpochMs,
    error: Option<String>,
    notice: Option<String>,
) -> rusqlite::Result<CollectionView> {
    let run = latest_run(conn)?;
    let totals = collection_totals(conn)?;
    let evaluated = evaluation_totals(conn)?;
    let collect = collection_plan(conn, now)?;
    let keywords = keyword_plan(conn, now)?;
    Ok(CollectionView {
        poll: false,
        articles: totals.articles,
        matches: totals.matches,
        evaluated: evaluated.evaluated,
        relevant: evaluated.relevant,
        running: run.as_ref().is_some_and(|r| r.status == RunStatus::Running),
        error,
        notice,
        collect_text: collect_schedule_text(&collect, now),
        show_retry: collect.retrying && !collect.running && !collect.idle,
        keywords_text: keywords_schedule_text(&keywords, now),
        run: run.as_ref().map(run_view),
    })
}

pub fn topics_view(
    conn: &Connection,
    now: EpochMs,
    error: Option<String>,
    notice: Option<String>,
) -> rusqlite::Result<TopicsView> {
    let plan = topic_plan(conn, now)?;
    let count = active_auto_topics(conn)?.len();
    let interval = read_num(conn, &TOPICS_INTERVAL_HOURS)?;
    let times: Vec<String> = parse_topic_times(&read_list(conn, &TOPICS_TIMES)?)
        .iter()
        .map(|t| format!("{:02}:{:02}", t.hour, t.minute))
        .collect();
    let mut rule: Vec<String> = Vec::new();
    if interval > 0.0 {
        rule.push(format!("{}시간마다", js_number(interval)));
    }
    if !times.is_empty() {
        rule.push(times.join("·"));
    }
    let rule = rule.join(" + ");
    let schedule_text = if !plan.enabled {
        "꺼짐(설정에서 간격이나 시각을 넣으면 켜짐)".to_string()
    } else if plan.running {
        format!("{rule} · 지금 갱신 중")
    } else {
        format!("{rule} · {}", next_text(plan.next_at.unwrap_or(now), now))
    };
    let running = plan.running;
    Ok(TopicsView {
        poll: false,
        count,
        last_text: plan
            .last_at
            .map_or_else(|| "아직 없음".to_string(), month_minute_text),
        running,
        error,
        notice,
        schedule_text,
        failure_text: plan.failure.filter(|_| !running).map(|failure| {
            format!(
                "{} 갱신 실패: {}",
                month_minute_text(failure.at),
                failure.error
            )
        }),
    })
}

/// DB를 읽지 못했을 때 패널 대신 보여 줄 글자
fn panel_error(reason: &str) -> Response {
    html(
        StatusCode::OK,
        format!(
            "<p class=\"mt-3 text-sm text-destructive\" role=\"alert\">DB를 읽지 못했습니다: {}</p>",
            reason
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        ),
    )
}

// ---- 화면 ----

#[derive(Template)]
#[template(path = "status/page.html")]
struct StatusPageTemplate {
    assets: AssetUrls,
    banner: Option<BannerView>,
    member: bool,
    collection: CollectionView,
    topics: TopicsView,
}

#[derive(Template)]
#[template(path = "status/system.html")]
struct SystemTemplate {
    system: SystemView,
}

#[derive(Template)]
#[template(path = "status/collection.html")]
struct CollectionTemplate {
    collection: CollectionView,
}

#[derive(Template)]
#[template(path = "status/topics.html")]
struct TopicsTemplate {
    topics: TopicsView,
}

pub async fn page(State(state): State<Arc<WebState>>) -> Response {
    let now = state.now();
    let views = state
        .db
        .read(move |conn| {
            Ok((
                collection_view(conn, now, None, None)?,
                topics_view(conn, now, None, None)?,
            ))
        })
        .await;
    match views {
        Ok((collection, topics)) => render(&StatusPageTemplate {
            assets: state.assets.urls.clone(),
            banner: budget_banner(&state).await,
            member: true,
            collection,
            topics,
        }),
        Err(reason) => html(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "DB를 읽지 못했습니다: {}",
                reason
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            ),
        ),
    }
}

/// 시스템 상태 조각. 화면이 먼저 "확인 중…"을 보이고 htmx로 이것을 채운다(R10 결정 ③).
pub async fn system(State(state): State<Arc<WebState>>) -> Response {
    let tunnel_fixed = state.public_url.is_some();
    let (status, tunnel_result) = tokio::join!(
        get_system_status(&state.db, state.worker_url.as_deref(), &state.http),
        async {
            if tunnel_fixed {
                Err(fetch_ready_status(
                    state.tunnel_metrics_url.as_deref(),
                    &state.http,
                    TUNNEL_TIMEOUT_MS,
                )
                .await)
            } else {
                Ok(fetch_tunnel_status(
                    state.tunnel_metrics_url.as_deref(),
                    &state.http,
                    TUNNEL_TIMEOUT_MS,
                )
                .await)
            }
        },
    );
    let settings = state
        .db
        .read(|conn| {
            Ok((
                read_num(conn, &BACKUP_LAST_AT)?,
                read_str(conn, &BACKUP_LAST_FILE)?,
                read_num(conn, &PRICES_CHECKED_AT)?,
                read_str(conn, &PRICES_ERROR)?,
                read_str(conn, &PRICES_NOTE)?,
            ))
        })
        .await;
    let (backup_at, backup_file, prices_at, prices_error, prices_note) =
        settings.unwrap_or((0.0, String::new(), 0.0, String::new(), String::new()));

    let (db_ok, db_text) = match &status.database {
        DatabaseStatus::Ok {
            journal_mode,
            settings_count,
        } => (
            true,
            format!("journal_mode {journal_mode} · settings {settings_count}건"),
        ),
        DatabaseStatus::Err { reason } => (false, reason.clone()),
    };
    let (worker_ok, worker_text) = match &status.worker {
        WorkerHealth::Ok {
            listings: Some((kr, us)),
        } => (
            true,
            format!("응답함 · 공식 종목 목록 한국 {kr}개, 미국 {us}개"),
        ),
        WorkerHealth::Ok { listings: None } => (true, "응답함".to_string()),
        WorkerHealth::Err { reason } => (false, reason.clone()),
    };
    let (tunnel_url, tunnel_text) = match tunnel_result {
        Ok(TunnelStatus::Ready { url }) => (Some(url), String::new()),
        Ok(TunnelStatus::Pending) => (
            None,
            "주소를 받는 중입니다. 잠시 뒤 새로 고침하세요.".to_string(),
        ),
        Ok(TunnelStatus::Off) => (
            None,
            "터널을 쓰지 않습니다(이 컴퓨터에서만 접속).".to_string(),
        ),
        Ok(TunnelStatus::Error { reason }) => (None, format!("터널에 닿지 못했습니다: {reason}")),
        Err(ready) => match state.public_url.as_deref().and_then(parse_public_url) {
            None => (
                None,
                "PUBLIC_URL 형식이 다릅니다(https://호스트 꼴)".to_string(),
            ),
            Some(url) => {
                let text = match ready {
                    ReadyStatus::Connected {
                        connections: Some(n),
                    } => format!("연결됨(연결 {n}개)"),
                    ReadyStatus::Connected { connections: None } => "연결됨".to_string(),
                    ReadyStatus::Disconnected => "Cloudflare와 연결되지 않았습니다".to_string(),
                    ReadyStatus::Error { reason } => format!("터널에 닿지 못했습니다: {reason}"),
                    ReadyStatus::Unknown => {
                        "연결 상태를 알 수 없습니다(터널 메트릭 주소 없음)".to_string()
                    }
                };
                (Some(url), text)
            }
        },
    };
    let backup_text = if backup_at > 0.0 {
        format!(
            "마지막 {} · {backup_file} · 최근 7개 보관",
            month_minute_text(backup_at as i64)
        )
    } else {
        "아직 없습니다. Docker로 실행하면 매일 04:00에 backups 폴더에 남깁니다.".to_string()
    };
    let prices_head = if prices_at > 0.0 {
        let error = if prices_error.is_empty() {
            String::new()
        } else {
            format!(" · 오류: {prices_error}")
        };
        format!("마지막 {}{error}", month_minute_text(prices_at as i64))
    } else {
        "아직 받은 적이 없습니다.".to_string()
    };
    let prices_text = [prices_head, prices_note]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");

    render(&SystemTemplate {
        system: SystemView {
            db_ok,
            db_text,
            worker_ok,
            worker_text,
            tunnel_url,
            tunnel_text,
            tunnel_fixed,
            backup_text,
            prices_text,
            checked_text: format_seoul_date_time(status.checked_at),
        },
    })
}

async fn collection_panel(
    state: &WebState,
    error: Option<String>,
    notice: Option<String>,
    poll: bool,
) -> Response {
    let now = state.now();
    match state
        .db
        .read(move |conn| collection_view(conn, now, error, notice))
        .await
    {
        Ok(mut collection) => {
            collection.poll = poll;
            render(&CollectionTemplate { collection })
        }
        Err(reason) => panel_error(&reason),
    }
}

async fn topics_panel(
    state: &WebState,
    error: Option<String>,
    notice: Option<String>,
    poll: bool,
) -> Response {
    let now = state.now();
    match state
        .db
        .read(move |conn| topics_view(conn, now, error, notice))
        .await
    {
        Ok(mut topics) => {
            topics.poll = poll;
            render(&TopicsTemplate { topics })
        }
        Err(reason) => panel_error(&reason),
    }
}

pub async fn collection(State(state): State<Arc<WebState>>) -> Response {
    collection_panel(&state, None, None, false).await
}

pub async fn topics(State(state): State<Arc<WebState>>) -> Response {
    topics_panel(&state, None, None, false).await
}

pub(crate) fn unreachable_result() -> WorkerResult {
    WorkerResult::Err {
        status: 0,
        error: "worker_unreachable".to_string(),
        body: Value::Null,
    }
}

/// "지금 수집": worker에 수집을 맡기고 바로 돌아온다. 진행 상황은 패널이 스스로 다시 읽어 보여 준다.
pub async fn collect_now(State(state): State<Arc<WebState>>) -> Response {
    let result = match state.worker_api() {
        Some(api) => api.start_collection().await,
        None => unreachable_result(),
    };
    let (error, notice) = match result {
        WorkerResult::Ok { .. } => (None, None),
        WorkerResult::Err { error, body, .. } => {
            if error == "already_running" {
                (None, Some("이미 수집 중입니다.".to_string()))
            } else {
                (Some(describe_stock_error(&error, &body)), None)
            }
        }
    };
    collection_panel(&state, error, notice, true).await
}

/// 11단계 "지금 갱신": worker에 자동 토픽 갱신을 맡기고 바로 돌아온다.
pub async fn refresh_topics(State(state): State<Arc<WebState>>) -> Response {
    let result = match state.worker_api() {
        Some(api) => api.refresh_topics().await,
        None => unreachable_result(),
    };
    let (error, notice) = match result {
        WorkerResult::Ok { data, .. } => {
            if data.get("started").and_then(Value::as_bool) == Some(true) {
                (None, None)
            } else {
                (None, Some("이미 갱신 중입니다.".to_string()))
            }
        }
        WorkerResult::Err { error, body, .. } => (Some(describe_stock_error(&error, &body)), None),
    };
    topics_panel(&state, error, notice, true).await
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
