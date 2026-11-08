//! 타임라인 화면(`app/(app)/page.tsx`, `timeline-card`·`timeline-filters`·`topic-cards`·`price-strip`·`read-card`·`impact`)
//! 과 읽음 저장(`setArticlesReadAction`).
//!
//! 필터 칩·"더 보기"·토픽/주가 카드는 htmx `hx-boost`로 `#timeline-body`만 바꾼다(R11 결정 ①).
//! htmx 요청(뒤로 가기 복원 제외)에는 그 조각만, 그 밖에는 화면 전체를 돌려준다.
use std::collections::HashMap;
use std::sync::Arc;

use crate::server::auth::session::Access;
use askama::Template;
use axum::Extension;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rusqlite::Connection;
use serde_json::{Value, json};

use crate::server::db::schema::{Impact, ItemKind, Market, MatchKind};
use crate::server::evaluate::read::count_pending_evaluations;
use crate::server::prices::store::{PriceStripItem, read_price_strip};
use crate::server::stocks::messages::describe_stock_error;
use crate::server::stocks::repository::list_active_stocks;
use crate::server::text::{js_number, js_to_fixed};
use crate::server::time::{
    EpochMs, format_seoul_date_time, format_timeline_time, seoul_date_key, timeline_day_label,
};
use crate::server::timeline::read::{
    TIMELINE_PAGE_SIZE, TimelineCard, TimelineFilter, TimelineStock, TopicCard,
    parse_timeline_params, read_timeline, read_topic_cards,
};
use crate::server::timeline::reads::MAX_READ_BATCH;
use crate::server::topics::read::{
    filterable_topic_ids, list_market_items, list_user_topic_items, topic_name,
};
use crate::server::worker_client::WorkerResult;
use crate::web::assets::AssetUrls;
use crate::web::layout::{BannerView, budget_banner_for};
use crate::web::response::{html, render};
use crate::web::state::WebState;
use crate::web::status::unreachable_result;

// ---- 숫자 모양 ----

