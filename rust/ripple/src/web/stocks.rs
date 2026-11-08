//! 종목 화면(`app/(app)/stocks/*`, 14단계 R12): 종목 찾기·등록·삭제, 키워드 다시 만들기, 시장·주제.
//! Next 서버 액션과 같은 동작이다. 바뀐 조각만 다시 그리고(`hx-swap-oob`), Next의 `refresh()`가 하던
//! 목록 다시 그리기는 같은 응답에 함께 보낸다. 만드는 중인 목록은 3초마다 그 조각만 다시 읽는다(R12 결정 ②).
use std::collections::HashMap;
use std::sync::Arc;

use crate::server::auth::session::Access;
use askama::Template;
use axum::Extension;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::Response;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::db::schema::{KeywordKind, KeywordStatus, Market};
use crate::server::keywords::read::{Keyword, list_keywords_by_stock};
use crate::server::log::log_error;
use crate::server::stocks::r#match::MAX_QUERY_LENGTH;
use crate::server::stocks::messages::describe_stock_error;
use crate::server::stocks::repository::{Stock, list_active_stocks};
use crate::server::text::{js_slice, js_to_fixed, js_trim};
use crate::server::time::format_seoul_date_time;
use crate::server::topics::read::{list_market_items, list_user_topic_items};
use crate::server::worker_client::WorkerResult;
use crate::web::assets::AssetUrls;
use crate::web::layout::{BannerView, budget_banner_for};
use crate::web::response::{html, render};
use crate::web::state::WebState;
use crate::web::status::unreachable_result;

/// 화면이 보내는 입력(`name=value` 쌍, 같은 이름은 첫 값을 쓴다 — `formData.get`과 같다)
type Fields = Vec<(String, String)>;

fn field(fields: &[(String, String)], name: &str) -> String {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map_or_else(String::new, |(_, value)| value.clone())
}

/// `Number(x)`가 정수이고 0보다 큰 값만 받는다(Next 서버 액션의 검사와 같다)
fn parse_id(text: &str) -> Option<i64> {
    text.parse::<i64>().ok().filter(|id| *id > 0)
}

// ---- 화면에 쓰는 값 ----

/// 키워드 트리를 펼친 순서의 한 조각. 들여쓰기(`div`·`ul`)는 시작과 끝 조각으로 나눈다.
pub enum TreeEvent {
    Node(TreeNodeView),
    BranchStart,
    BranchEnd,
    NodeEnd,
}

pub struct TreeNodeView {
    pub text_ko: String,
    pub text_en: Option<String>,
    pub reason: String,
    pub path: String,
    /// 맨 위 키워드가 토픽 갱신이 붙인 가지(토픽 배지)
    pub topic: bool,
    /// 맨 위 키워드가 시사 가지(시사 배지, 토픽 가지가 아닌 것만)
    pub current: bool,
    pub scores: Option<ScoreView>,
}

pub struct ScoreView {
    pub relevance: String,
    pub magnitude: String,
}

/// 종목 한 줄 아래의 키워드 영역(`StockKeywords`)
pub struct KeywordsView {
    pub stock_id: i64,
    pub name: String,
    pub making: bool,
    pub failed: bool,
    pub failed_text: String,
    pub count: usize,
    pub depth: i64,
    pub generated_text: Option<String>,
    pub ready: bool,
    pub events: Vec<TreeEvent>,
    /// 다시 만들기가 실패했을 때 그 단추 아래 보일 글
    pub regen_error: Option<String>,
    /// 키워드 다시 만들기 단추의 이름(접근성): "{이름} 키워드 다시 만들기"
    pub regen_aria: String,
}

/// 확인창(`AlertDialog`) 하나: 삭제 단추와 그 안의 확인 양식
pub struct DialogView {
    pub id: String,
    pub error_id: String,
    pub item_id: i64,
    pub topic: bool,
    /// 삭제 단추의 이름(접근성): "{이름} 삭제"
    pub trigger_aria: String,
    pub title: String,
    pub description: String,
}

pub struct StockView {
    pub id: i64,
    pub name: String,
    pub us: bool,
    pub market_label: &'static str,
    pub exchange: String,
    pub code: String,
    pub english_name: String,
    pub keywords: KeywordsView,
    pub dialog: DialogView,
}

pub struct LensView {
    pub exchange: String,
    /// 켜진 증시가 둘 이상이면 증시 이름을 붙인다
    pub multi: bool,
    pub keywords: KeywordsView,
}

