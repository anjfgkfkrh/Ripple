//! worker 내부 API(`worker/server.ts`). compose 내부 네트워크에만 열고 호스트에는 공개하지 않는다.
//! DB 쓰기와 AI 호출은 모두 여기를 거친다(SPEC D-36). /health를 빼면 공유 토큰이 필요하다.
//!
//! TS처럼 한 함수가 (메서드, 주소)를 차례로 맞춰 본다. 길이가 맞지 않거나 메서드가 다르면 모두 404다.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::Response;
use serde_json::{Map, Value, json};

use crate::server::ai::llm::models::ModelListError;
use crate::server::ai::llm::{LlmClient, LlmErrorKind};
use crate::server::auth::worker_token::is_authorized_bearer;
use crate::server::collect::backfill::link_stored_headlines;
use crate::server::collect::finnhub::FinnhubNewsClient;
use crate::server::collect::http::HttpClient;
use crate::server::collect::robots::RobotsCheck;
use crate::server::collect::runner::CollectionRunner;
use crate::server::db::client::Db;
use crate::server::db::schema::{Market, RunTrigger};
use crate::server::keywords::read::stocks_with_other_depth;
use crate::server::listings::sync::listing_count;
use crate::server::log::{log, log_error};
use crate::server::schedule::scheduler::{BoxFuture, KeywordEnqueuer};
use crate::server::settings::{LLM_MODEL, read_str};
use crate::server::settings_form::{
    parse_settings_form, settings_errors_json, write_settings_form,
};
use crate::server::stocks::r#match::{MAX_QUERY_LENGTH, MatchError, StockCandidate, match_stocks};
use crate::server::stocks::repository::{
    RegisterInput, RegisterResult, Stock, delete_stock, is_active_stock_id, register_stock,
    remove_alias,
};
use crate::server::text::{js_len, js_trim, json_number};
use crate::server::time::{EpochMs, now_ms};
use crate::server::timeline::reads::{MAX_READ_BATCH, set_articles_read};
use crate::server::topics::run::TopicRefresher;
use crate::server::topics::store::{
    AddTopicResult, add_user_topic, delete_user_topic, market_item, set_market_enabled,
};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_BODY_BYTES: usize = 16 * 1024;

type NowFn = Arc<dyn Fn() -> EpochMs + Send + Sync>;
/// OpenRouter 목록에 이 모델 ID가 있는지(설정 저장 때, 2026-10-21 결정). 확인하지 못하면 `ModelListError`
pub type CheckModelFn =
    Arc<dyn Fn(String) -> BoxFuture<Result<bool, ModelListError>> + Send + Sync>;

/// 기사 수집을 시작하는 쪽(`CollectionRunner`, 테스트의 가짜). 정기 수집은 스케줄러가, "지금 수집"은 여기가 시작한다.
pub trait Collector: Send + Sync {
    fn start(&self, trigger: RunTrigger) -> Option<i64>;
    fn running_id(&self) -> Option<i64>;
}

impl<H, R, F, D> Collector for CollectionRunner<H, R, F, D>
where
    H: HttpClient + 'static,
    R: RobotsCheck + 'static,
    F: FinnhubNewsClient + 'static,
    D: crate::server::ai::decider::types::Decider + 'static,
{
    fn start(&self, trigger: RunTrigger) -> Option<i64> {
        CollectionRunner::start(self, trigger)
    }

    fn running_id(&self) -> Option<i64> {
        CollectionRunner::running_id(self)
    }
}

/// 자동 토픽 갱신(11단계). "지금 갱신"이 쓴다.
pub trait TopicRefreshControl: Send + Sync {
    fn running(&self) -> bool;
    /// 갱신을 시작하고 기다리지 않는다(이미 하고 있으면 그 갱신에 합류).
    fn start_refresh(&self);
}