/// 천 단위 쉼표(정수 글자)
fn group_thousands(digits: &str) -> String {
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `Intl.NumberFormat`(소수 자릿수를 정해 반올림, 기본 `halfExpand`)과 같은 글자.
/// ICU는 수를 가장 짧게 되읽히는 십진수로 바꾼 뒤 그 글자에서 반올림한다(`toFixed`와 달리 1.005 → 1.01).
/// `min_digits`가 0이면 소수 끝의 0을 지우고, 같으면 그 자리까지 채운다.
pub fn format_number(x: f64, max_digits: usize, min_digits: usize) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".to_string()
        } else if x > 0.0 {
            "∞".to_string()
        } else {
            "-∞".to_string()
        };
    }
    let negative = x < 0.0;
    // Rust의 `{}`는 지수 없이 가장 짧게 되읽히는 십진수를 낸다.
    let shortest = format!("{}", x.abs());
    let (int_text, frac_text) = shortest.split_once('.').unwrap_or((shortest.as_str(), ""));
    let mut digits: Vec<u8> = int_text.bytes().chain(frac_text.bytes()).collect();
    let mut int_len = int_text.len();
    if frac_text.len() > max_digits {
        let keep = int_len + max_digits;
        let round_up = digits[keep] >= b'5';
        digits.truncate(keep);
        if round_up {
            let mut i = keep;
            loop {
                if i == 0 {
                    digits.insert(0, b'1');
                    int_len += 1;
                    break;
                }
                i -= 1;
                if digits[i] == b'9' {
                    digits[i] = b'0';
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
    }
    let (int_part, frac_part) = digits.split_at(int_len);
    let int_digits = std::str::from_utf8(int_part).unwrap_or("0");
    let mut frac = String::from_utf8_lossy(frac_part).into_owned();
    while frac.len() > min_digits && frac.ends_with('0') {
        frac.pop();
    }
    while frac.len() < min_digits {
        frac.push('0');
    }
    let mut out = group_thousands(int_digits);
    if !frac.is_empty() {
        out.push('.');
        out.push_str(&frac);
    }
    let is_zero = out.bytes().all(|b| matches!(b, b'0' | b',' | b'.'));
    if negative && !is_zero {
        out.insert(0, '-');
    }
    out
}

/// 주가 띠의 값: 한국은 소수 없는 정수(`ko-KR`), 미국은 소수 둘(`en-US`)
fn price_format(market: Market, x: f64) -> String {
    match market {
        Market::Kr => format_number(x, 0, 0),
        Market::Us => format_number(x, 2, 2),
    }
}

// ---- 화면에 쓰는 값 ----

pub struct ChipView {
    pub href: String,
    pub label: String,
    pub active: bool,
}

pub struct PriceView {
    pub href: String,
    pub selected: bool,
    pub name: String,
    pub price_text: String,
    pub is_kr: bool,
    /// 1 상승, -1 하락, 0 보합
    pub direction: i8,
    pub change_text: String,
    pub basis_text: String,
}

/// 호재/악재/중립 배지(`impact.tsx`): 색은 kind로, 글자는 label로
#[derive(Clone, Copy)]
pub struct ImpactTag {
    pub kind: &'static str,
    pub label: &'static str,
}

fn impact_tag(impact: Impact) -> ImpactTag {
    ImpactTag {
        kind: impact.as_str(),
        label: impact_label(impact),
    }
}

pub struct MarketImpactView {
    /// 증시가 둘 이상일 때만 "한국"·"미국"
    pub market_label: Option<&'static str>,
    pub impact: ImpactTag,
}

pub struct RelatedView {
    pub name: String,
    pub impact: Option<ImpactTag>,
}

pub struct TopicCardView {
    pub href: String,
    pub selected: bool,
    /// 접힌 카드는 흐리게(선택하면 흐리지 않음)
    pub dim: bool,
    pub name: String,
    pub impacts: Vec<MarketImpactView>,
    pub summary: Option<String>,
    pub count_text: String,
    pub related: Vec<RelatedView>,
}

pub struct TopicsView {
    pub active: Vec<TopicCardView>,
    pub faded: Vec<TopicCardView>,
    /// 고른 토픽이 접힌 쪽에 있으면 펼쳐 둔다
    pub faded_open: bool,
}

pub struct CardStockView {
    pub impact: ImpactTag,
    pub kind_label: Option<&'static str>,
    pub name: String,
    pub lens_suffix: Option<&'static str>,
    pub score_text: String,
    pub description: String,
}

pub struct OtherView {
    pub publisher: String,
    pub url: String,
}

pub struct CardView {
    pub article_ids: String,
    pub title: String,
    pub url: String,
    pub publisher: String,
    pub time_text: String,
    pub read: bool,
    pub stocks: Vec<CardStockView>,
    pub others: Vec<OtherView>,
}

pub struct DayView {
    pub label: String,
    pub cards: Vec<CardView>,
}

pub struct FiltersView {
    pub items: Vec<ChipView>,
    pub impacts: Vec<ChipView>,
}

pub struct AutoTopicView {
    pub name: String,
    pub all_href: String,
}

pub struct TimelineBody {
    /// 종목·시장·주제·토픽이 하나도 없음
    pub empty: bool,
    pub min_relevance: String,
    pub min_magnitude: String,
    pub prices: Vec<PriceView>,
    pub topics: Option<TopicsView>,
    pub filters: Option<FiltersView>,
    pub auto_topic: Option<AutoTopicView>,
    pub pending: i64,
    pub empty_text: Option<String>,
    pub days: Vec<DayView>,
    pub more_href: Option<String>,
}

// ---- 계산 ----

const FILTER_ONE_ITEM: (Option<i64>, Option<i64>) = (None, None);

/// 필터와 개수를 담은 타임라인 주소. 필터를 바꾸면 개수는 처음(30개)으로 돌아간다.
pub fn timeline_href(filter: &TimelineFilter, limit: Option<usize>) -> String {
    let mut params: Vec<String> = Vec::new();
    if let Some(id) = filter.stock_id {
        params.push(format!("stock={id}"));
    }
    if let Some(id) = filter.topic_id {
        params.push(format!("topic={id}"));
    }
    if let Some(impact) = filter.impact {
        params.push(format!("impact={}", impact.as_str()));
    }
    if let Some(limit) = limit.filter(|l| *l > TIMELINE_PAGE_SIZE) {
        params.push(format!("limit={limit}"));
    }
    if params.is_empty() {
        "/".to_string()
    } else {
        format!("/?{}", params.join("&"))
    }
}

fn impact_label(impact: Impact) -> &'static str {
    match impact {
        Impact::Positive => "호재",
        Impact::Negative => "악재",
        Impact::Neutral => "중립",
    }
}

fn market_label(market: Market) -> &'static str {
    if market == Market::Kr {
        "한국"
    } else {
        "미국"
    }
}