pub struct TopicView {
    pub name: String,
    pub dialog: DialogView,
    pub lenses: Vec<LensView>,
}

pub struct MarketRowView {
    pub market: &'static str,
    pub name: &'static str,
    pub enabled: bool,
    /// 누르면 바뀔 값("true"·"false")
    pub next: &'static str,
    pub toggle_label: &'static str,
    pub toggle_aria: String,
    pub detail: String,
    pub keywords: Option<KeywordsView>,
    pub error: Option<String>,
}

pub struct RegisterFormView {
    pub market: String,
    pub code: String,
    pub name_ko: String,
    pub name: String,
    pub user_input: String,
    pub label: String,
    pub key: String,
}

pub struct CandidateView {
    pub name: String,
    pub market_label: &'static str,
    pub us: bool,
    pub exchange: String,
    pub code: String,
    pub english_name: String,
    pub is_etf: bool,
    pub already: bool,
    pub form: RegisterFormView,
}

pub struct SearchView {
    pub query: String,
    pub error: Option<String>,
    pub candidates: Vec<CandidateView>,
    pub searched: bool,
}

impl SearchView {
    fn idle() -> Self {
        Self {
            query: String::new(),
            error: None,
            candidates: Vec::new(),
            searched: false,
        }
    }
}

pub struct TopicFormView {
    pub value: String,
    pub error: Option<String>,
}

impl TopicFormView {
    fn empty() -> Self {
        Self {
            value: String::new(),
            error: None,
        }
    }
}

pub struct StockListView {
    pub stocks: Vec<StockView>,
    pub poll: bool,
    pub oob: bool,
}

pub struct MarketListView {
    pub markets: Vec<MarketRowView>,
    pub poll: bool,
    pub oob: bool,
}

pub struct TopicListView {
    pub topics: Vec<TopicView>,
    pub poll: bool,
    pub oob: bool,
}

// ---- 계산 ----

fn is_making(status: KeywordStatus) -> bool {
    matches!(status, KeywordStatus::Pending | KeywordStatus::Generating)
}

fn market_label(market: Market) -> &'static str {
    match market {
        Market::Kr => "한국",
        Market::Us => "미국",
    }
}

fn display_name(stock: &Stock) -> String {
    stock.name_ko.clone().unwrap_or_else(|| stock.name.clone())
}

/// 키워드 트리: 부모가 없으면 맨 위, 있으면 부모 아래(목록 순서 그대로, TS `toTree`와 같다)
fn keyword_tree(keywords: &[Keyword]) -> Vec<TreeEvent> {
    let position: HashMap<i64, usize> = keywords
        .iter()
        .enumerate()
        .map(|(index, keyword)| (keyword.id, index))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); keywords.len()];
    let mut roots = Vec::new();
    for (index, keyword) in keywords.iter().enumerate() {
        match keyword
            .parent_id
            .and_then(|parent| position.get(&parent).copied())
        {
            Some(parent) => children[parent].push(index),
            None => roots.push(index),
        }
    }
    let mut events = Vec::new();
    for root in roots {
        push_keyword(keywords, &children, root, &mut events);
    }
    events
}

fn push_keyword(
    keywords: &[Keyword],
    children: &[Vec<usize>],
    index: usize,
    events: &mut Vec<TreeEvent>,
) {
    events.push(TreeEvent::Node(tree_node(&keywords[index])));
    if !children[index].is_empty() {
        events.push(TreeEvent::BranchStart);
        for &child in &children[index] {
            push_keyword(keywords, children, child, events);
        }
        events.push(TreeEvent::BranchEnd);
    }
    events.push(TreeEvent::NodeEnd);
}

fn tree_node(keyword: &Keyword) -> TreeNodeView {
    let top = keyword.parent_id.is_none();
    TreeNodeView {
        text_ko: keyword.text_ko.clone(),
        text_en: keyword.text_en.clone().filter(|text| !text.is_empty()),
        reason: keyword.reason.clone(),
        path: keyword.path.clone(),
        topic: top && keyword.topic_id.is_some(),
        current: top && keyword.topic_id.is_none() && keyword.kind == KeywordKind::Current,
        scores: match (keyword.relevance, keyword.magnitude) {
            (Some(relevance), Some(magnitude)) => Some(ScoreView {
                relevance: js_to_fixed(relevance, 1),
                magnitude: js_to_fixed(magnitude, 1),
            }),
            _ => None,
        },
    }
}