impl<L, D> TopicRefreshControl for TopicRefresher<L, D>
where
    L: crate::server::ai::llm::LlmClient + 'static,
    D: crate::server::ai::decider::types::Decider + 'static,
{
    fn running(&self) -> bool {
        TopicRefresher::running(self)
    }

    fn start_refresh(&self) {
        let refresher = self.clone();
        tokio::spawn(async move {
            refresher.refresh().await;
        });
    }
}

pub struct WorkerDeps<L> {
    pub db: Db,
    pub api_token: String,
    pub llm: Arc<L>,
    /// 등록 직후·다시 만들기 요청 때 키워드 생성을 맡긴다(D-37).
    pub keyword_queue: Arc<dyn KeywordEnqueuer>,
    pub collector: Arc<dyn Collector>,
    pub check_model: CheckModelFn,
    pub topic_refresher: Option<Arc<dyn TopicRefreshControl>>,
    pub now: Option<NowFn>,
}

impl<L> WorkerDeps<L> {
    fn now(&self) -> EpochMs {
        self.now.as_ref().map_or_else(now_ms, |f| f())
    }
}

/// worker 내부 API 라우터. 모든 요청을 하나의 처리 함수가 받는다.
pub fn worker_router<L: LlmClient + 'static>(deps: WorkerDeps<L>) -> Router {
    Router::new()
        .fallback(handle::<L>)
        .with_state(Arc::new(deps))
}

type ApiError = Box<dyn std::error::Error + Send + Sync>;

async fn handle<L: LlmClient + 'static>(
    State(deps): State<Arc<WorkerDeps<L>>>,
    request: Request<Body>,
) -> Response {
    match route(&deps, request).await {
        Ok(response) => response,
        Err(error) => {
            log_error("worker 내부 API 오류", &error);
            send_json(500, json!({ "error": "internal" }))
        }
    }
}

fn send_json(status: u16, body: Value) -> Response {
    let mut response = Response::new(Body::from(body.to_string()));
    *response.status_mut() = StatusCode::from_u16(status).expect("올바른 상태 코드");
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::AUTHORIZATION)?.to_str().ok()
}

/// 주소 `/<앞>/<숫자>/<뒤>`에서 숫자(ASCII 숫자만)를 꺼낸다. 앞·뒤가 비어 있으면 `/<앞>/<숫자>`.
fn numeric_segment(path: &str, prefix: &str, suffix: &str) -> Option<i64> {
    let rest = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // JS `Number()`는 아주 큰 수도 받지만 그런 id는 어차피 없으므로 못 찾는 것으로 둔다.
    Some(rest.parse().unwrap_or(i64::MAX))
}