fn filters_view(
    stocks: &[(i64, String)],
    markets: &[(i64, String)],
    topics: &[(i64, String)],
    filter: &TimelineFilter,
) -> FiltersView {
    let one = |stock_id, topic_id| TimelineFilter {
        stock_id,
        topic_id,
        ..*filter
    };
    let mut items = vec![ChipView {
        href: timeline_href(&one(FILTER_ONE_ITEM.0, FILTER_ONE_ITEM.1), None),
        label: "전체".to_string(),
        active: filter.stock_id.is_none() && filter.topic_id.is_none(),
    }];
    for (id, name) in stocks.iter().chain(markets) {
        items.push(ChipView {
            href: timeline_href(&one(Some(*id), None), None),
            label: name.clone(),
            active: filter.stock_id == Some(*id),
        });
    }
    for (id, name) in topics {
        items.push(ChipView {
            href: timeline_href(&one(None, Some(*id)), None),
            label: name.clone(),
            active: filter.topic_id == Some(*id),
        });
    }
    let mut impacts = vec![ChipView {
        href: timeline_href(
            &TimelineFilter {
                impact: None,
                ..*filter
            },
            None,
        ),
        label: "전체".to_string(),
        active: filter.impact.is_none(),
    }];
    for impact in Impact::ALL {
        impacts.push(ChipView {
            href: timeline_href(
                &TimelineFilter {
                    impact: Some(*impact),
                    ..*filter
                },
                None,
            ),
            label: impact_label(*impact).to_string(),
            active: filter.impact == Some(*impact),
        });
    }
    FiltersView { items, impacts }
}

fn price_view(item: &PriceStripItem, filter: &TimelineFilter, now: EpochMs) -> PriceView {
    let selected = filter.stock_id == Some(item.stock_id);
    let direction: i8 = if item.change > 0.0 {
        1
    } else if item.change < 0.0 {
        -1
    } else {
        0
    };
    let arrow = match direction {
        1 => "▲",
        -1 => "▼",
        _ => "-",
    };
    let sign = match direction {
        1 => "+",
        -1 => "-",
        _ => "",
    };
    PriceView {
        href: timeline_href(
            &TimelineFilter {
                topic_id: None,
                stock_id: if selected { None } else { Some(item.stock_id) },
                ..*filter
            },
            None,
        ),
        selected,
        name: item.name.clone(),
        price_text: format!(
            "{}{}",
            if item.market == Market::Us { "$" } else { "" },
            price_format(item.market, item.price)
        ),
        is_kr: item.market == Market::Kr,
        direction,
        change_text: format!(
            "{arrow}{} {sign}{}%",
            price_format(item.market, item.change.abs()),
            js_to_fixed(item.change_percent.abs(), 2)
        ),
        basis_text: if item.open {
            format!("{} 기준", &format_seoul_date_time(item.at)[11..16])
        } else {
            format!("장 마감 · {} 기준", format_timeline_time(item.at, now))
        },
    }
}

fn topic_card_view(card: &TopicCard, filter: &TimelineFilter, now: EpochMs) -> TopicCardView {
    let selected = filter.topic_id == Some(card.topic_id);
    let several = card.impacts.len() > 1;
    TopicCardView {
        href: timeline_href(
            &TimelineFilter {
                stock_id: None,
                topic_id: if selected { None } else { Some(card.topic_id) },
                ..*filter
            },
            None,
        ),
        selected,
        dim: card.faded && !selected,
        name: card.name.clone(),
        impacts: card
            .impacts
            .iter()
            .map(|item| MarketImpactView {
                market_label: several.then(|| market_label(item.market)),
                impact: impact_tag(item.impact),
            })
            .collect(),
        summary: card.summary.clone().filter(|s| !s.is_empty()),
        count_text: format!(
            "기사 {}건{} · 마지막 {}",
            card.shown_total,
            if card.shown_24h != card.shown_total {
                format!(" (24시간 {}건)", card.shown_24h)
            } else {
                String::new()
            },
            format_timeline_time(card.last_article_at, now)
        ),
        related: card
            .related
            .iter()
            .map(|stock| RelatedView {
                name: stock.name.clone(),
                impact: stock.impact.map(impact_tag),
            })
            .collect(),
    }
}

fn topics_view(cards: &[TopicCard], filter: &TimelineFilter, now: EpochMs) -> Option<TopicsView> {
    if cards.is_empty() {
        return None;
    }
    let view = |faded: bool| -> Vec<TopicCardView> {
        cards
            .iter()
            .filter(|card| card.faded == faded)
            .map(|card| topic_card_view(card, filter, now))
            .collect()
    };
    Some(TopicsView {
        active: view(false),
        faded: view(true),
        faded_open: cards
            .iter()
            .any(|card| card.faded && Some(card.topic_id) == filter.topic_id),
    })
}