fn keywords_view(stock: &Stock, keywords: &[Keyword]) -> KeywordsView {
    let count = keywords.len();
    let depth = keywords.iter().map(|k| k.level).max().unwrap_or(0).max(0);
    let lead = if count > 0 {
        "새 키워드를 만들지 못해 이전 키워드를 그대로 둡니다."
    } else {
        "키워드를 만들지 못했습니다."
    };
    KeywordsView {
        stock_id: stock.id,
        name: display_name(stock),
        making: is_making(stock.keywords_status),
        failed: stock.keywords_status == KeywordStatus::Failed,
        failed_text: format!("{lead} {}", stock.keywords_error.as_deref().unwrap_or("")),
        count,
        depth,
        generated_text: stock
            .keywords_generated_at
            .map(|at| format_seoul_date_time(at)[..16].to_string()),
        ready: stock.keywords_status == KeywordStatus::Ready,
        events: if count > 0 {
            keyword_tree(keywords)
        } else {
            Vec::new()
        },
        regen_error: None,
        regen_aria: format!("{} 키워드 다시 만들기", display_name(stock)),
    }
}

/// 종목이 없을 때(잘못된 입력)의 자리표시. 화면에는 이름 없는 빈 키워드 영역이 된다.
fn blank_keywords(stock_id: i64) -> KeywordsView {
    KeywordsView {
        stock_id,
        name: String::new(),
        making: false,
        failed: false,
        failed_text: String::new(),
        count: 0,
        depth: 0,
        generated_text: None,
        ready: false,
        events: Vec::new(),
        regen_error: None,
        regen_aria: String::new(),
    }
}

fn stock_dialog(stock_id: i64, name: &str) -> DialogView {
    DialogView {
        id: format!("delete-stock-{stock_id}"),
        error_id: format!("delete-stock-error-{stock_id}"),
        item_id: stock_id,
        topic: false,
        trigger_aria: format!("{name} 삭제"),
        title: format!("{name} 종목을 삭제할까요?"),
        description:
            "수집 대상에서 빠집니다. 이미 모은 기사와 판정은 보관되고, 다시 등록하면 되살아납니다."
                .to_string(),
    }
}

fn topic_dialog(topic_id: i64, name: &str) -> DialogView {
    DialogView {
        id: format!("delete-topic-{topic_id}"),
        error_id: format!("delete-topic-error-{topic_id}"),
        item_id: topic_id,
        topic: true,
        trigger_aria: format!("{name} 삭제"),
        title: format!("{name} 주제를 삭제할까요?"),
        description: "이 주제로는 기사를 더 모으지 않습니다. 이미 모은 기사와 판정은 보관됩니다."
            .to_string(),
    }
}

/// 확인창의 양식만 다시 그릴 때(글자는 쓰지 않는다)
fn form_only_dialog(item_id: i64, topic: bool) -> DialogView {
    if topic {
        topic_dialog(item_id, "")
    } else {
        stock_dialog(item_id, "")
    }
}

/// 한 번 읽은 화면 자료(종목·시장·주제와 그 키워드)
pub struct StocksData {
    stocks: Vec<StockView>,
    markets: Vec<MarketRowView>,
    topics: Vec<TopicView>,
}