async fn route<L: LlmClient + 'static>(
    deps: &WorkerDeps<L>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.as_str();

    if method == "GET" && path == "/health" {
        return Ok(health(&deps.db));
    }

    if !is_authorized_bearer(bearer(&parts.headers), &deps.api_token) {
        return Ok(send_json(401, json!({ "error": "unauthorized" })));
    }

    if method == "POST" && path == "/stocks/match" {
        let query = match read_body(body, parse_match).await? {
            Body_::Parsed(query) => query,
            Body_::Response(response) => return Ok(response),
        };
        return match_stocks_route(deps, &query).await;
    }

    if method == "POST" && path == "/stocks" {
        let input = match read_body(body, parse_register).await? {
            Body_::Parsed(input) => input,
            Body_::Response(response) => return Ok(response),
        };
        return register_route(deps, input);
    }

    if method == "POST"
        && let Some(id) = numeric_segment(&path, "/stocks/", "/keywords")
    {
        let active = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            is_active_stock_id(&conn, id)?
        };
        if !active {
            return Ok(send_json(404, json!({ "error": "not_found" })));
        }
        deps.keyword_queue.enqueue(id);
        return Ok(send_json(202, json!({ "status": "queued" })));
    }

    // 17단계: 종목 화면에서 별칭 하나를 지운다(지운 별칭은 다시 붙지 않는다)
    if method == "POST"
        && let Some(id) = numeric_segment(&path, "/stocks/", "/aliases/remove")
    {
        let alias = match read_body(body, parse_alias).await? {
            Body_::Parsed(alias) => alias,
            Body_::Response(response) => return Ok(response),
        };
        let removed = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            if !is_active_stock_id(&conn, id)? {
                return Ok(send_json(404, json!({ "error": "not_found" })));
            }
            remove_alias(&conn, id, &alias)?
        };
        if !removed {
            return Ok(send_json(404, json!({ "error": "alias_not_found" })));
        }
        return Ok(send_json(200, json!({ "ok": true })));
    }

    if method == "POST" && path == "/collect" {
        let Some(run_id) = deps.collector.start(RunTrigger::Manual) else {
            return Ok(send_json(
                409,
                json!({ "error": "already_running", "runId": deps.collector.running_id() }),
            ));
        };
        log(&format!("수집 시작: 회차 {run_id}(직접 실행)"));
        return Ok(send_json(202, json!({ "runId": run_id })));
    }

    if method == "PUT" && path == "/settings" {
        let raw = match read_json(body).await {
            Ok(raw) => raw,
            Err(TooLarge) => return Ok(too_large()),
        };
        return settings_route(deps, &raw).await;
    }

    if method == "POST" && path == "/keywords/rebuild" {
        let ids = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            stocks_with_other_depth(&conn)?
        };
        for id in &ids {
            deps.keyword_queue.enqueue(*id);
        }
        if !ids.is_empty() {
            log(&format!(
                "키워드 다시 만들기(깊이 변경): {}개 종목",
                ids.len()
            ));
        }
        return Ok(send_json(202, json!({ "count": ids.len() })));
    }

    // 11단계: 시장(한국·미국 증시) 켜기/끄기
    let market = match path.as_str() {
        "/markets/KR" => Some(Market::Kr),
        "/markets/US" => Some(Market::Us),
        _ => None,
    };
    if method == "PUT"
        && let Some(market) = market
    {
        let enabled = match read_body(body, parse_enabled).await? {
            Body_::Parsed(enabled) => enabled,
            Body_::Response(response) => return Ok(response),
        };
        let now = deps.now();
        let (generate, created) = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            set_market_enabled(&conn, market, enabled, now)?
        };
        for id in &generate {
            deps.keyword_queue.enqueue(*id);
        }
        if !created.is_empty() {
            let mut conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            link_stored_headlines(&mut conn, &created, now - DAY_MS, now)?;
        }
        log(&format!(
            "{} {}",
            market_item(market).name,
            if enabled { "켬" } else { "끔" }
        ));
        return Ok(send_json(200, json!({ "ok": true })));
    }

    // 11단계: 사용자 주제 추가·삭제
    if method == "POST" && path == "/topics" {
        let name = match read_body(body, parse_topic_name).await? {
            Body_::Parsed(name) => name,
            Body_::Response(response) => return Ok(response),
        };
        let result = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            add_user_topic(&conn, &name, deps.now())?
        };
        return Ok(match result {
            AddTopicResult::Added { topic_id, lens_ids } => {
                for id in &lens_ids {
                    deps.keyword_queue.enqueue(*id);
                }
                log(&format!("주제 추가: {}", js_trim(&name)));
                send_json(201, json!({ "topicId": topic_id }))
            }
            AddTopicResult::Exists => send_json(409, json!({ "error": "exists" })),
            AddTopicResult::Invalid => send_json(400, json!({ "error": "invalid" })),
        });
    }

    if method == "DELETE"
        && let Some(id) = numeric_segment(&path, "/topics/", "")
    {
        let deleted = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            delete_user_topic(&conn, id, deps.now())?
        };
        if deleted {
            log(&format!("주제 삭제(숨김): id {id}"));
        }
        return Ok(deleted_response(deleted));
    }

    // 12단계: 읽은 기사 표시(카드 하나에 묶인 기사들을 한꺼번에). 누를 때마다 부르므로 로그는 남기지 않는다.
    if method == "PUT" && path == "/articles/read" {
        let (article_ids, read) = match read_body(body, parse_article_read).await? {
            Body_::Parsed(parsed) => parsed,
            Body_::Response(response) => return Ok(response),
        };
        let changed = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            set_articles_read(&conn, &article_ids, read, deps.now())?
        };
        return Ok(send_json(200, json!({ "changed": changed })));
    }

    // 11단계: 자동 토픽 지금 갱신(상태 화면). 이미 하고 있으면 그것을 기다린다.
    if method == "POST" && path == "/topics/refresh" {
        let Some(refresher) = &deps.topic_refresher else {
            return Ok(send_json(503, json!({ "error": "unavailable" })));
        };
        let running = refresher.running();
        refresher.start_refresh();
        if !running {
            log("토픽 갱신 시작(직접 실행)");
        }
        return Ok(send_json(202, json!({ "started": !running })));
    }

    if method == "DELETE"
        && let Some(id) = numeric_segment(&path, "/stocks/", "")
    {
        let deleted = {
            let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
            delete_stock(&conn, id, deps.now())?
        };
        if deleted {
            log(&format!("종목 삭제(숨김): id {id}"));
        }
        return Ok(deleted_response(deleted));
    }

    Ok(send_json(404, json!({ "error": "not_found" })))
}