/// 이 기사가 이 항목에 걸린 경위(왜 이 기사가 나왔는지)
fn describe_link(stock: &TimelineStock) -> String {
    match stock.match_kind {
        MatchKind::Direct => "Finnhub가 이 종목 뉴스로 분류".to_string(),
        MatchKind::Name => "기사에 종목 이름이 나옴".to_string(),
        MatchKind::Keyword => format!("키워드: {}", stock.path),
    }
}

fn card_view(card: &TimelineCard, now: EpochMs) -> CardView {
    let mut topic_counts: HashMap<i64, usize> = HashMap::new();
    for stock in &card.stocks {
        if let Some(topic_id) = stock.topic_id {
            *topic_counts.entry(topic_id).or_insert(0) += 1;
        }
    }
    let ids: Vec<String> = std::iter::once(card.article_id)
        .chain(card.others.iter().map(|o| o.article_id))
        .map(|id| id.to_string())
        .collect();
    CardView {
        article_ids: ids.join(","),
        title: card.title.clone(),
        url: card.url.clone(),
        publisher: card.publisher.clone(),
        time_text: format_timeline_time(card.published_at, now),
        read: card.read,
        stocks: card
            .stocks
            .iter()
            .map(|stock| CardStockView {
                impact: impact_tag(stock.impact),
                kind_label: match stock.kind {
                    ItemKind::Stock => None,
                    ItemKind::Market => Some("시장"),
                    ItemKind::Topic => Some("주제"),
                },
                name: stock.name.clone(),
                lens_suffix: stock
                    .topic_id
                    .filter(|id| topic_counts.get(id).copied().unwrap_or(0) > 1)
                    .map(|_| {
                        if stock.market == Market::Kr {
                            "(한국 증시 기준)"
                        } else {
                            "(미국 증시 기준)"
                        }
                    }),
                score_text: format!(
                    "관련도 {}{}",
                    js_to_fixed(stock.relevance, 1),
                    stock
                        .magnitude
                        .map_or_else(String::new, |m| format!(" · 영향도 {}", js_to_fixed(m, 1)))
                ),
                description: describe_link(stock),
            })
            .collect(),
        others: card
            .others
            .iter()
            .map(|other| OtherView {
                publisher: other.publisher.clone(),
                url: other.url.clone(),
            })
            .collect(),
    }
}

/// 날짜마다 구분선을 둔다(2026-10-21 결정).
fn day_groups(cards: &[TimelineCard], now: EpochMs) -> Vec<DayView> {
    let mut days: Vec<(String, DayView)> = Vec::new();
    for card in cards {
        let key = seoul_date_key(card.published_at);
        let view = card_view(card, now);
        match days.last_mut() {
            Some((last, day)) if *last == key => day.cards.push(view),
            _ => days.push((
                key,
                DayView {
                    label: timeline_day_label(card.published_at, now),
                    cards: vec![view],
                },
            )),
        }
    }
    days.into_iter().map(|(_, day)| day).collect()
}

pub fn build_body(
    conn: &Connection,
    now: EpochMs,
    params: &[(String, String)],
) -> rusqlite::Result<TimelineBody> {
    let stocks = list_active_stocks(conn)?;
    let stock_chips: Vec<(i64, String)> = stocks
        .iter()
        .map(|s| (s.id, s.name_ko.clone().unwrap_or_else(|| s.name.clone())))
        .collect();
    let markets: Vec<(i64, String)> = list_market_items(conn)?
        .into_iter()
        .filter_map(|m| m.stock.map(|s| (s.id, m.name.to_string())))
        .collect();
    let user_topics: Vec<(i64, String)> = list_user_topic_items(conn)?
        .into_iter()
        .map(|item| (item.topic.id, item.topic.name))
        .collect();
    let topic_cards = read_topic_cards(conn, now)?;
    if stock_chips.is_empty()
        && markets.is_empty()
        && user_topics.is_empty()
        && topic_cards.is_empty()
    {
        return Ok(TimelineBody {
            empty: true,
            min_relevance: String::new(),
            min_magnitude: String::new(),
            prices: Vec::new(),
            topics: None,
            filters: None,
            auto_topic: None,
            pending: 0,
            empty_text: None,
            days: Vec::new(),
            more_href: None,
        });
    }
    let active_ids: Vec<i64> = stock_chips
        .iter()
        .chain(&markets)
        .map(|(id, _)| *id)
        .collect();
    let (filter, limit) = parse_timeline_params(params, &active_ids, &filterable_topic_ids(conn)?);
    let page = read_timeline(conn, &filter, limit)?;
    let pending = count_pending_evaluations(conn, now)?;
    let filtered =
        filter.stock_id.is_some() || filter.topic_id.is_some() || filter.impact.is_some();
    // 자동 토픽은 칩이 없으므로 무엇을 보고 있는지 따로 알린다.
    let auto_topic = match filter.topic_id {
        Some(id) if !user_topics.iter().any(|(topic_id, _)| *topic_id == id) => {
            topic_name(conn, id)?.map(|name| AutoTopicView {
                name,
                all_href: timeline_href(
                    &TimelineFilter {
                        topic_id: None,
                        ..filter
                    },
                    None,
                ),
            })
        }
        _ => None,
    };
    let min_relevance = js_number(page.min_relevance);
    let min_magnitude = js_number(page.min_magnitude);
    Ok(TimelineBody {
        empty: false,
        prices: read_price_strip(conn, now)?
            .iter()
            .map(|item| price_view(item, &filter, now))
            .collect(),
        topics: topics_view(&topic_cards, &filter, now),
        filters: Some(filters_view(&stock_chips, &markets, &user_topics, &filter)),
        auto_topic,
        pending,
        empty_text: page.cards.is_empty().then(|| {
            if filtered {
                "조건에 맞는 기사가 없습니다.".to_string()
            } else {
                format!(
                    "아직 관련도 {min_relevance}점·영향도 {min_magnitude}점 이상인 기사가 없습니다."
                )
            }
        }),
        days: day_groups(&page.cards, now),
        more_href: page
            .has_more
            .then(|| timeline_href(&filter, Some(limit + TIMELINE_PAGE_SIZE))),
        min_relevance,
        min_magnitude,
    })
}