/// DB에서 화면 자료를 한꺼번에 읽는다(Next 페이지의 `StockList`·`MarketTopicSection`과 같은 자료)
fn load(conn: &Connection) -> rusqlite::Result<StocksData> {
    let stocks = list_active_stocks(conn)?;
    let markets = list_market_items(conn)?;
    let topics = list_user_topic_items(conn)?;
    let mut ids: Vec<i64> = stocks.iter().map(|stock| stock.id).collect();
    for market in &markets {
        if let Some(stock) = &market.stock {
            ids.push(stock.id);
        }
    }
    for topic in &topics {
        ids.extend(topic.lenses.iter().map(|lens| lens.id));
    }
    let grouped = list_keywords_by_stock(conn, &ids)?;
    let keywords_of = |id: i64| grouped.get(&id).map_or(&[][..], Vec::as_slice);
    let enabled = markets
        .iter()
        .filter(|market| market.stock.is_some())
        .count();

    let stock_views = stocks
        .iter()
        .map(|stock| StockView {
            id: stock.id,
            name: display_name(stock),
            us: stock.market == Market::Us,
            market_label: market_label(stock.market),
            exchange: stock.exchange.clone(),
            code: stock.code.clone(),
            english_name: stock.name.clone(),
            keywords: keywords_view(stock, keywords_of(stock.id)),
            dialog: stock_dialog(stock.id, &display_name(stock)),
        })
        .collect();

    let market_views = markets
        .iter()
        .map(|item| {
            let enabled_now = item.stock.is_some();
            MarketRowView {
                market: item.market.as_str(),
                name: item.name,
                enabled: enabled_now,
                next: if enabled_now { "false" } else { "true" },
                toggle_label: if enabled_now { "끄기" } else { "켜기" },
                toggle_aria: format!(
                    "{} {}",
                    item.name,
                    if enabled_now { "끄기" } else { "켜기" }
                ),
                detail: match &item.stock {
                    Some(stock) => format!(
                        "{} · 시장 전체에 영향을 주는 기사를 모읍니다",
                        stock.exchange
                    ),
                    None => "꺼짐".to_string(),
                },
                keywords: item
                    .stock
                    .as_ref()
                    .map(|stock| keywords_view(stock, keywords_of(stock.id))),
                error: None,
            }
        })
        .collect();

    let topic_views = topics
        .iter()
        .map(|user| TopicView {
            name: user.topic.name.clone(),
            dialog: topic_dialog(user.topic.id, &user.topic.name),
            lenses: user
                .lenses
                .iter()
                .map(|lens| LensView {
                    exchange: lens.exchange.clone(),
                    multi: enabled > 1,
                    keywords: keywords_view(lens, keywords_of(lens.id)),
                })
                .collect(),
        })
        .collect();

    Ok(StocksData {
        stocks: stock_views,
        markets: market_views,
        topics: topic_views,
    })
}

async fn reload(state: &WebState) -> Result<StocksData, String> {
    state.db.read(load).await
}

fn stock_list_view(data: &StocksData, oob: bool) -> StockListView {
    StockListView {
        stocks: data.stocks.iter().map(clone_stock).collect(),
        poll: data.stocks.iter().any(|stock| stock.keywords.making),
        oob,
    }
}

fn market_list_view(data: &StocksData, oob: bool) -> MarketListView {
    MarketListView {
        markets: data.markets.iter().map(clone_market).collect(),
        poll: data
            .markets
            .iter()
            .any(|market| market.keywords.as_ref().is_some_and(|k| k.making)),
        oob,
    }
}

fn topic_list_view(data: &StocksData, oob: bool) -> TopicListView {
    TopicListView {
        topics: data.topics.iter().map(clone_topic).collect(),
        poll: data
            .topics
            .iter()
            .flat_map(|topic| topic.lenses.iter())
            .any(|lens| lens.keywords.making),
        oob,
    }
}

// 화면 값은 한 번 읽어 여러 조각에 나눠 쓰므로 복사한다(작은 목록이라 비용은 무시할 만하다).
fn clone_keywords(keywords: &KeywordsView) -> KeywordsView {
    KeywordsView {
        stock_id: keywords.stock_id,
        name: keywords.name.clone(),
        making: keywords.making,
        failed: keywords.failed,
        failed_text: keywords.failed_text.clone(),
        count: keywords.count,
        depth: keywords.depth,
        generated_text: keywords.generated_text.clone(),
        ready: keywords.ready,
        events: keywords.events.iter().map(clone_event).collect(),
        regen_error: keywords.regen_error.clone(),
        regen_aria: keywords.regen_aria.clone(),
    }
}

fn clone_event(event: &TreeEvent) -> TreeEvent {
    match event {
        TreeEvent::Node(node) => TreeEvent::Node(TreeNodeView {
            text_ko: node.text_ko.clone(),
            text_en: node.text_en.clone(),
            reason: node.reason.clone(),
            path: node.path.clone(),
            topic: node.topic,
            current: node.current,
            scores: node.scores.as_ref().map(|score| ScoreView {
                relevance: score.relevance.clone(),
                magnitude: score.magnitude.clone(),
            }),
        }),
        TreeEvent::BranchStart => TreeEvent::BranchStart,
        TreeEvent::BranchEnd => TreeEvent::BranchEnd,
        TreeEvent::NodeEnd => TreeEvent::NodeEnd,
    }
}

fn clone_dialog(dialog: &DialogView) -> DialogView {
    DialogView {
        id: dialog.id.clone(),
        error_id: dialog.error_id.clone(),
        item_id: dialog.item_id,
        topic: dialog.topic,
        trigger_aria: dialog.trigger_aria.clone(),
        title: dialog.title.clone(),
        description: dialog.description.clone(),
    }
}