fn deleted_response(deleted: bool) -> Response {
    if deleted {
        send_json(200, json!({ "ok": true }))
    } else {
        send_json(404, json!({ "error": "not_found" }))
    }
}

fn invalid_request() -> Response {
    send_json(400, json!({ "error": "invalid_request" }))
}

fn health(db: &Db) -> Response {
    // 연결이 깨졌으면(잠금이 깨짐 포함) 503
    let result = (|| -> Option<(i64, i64)> {
        let conn = db.lock().ok()?;
        conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .ok()?;
        Some((
            listing_count(&conn, Market::Kr).ok()?,
            listing_count(&conn, Market::Us).ok()?,
        ))
    })();
    match result {
        Some((kr, us)) => send_json(
            200,
            json!({ "status": "ok", "db": "ok", "listings": { "KR": kr, "US": us } }),
        ),
        None => send_json(503, json!({ "status": "error", "db": "error" })),
    }
}

// ---- 본문 읽기와 검사 ----

/// 본문 읽기 결과: 검사를 통과한 값이거나, 이미 만든 오류 응답(413·400)
enum Body_<T> {
    Parsed(T),
    Response(Response),
}

/// 본문이 16KB를 넘었다(413)
struct TooLarge;

fn too_large() -> Response {
    send_json(413, json!({ "error": "body_too_large" }))
}

/// JSON 본문을 읽는다. 너무 크면 413 응답, 형식이 틀리면 `null`을 돌려준다(JS `JSON.parse` 실패와 같은 처리).
async fn read_json(body: Body) -> Result<Value, TooLarge> {
    let bytes = axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| TooLarge)?;
    Ok(serde_json::from_str(&String::from_utf8_lossy(&bytes)).unwrap_or(Value::Null))
}

/// JSON 본문을 읽고 검사한다. 실패하면 응답(413·400)을 돌려준다.
async fn read_body<T>(
    body: Body,
    parse: impl FnOnce(&Value) -> Option<T>,
) -> Result<Body_<T>, ApiError> {
    let json = match read_json(body).await {
        Ok(json) => json,
        Err(TooLarge) => return Ok(Body_::Response(too_large())),
    };
    Ok(match parse(&json) {
        Some(value) => Body_::Parsed(value),
        None => Body_::Response(invalid_request()),
    })
}