// ---- 화면 ----

#[derive(Template)]
#[template(path = "timeline/page.html")]
struct TimelinePageTemplate {
    assets: AssetUrls,
    banner: Option<BannerView>,
    member: bool,
    body: TimelineBody,
}

#[derive(Template)]
#[template(path = "timeline/body.html")]
struct TimelineBodyTemplate {
    body: TimelineBody,
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 타임라인(`/`). 고른 필터와 개수는 주소에 남는다(D-25).
pub async fn page(
    State(state): State<Arc<WebState>>,
    Extension(access): Extension<Access>,
    headers: HeaderMap,
    Query(params): Query<Vec<(String, String)>>,
) -> Response {
    let now = state.now();
    let mut body = match state.db.with(|conn| build_body(conn, now, &params)) {
        Ok(body) => body,
        Err(reason) => {
            return html(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB를 읽지 못했습니다: {}", escape(&reason)),
            );
        }
    };
    // 주인의 읽음 표시를 손님에게 보이지 않는다.
    if !access.is_member() {
        for card in body.days.iter_mut().flat_map(|day| day.cards.iter_mut()) {
            card.read = false;
        }
    }
    // 뒤로 가기 복원은 화면 전체를 기대한다.
    let partial =
        headers.contains_key("hx-request") && !headers.contains_key("hx-history-restore-request");
    if partial {
        return render(&TimelineBodyTemplate { body });
    }
    render(&TimelinePageTemplate {
        assets: state.assets.urls.clone(),
        banner: budget_banner_for(&state, &access),
        member: access.is_member(),
        body,
    })
}

// ---- 읽음 저장 ----

const READ_FAILED: &str = "읽음 표시를 저장하지 못했습니다.";

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, value.to_string()).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response
}

/// 인자는 믿지 않고 형식을 다시 본다(`setArticlesReadAction`).
fn parse_read_request(body: &[u8]) -> Option<(Vec<i64>, bool)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let read = value.get("read")?.as_bool()?;
    let ids = value.get("articleIds")?.as_array()?;
    if ids.is_empty() || ids.len() > MAX_READ_BATCH {
        return None;
    }
    let ids = ids
        .iter()
        .map(|id| id.as_i64().filter(|id| *id > 0))
        .collect::<Option<Vec<i64>>>()?;
    Some((ids, read))
}

/// `POST /articles/read`: 카드에 묶인 기사들을 읽음/안 읽음으로 저장한다(`static/app.js`가 부른다).
pub async fn set_read(State(state): State<Arc<WebState>>, body: Bytes) -> Response {
    let Some((ids, read)) = parse_read_request(&body) else {
        return json_response(StatusCode::OK, json!({ "error": READ_FAILED }));
    };
    let result = match state.worker_api() {
        Some(api) => api.set_articles_read(&ids, read).await,
        None => unreachable_result(),
    };
    match result {
        WorkerResult::Ok { .. } => json_response(StatusCode::OK, json!({ "ok": true })),
        WorkerResult::Err { error, body, .. } => json_response(
            StatusCode::OK,
            json!({ "error": describe_stock_error(&error, &body) }),
        ),
    }
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