fn clone_stock(stock: &StockView) -> StockView {
    StockView {
        id: stock.id,
        name: stock.name.clone(),
        us: stock.us,
        market_label: stock.market_label,
        exchange: stock.exchange.clone(),
        code: stock.code.clone(),
        english_name: stock.english_name.clone(),
        keywords: clone_keywords(&stock.keywords),
        dialog: clone_dialog(&stock.dialog),
    }
}

fn clone_market(market: &MarketRowView) -> MarketRowView {
    MarketRowView {
        market: market.market,
        name: market.name,
        enabled: market.enabled,
        next: market.next,
        toggle_label: market.toggle_label,
        toggle_aria: market.toggle_aria.clone(),
        detail: market.detail.clone(),
        keywords: market.keywords.as_ref().map(clone_keywords),
        error: market.error.clone(),
    }
}

fn clone_topic(topic: &TopicView) -> TopicView {
    TopicView {
        name: topic.name.clone(),
        dialog: clone_dialog(&topic.dialog),
        lenses: topic
            .lenses
            .iter()
            .map(|lens| LensView {
                exchange: lens.exchange.clone(),
                multi: lens.multi,
                keywords: clone_keywords(&lens.keywords),
            })
            .collect(),
    }
}

/// 검색 결과 후보(Next `CandidateItem`에 넘기던 값). 등록 양식의 숨은 값도 여기서 정한다.
fn candidate_view(candidate: &CandidateJson, query: &str) -> Option<CandidateView> {
    let market: Market = candidate.market.parse().ok()?;
    let name = candidate
        .name_ko
        .clone()
        .unwrap_or_else(|| candidate.name.clone());
    let name_ko = candidate.name_ko.clone().unwrap_or_default();
    Some(CandidateView {
        market_label: market_label(market),
        us: market == Market::Us,
        exchange: candidate.exchange.clone(),
        code: candidate.code.clone(),
        english_name: candidate.name.clone(),
        is_etf: candidate.is_etf,
        already: candidate.already_registered,
        form: register_form(
            market.as_str(),
            &candidate.code,
            &name_ko,
            &candidate.name,
            query,
        ),
        name,
    })
}

fn register_form(
    market: &str,
    code: &str,
    name_ko: &str,
    name: &str,
    user_input: &str,
) -> RegisterFormView {
    let label = if name_ko.is_empty() { name } else { name_ko };
    RegisterFormView {
        market: market.to_string(),
        code: code.to_string(),
        name_ko: name_ko.to_string(),
        name: name.to_string(),
        user_input: user_input.to_string(),
        label: format!("{label} 등록"),
        key: format!("cand-error-{market}-{code}"),
    }
}

/// worker `/stocks/match`가 주는 후보(camelCase, `candidateJson`과 같다)
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidateJson {
    market: String,
    code: String,
    name: String,
    name_ko: Option<String>,
    exchange: String,
    is_etf: bool,
    already_registered: bool,
}

// ---- 응답 조립 ----

/// 본문 조각 하나와 같은 응답에 실을 OOB 조각들을 이어 붙인다. 그릴 수 없으면 500.
pub(crate) fn respond(
    main: Result<String, askama::Error>,
    oob: Vec<Result<String, askama::Error>>,
) -> Response {
    let mut body = String::new();
    for part in std::iter::once(main).chain(oob) {
        match part {
            Ok(text) => body.push_str(&text),
            Err(error) => {
                log_error("화면을 그리지 못했습니다", &error);
                return html(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "화면을 그리지 못했습니다.".to_string(),
                );
            }
        }
    }
    html(StatusCode::OK, body)
}

/// DB를 읽지 못했을 때 보여 줄 글자(`status.rs`의 `panel_error`와 같은 방식)
pub(crate) fn db_error(reason: &str) -> Response {
    html(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!(
            "DB를 읽지 못했습니다: {}",
            reason
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        ),
    )
}

/// worker 결과를 "처리하지 못함" 글로 바꾸거나, 성공이면 `()`
fn outcome(result: WorkerResult) -> Result<(), String> {
    match result {
        WorkerResult::Ok { .. } => Ok(()),
        WorkerResult::Err { error, body, .. } => Err(describe_stock_error(&error, &body)),
    }
}

// ---- 템플릿 ----

#[derive(Template)]
#[template(path = "stocks/page.html")]
struct StocksPageTemplate {
    assets: AssetUrls,
    banner: Option<BannerView>,
    member: bool,
    search: SearchView,
    stock_list: StockListView,
    market_list: MarketListView,
    topic_form: TopicFormView,
    topic_list: TopicListView,
}