fn object_of(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

/// zod `z.string().trim().min(1).max(max)`: 앞뒤 공백을 지운 뒤 1~max자(UTF-16 단위), 지운 값을 쓴다.
fn trimmed_string(value: Option<&Value>, max: usize) -> Option<String> {
    let text = js_trim(value?.as_str()?);
    (1..=max).contains(&js_len(text)).then(|| text.to_string())
}

fn parse_match(value: &Value) -> Option<String> {
    trimmed_string(object_of(value)?.get("query"), MAX_QUERY_LENGTH)
}

struct RegisterBody {
    market: Market,
    code: String,
    user_input: String,
    name_ko: Option<String>,
}

fn parse_register(value: &Value) -> Option<RegisterBody> {
    let object = object_of(value)?;
    let market = match object.get("market")?.as_str()? {
        "KR" => Market::Kr,
        "US" => Market::Us,
        _ => return None,
    };
    let name_ko = match object.get("nameKo") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if js_len(text) <= 100 => Some(text.clone()),
        Some(_) => return None,
    };
    Some(RegisterBody {
        market,
        code: trimmed_string(object.get("code"), 20)?,
        user_input: trimmed_string(object.get("userInput"), MAX_QUERY_LENGTH)?,
        name_ko,
    })
}

/// 지울 별칭: 저장된 글자와 정확히 맞춰야 하므로 다듬지 않는다(비어 있거나 100자를 넘으면 거부)
fn parse_alias(value: &Value) -> Option<String> {
    let text = object_of(value)?.get("alias")?.as_str()?;
    (1..=100).contains(&js_len(text)).then(|| text.to_string())
}

fn parse_enabled(value: &Value) -> Option<bool> {
    object_of(value)?.get("enabled")?.as_bool()
}

/// `name`은 최대 100자(UTF-16 단위). 줄이거나 다듬는 것은 `add_user_topic`이 한다.
fn parse_topic_name(value: &Value) -> Option<String> {
    let text = object_of(value)?.get("name")?.as_str()?;
    (js_len(text) <= 100).then(|| text.to_string())
}

/// zod `z.number().int().positive()`: 안전한 양의 정수(`1.0`도 정수로 읽는다)
fn positive_safe_integer(value: &Value) -> Option<i64> {
    let number = value.as_f64()?;
    (number.fract() == 0.0 && number > 0.0 && number <= 9_007_199_254_740_991.0)
        .then_some(number as i64)
}

fn parse_article_read(value: &Value) -> Option<(Vec<i64>, bool)> {
    let object = object_of(value)?;
    let ids = object.get("articleIds")?.as_array()?;
    if ids.is_empty() || ids.len() > MAX_READ_BATCH {
        return None;
    }
    let ids = ids
        .iter()
        .map(positive_safe_integer)
        .collect::<Option<Vec<_>>>()?;
    Some((ids, object.get("read")?.as_bool()?))
}

// ---- 경로별 처리 ----

fn stock_json(stock: &Stock) -> Value {
    json!({
        "id": stock.id,
        "market": stock.market,
        "code": stock.code,
        "name": stock.name,
        "nameKo": stock.name_ko,
        "exchange": stock.exchange,
        "userInput": stock.user_input,
        "createdAt": iso(stock.created_at),
        "deletedAt": stock.deleted_at.map(iso),
        "keywordsStatus": stock.keywords_status,
        "keywordsError": stock.keywords_error,
        "keywordsGeneratedAt": stock.keywords_generated_at.map(iso),
        "backfillFrom": stock.backfill_from.map(iso),
        "kind": stock.kind,
        "topicId": stock.topic_id,
    })
}

/// JS `Date`를 JSON으로 쓸 때의 글자(`2026-10-21T00:00:00.000Z`)
fn iso(at: EpochMs) -> String {
    chrono::DateTime::from_timestamp_millis(at).map_or_else(String::new, |time| {
        time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    })
}

fn candidate_json(candidate: &StockCandidate) -> Value {
    json!({
        "market": candidate.market,
        "code": candidate.code,
        "name": candidate.name,
        "nameKo": candidate.name_ko,
        "exchange": candidate.exchange,
        "isEtf": candidate.is_etf,
        "alreadyRegistered": candidate.already_registered,
    })
}