#[derive(Template)]
#[template(path = "stocks/search.html")]
struct SearchTemplate {
    search: SearchView,
}

#[derive(Template)]
#[template(path = "stocks/list.html")]
struct StockListTemplate {
    stock_list: StockListView,
}

#[derive(Template)]
#[template(path = "stocks/markets.html")]
struct MarketListTemplate {
    market_list: MarketListView,
}

#[derive(Template)]
#[template(path = "stocks/topic_list.html")]
struct TopicListTemplate {
    topic_list: TopicListView,
}

#[derive(Template)]
#[template(path = "stocks/topic_form.html")]
struct TopicFormTemplate {
    topic_form: TopicFormView,
}

#[derive(Template)]
#[template(path = "stocks/register_result.html")]
struct RegisterResultTemplate {
    form: RegisterFormView,
    status: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "stocks/candidate_error.html")]
struct CandidateErrorTemplate {
    key: String,
    message: Option<String>,
}

#[derive(Template)]
#[template(path = "stocks/delete_form.html")]
struct DeleteFormTemplate {
    dialog: DialogView,
}

#[derive(Template)]
#[template(path = "stocks/dialog_error.html")]
struct DialogErrorTemplate {
    id: String,
    message: Option<String>,
}

#[derive(Template)]
#[template(path = "stocks/regen_form.html")]
struct RegenFormTemplate {
    keywords: KeywordsView,
}

#[derive(Template)]
#[template(path = "stocks/market_form.html")]
struct MarketFormTemplate {
    market: MarketRowView,
}

// ---- 화면 ----

/// `/stocks`: 종목 화면 전체
pub async fn page(
    State(state): State<Arc<WebState>>,
    Extension(access): Extension<Access>,
) -> Response {
    let data = match reload(&state).await {
        Ok(data) => data,
        Err(reason) => return db_error(&reason),
    };
    render(&StocksPageTemplate {
        assets: state.assets.urls.clone(),
        banner: budget_banner_for(&state, &access).await,
        member: access.is_member(),
        search: SearchView::idle(),
        stock_list: stock_list_view(&data, false),
        market_list: market_list_view(&data, false),
        topic_form: TopicFormView::empty(),
        topic_list: topic_list_view(&data, false),
    })
}

/// 만드는 중인 종목 목록을 3초마다 다시 읽는 조각(`AutoRefresh`의 자리)
pub async fn list(State(state): State<Arc<WebState>>) -> Response {
    match reload(&state).await {
        Ok(data) => render(&StockListTemplate {
            stock_list: stock_list_view(&data, false),
        }),
        Err(reason) => db_error(&reason),
    }
}

/// 시장 목록 조각(만드는 중일 때만 3초마다 다시 읽는다)
pub async fn markets(State(state): State<Arc<WebState>>) -> Response {
    match reload(&state).await {
        Ok(data) => render(&MarketListTemplate {
            market_list: market_list_view(&data, false),
        }),
        Err(reason) => db_error(&reason),
    }
}

/// 주제 목록 조각(만드는 중일 때만 3초마다 다시 읽는다)
pub async fn topic_list(State(state): State<Arc<WebState>>) -> Response {
    match reload(&state).await {
        Ok(data) => render(&TopicListTemplate {
            topic_list: topic_list_view(&data, false),
        }),
        Err(reason) => db_error(&reason),
    }
}

/// 입력한 이름으로 후보를 찾는다(`matchStocksAction`). 검색 영역 전체를 바꾼다.
pub async fn match_stocks(
    State(state): State<Arc<WebState>>,
    Form(fields): Form<Fields>,
) -> Response {
    let query = js_slice(js_trim(&field(&fields, "query")), 0, MAX_QUERY_LENGTH);
    let mut search = SearchView {
        query: query.clone(),
        error: None,
        candidates: Vec::new(),
        searched: true,
    };
    if query.is_empty() {
        search.error = Some("종목 이름이나 티커를 입력하세요.".to_string());
    } else {
        let result = match state.worker_api() {
            Some(api) => api.match_stocks(&query).await,
            None => unreachable_result(),
        };
        match result {
            WorkerResult::Ok { data, .. } => {
                match serde_json::from_value::<Vec<CandidateJson>>(data["candidates"].clone()) {
                    Ok(list) => {
                        search.candidates = list
                            .iter()
                            .filter_map(|candidate| candidate_view(candidate, &query))
                            .collect();
                    }
                    Err(_) => search.error = Some(describe_stock_error("unknown", &Value::Null)),
                }
            }
            WorkerResult::Err { error, body, .. } => {
                search.error = Some(describe_stock_error(&error, &body));
            }
        }
    }
    render(&SearchTemplate { search })
}

/// 후보 하나를 등록한다(`registerStockAction`). 후보 줄의 단추 자리만 바꾼다.
pub async fn register(State(state): State<Arc<WebState>>, Form(fields): Form<Fields>) -> Response {
    let form = register_form(
        &field(&fields, "market"),
        &field(&fields, "code"),
        &field(&fields, "nameKo"),
        &field(&fields, "name"),
        &field(&fields, "userInput"),
    );
    let Ok(market) = form.market.parse::<Market>() else {
        return register_failure(form, describe_stock_error("invalid_request", &Value::Null));
    };
    let name_ko = if form.name_ko.is_empty() {
        Value::Null
    } else {
        json!(form.name_ko)
    };
    let input = json!({
        "market": market.as_str(),
        "code": form.code,
        "userInput": form.user_input,
        "nameKo": name_ko,
    });
    let result = match state.worker_api() {
        Some(api) => api.register_stock(input).await,
        None => unreachable_result(),
    };
    match result {
        WorkerResult::Ok { .. } => {
            let data = match reload(&state).await {
                Ok(data) => data,
                Err(reason) => return db_error(&reason),
            };
            let key = form.key.clone();
            respond(
                RegisterResultTemplate {
                    form,
                    status: Some("등록함"),
                }
                .render(),
                vec![
                    CandidateErrorTemplate { key, message: None }.render(),
                    StockListTemplate {
                        stock_list: stock_list_view(&data, true),
                    }
                    .render(),
                ],
            )
        }
        WorkerResult::Err { error, .. } if error == "already_registered" => {
            let key = form.key.clone();
            respond(
                RegisterResultTemplate {
                    form,
                    status: Some("이미 등록됨"),
                }
                .render(),
                vec![CandidateErrorTemplate { key, message: None }.render()],
            )
        }
        WorkerResult::Err { error, body, .. } => {
            register_failure(form, describe_stock_error(&error, &body))
        }
    }
}

fn register_failure(form: RegisterFormView, message: String) -> Response {
    let key = form.key.clone();
    respond(
        RegisterResultTemplate { form, status: None }.render(),
        vec![
            CandidateErrorTemplate {
                key,
                message: Some(message),
            }
            .render(),
        ],
    )
}

/// 종목 삭제(확인창에서). 지우면 목록을 다시 그린다.
pub async fn delete(State(state): State<Arc<WebState>>, Form(fields): Form<Fields>) -> Response {
    delete_item(&state, &fields, false).await
}

/// 주제 삭제(확인창에서, 11단계)
pub async fn delete_topic(
    State(state): State<Arc<WebState>>,
    Form(fields): Form<Fields>,
) -> Response {
    delete_item(&state, &fields, true).await
}

async fn delete_item(state: &WebState, fields: &Fields, topic: bool) -> Response {
    let id = parse_id(&field(fields, "id"));
    let result = match id {
        None => Err(describe_stock_error("invalid_request", &Value::Null)),
        Some(id) => {
            let result = match state.worker_api() {
                Some(api) if topic => api.delete_topic(id).await,
                Some(api) => api.delete_stock(id).await,
                None => unreachable_result(),
            };
            match result {
                // 이미 지워졌으면 목록만 새로 그리면 된다(Next와 같다)
                WorkerResult::Err { error, .. } if error == "not_found" => Ok(()),
                other => outcome(other),
            }
        }
    };
    match result {
        Ok(()) => {
            let data = match reload(state).await {
                Ok(data) => data,
                Err(reason) => return db_error(&reason),
            };
            let list = if topic {
                TopicListTemplate {
                    topic_list: topic_list_view(&data, true),
                }
                .render()
            } else {
                StockListTemplate {
                    stock_list: stock_list_view(&data, true),
                }
                .render()
            };
            respond(Ok(String::new()), vec![list])
        }
        Err(message) => {
            let item_id = id.unwrap_or(0);
            let dialog = form_only_dialog(item_id, topic);
            let error_id = dialog.error_id.clone();
            respond(
                DeleteFormTemplate { dialog }.render(),
                vec![
                    DialogErrorTemplate {
                        id: error_id,
                        message: Some(message),
                    }
                    .render(),
                ],
            )
        }
    }
}