async fn match_stocks_route<L: LlmClient + 'static>(
    deps: &WorkerDeps<L>,
    query: &str,
) -> Result<Response, ApiError> {
    match match_stocks(&deps.db, &*deps.llm, query, deps.now.as_deref()).await {
        Ok(candidates) => {
            log(&format!(
                "종목 찾기 \"{query}\" → 후보 {}개",
                candidates.len()
            ));
            Ok(send_json(
                200,
                json!({ "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>() }),
            ))
        }
        Err(error) => send_match_error(error),
    }
}

fn send_match_error(error: MatchError) -> Result<Response, ApiError> {
    match error {
        MatchError::Budget(error) => Ok(send_json(
            402,
            json!({
                "error": "budget_exceeded",
                "spentUsd": json_number(error.spent_usd),
                "limitUsd": json_number(error.limit_usd),
            }),
        )),
        MatchError::Listings(_) => Ok(send_json(503, json!({ "error": "listings_unavailable" }))),
        MatchError::Llm(error) => {
            log_error("종목 찾기 AI 호출 실패", &error.message);
            let (status, kind) = match error.kind {
                LlmErrorKind::NoApiKey => (503, "no_api_key"),
                LlmErrorKind::PaymentRequired => (402, "payment_required"),
                LlmErrorKind::RateLimited => (429, "rate_limited"),
                LlmErrorKind::Timeout => (504, "timeout"),
                LlmErrorKind::Upstream => (502, "upstream"),
                LlmErrorKind::InvalidOutput => (502, "invalid_output"),
            };
            Ok(send_json(status, json!({ "error": format!("llm_{kind}") })))
        }
        MatchError::Db(error) => Err(error.into()),
    }
}

fn register_route<L>(deps: &WorkerDeps<L>, input: RegisterBody) -> Result<Response, ApiError> {
    let result = {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        register_stock(
            &conn,
            &RegisterInput {
                market: input.market,
                code: &input.code,
                user_input: &input.user_input,
                name_ko: input.name_ko.as_deref(),
            },
            deps.now(),
        )?
    };
    let (status, label, stock) = match result {
        RegisterResult::NotListed => return Ok(send_json(404, json!({ "error": "not_listed" }))),
        RegisterResult::Exists(stock) => {
            return Ok(send_json(
                409,
                json!({ "error": "already_registered", "stock": stock_json(&stock) }),
            ));
        }
        RegisterResult::Created(stock) => (201, "created", stock),
        RegisterResult::Restored(stock) => (200, "restored", stock),
    };
    log(&format!(
        "종목 {}: {} ({})",
        if label == "created" {
            "등록"
        } else {
            "복원"
        },
        stock.name,
        stock.code
    ));
    // 키워드는 응답을 기다리게 하지 않고 따로 만든다(D-37).
    deps.keyword_queue.enqueue(stock.id);
    Ok(send_json(
        status,
        json!({ "status": label, "stock": stock_json(&stock) }),
    ))
}

async fn settings_route<L>(deps: &WorkerDeps<L>, raw: &Value) -> Result<Response, ApiError> {
    let form = match parse_settings_form(raw) {
        Ok(form) => form,
        Err(errors) => {
            return Ok(send_json(
                400,
                json!({ "error": "invalid_settings", "fields": settings_errors_json(&errors) }),
            ));
        }
    };
    // 모델을 바꿀 때만 OpenRouter 목록으로 확인한다(없으면 저장하지 않음).
    let current_model = {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        read_str(&conn, &LLM_MODEL)?
    };
    if form.llm_model != current_model {
        match (deps.check_model)(form.llm_model.clone()).await {
            Ok(true) => {}
            Ok(false) => return Ok(send_json(422, json!({ "error": "unknown_model" }))),
            Err(error) => {
                log_error("OpenRouter 모델 목록 확인 실패", &error.0);
                return Ok(send_json(502, json!({ "error": "model_check_failed" })));
            }
        }
    }
    let changed = {
        let conn = deps.db.lock().expect("DB 잠금이 깨졌습니다");
        write_settings_form(&conn, &form, deps.now())?
    };
    let keys: Vec<&str> = changed.iter().map(|field| field.key()).collect();
    if !keys.is_empty() {
        log(&format!("설정 저장: {}", keys.join(", ")));
    }
    Ok(send_json(200, json!({ "ok": true, "changed": keys })))
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