/// 키워드 다시 만들기(D-15). 대기열에 넣고 바로 돌아오며, 결과는 목록이 다시 읽어 보여 준다(D-37).
pub async fn regenerate(
    State(state): State<Arc<WebState>>,
    Form(fields): Form<Fields>,
) -> Response {
    let id = parse_id(&field(&fields, "id"));
    let result = match id {
        None => Err(describe_stock_error("invalid_request", &Value::Null)),
        Some(id) => {
            let result = match state.worker_api() {
                Some(api) => api.regenerate_keywords(id).await,
                None => unreachable_result(),
            };
            outcome(result)
        }
    };
    match result {
        Ok(()) => {
            let data = match reload(&state).await {
                Ok(data) => data,
                Err(reason) => return db_error(&reason),
            };
            respond(
                Ok(String::new()),
                vec![
                    StockListTemplate {
                        stock_list: stock_list_view(&data, true),
                    }
                    .render(),
                ],
            )
        }
        Err(message) => {
            let stock_id = id.unwrap_or(0);
            let mut keywords = match reload(&state).await {
                Ok(data) => data
                    .stocks
                    .into_iter()
                    .find(|stock| stock.id == stock_id)
                    .map_or_else(|| blank_keywords(stock_id), |stock| stock.keywords),
                Err(reason) => return db_error(&reason),
            };
            keywords.regen_error = Some(message);
            respond(RegenFormTemplate { keywords }.render(), vec![])
        }
    }
}

/// 한국·미국 증시 켜기/끄기(11단계). 켜면 worker가 시장 트리를 만든다.
pub async fn set_market(
    State(state): State<Arc<WebState>>,
    Form(fields): Form<Fields>,
) -> Response {
    let market_text = field(&fields, "market");
    let enabled = field(&fields, "enabled") == "true";
    let result = match market_text.parse::<Market>() {
        Err(_) => Err(describe_stock_error("invalid_request", &Value::Null)),
        Ok(market) => {
            let result = match state.worker_api() {
                Some(api) => api.set_market(market.as_str(), enabled).await,
                None => unreachable_result(),
            };
            outcome(result)
        }
    };
    let data = match reload(&state).await {
        Ok(data) => data,
        Err(reason) => return db_error(&reason),
    };
    match result {
        Ok(()) => respond(
            Ok(String::new()),
            vec![
                MarketListTemplate {
                    market_list: market_list_view(&data, true),
                }
                .render(),
                TopicListTemplate {
                    topic_list: topic_list_view(&data, true),
                }
                .render(),
            ],
        ),
        Err(message) => {
            let mut market = data
                .markets
                .into_iter()
                .find(|row| row.market == market_text)
                .unwrap_or_else(|| MarketRowView {
                    market: "",
                    name: "",
                    enabled: false,
                    next: "true",
                    toggle_label: "켜기",
                    toggle_aria: String::new(),
                    detail: "꺼짐".to_string(),
                    keywords: None,
                    error: None,
                });
            market.error = Some(message);
            respond(MarketFormTemplate { market }.render(), vec![])
        }
    }
}

/// 사용자 주제 넣기(11단계): 적은 그대로 2~20자. 넣으면 켜진 증시 기준으로 트리를 만든다.
pub async fn add_topic(State(state): State<Arc<WebState>>, Form(fields): Form<Fields>) -> Response {
    let name = field(&fields, "name");
    let result = match state.worker_api() {
        Some(api) => api.add_topic(&name).await,
        None => unreachable_result(),
    };
    match outcome(result) {
        Ok(()) => {
            let data = match reload(&state).await {
                Ok(data) => data,
                Err(reason) => return db_error(&reason),
            };
            // 넣을 때마다 입력칸을 비운다(Next는 key를 바꿔 다시 그렸다)
            respond(
                TopicFormTemplate {
                    topic_form: TopicFormView::empty(),
                }
                .render(),
                vec![
                    TopicListTemplate {
                        topic_list: topic_list_view(&data, true),
                    }
                    .render(),
                ],
            )
        }
        Err(message) => respond(
            TopicFormTemplate {
                topic_form: TopicFormView {
                    value: name,
                    error: Some(message),
                },
            }
            .render(),
            vec![],
        ),
    }
}

#[cfg(test)]
#[path = "stocks_tests.rs"]
mod tests;
