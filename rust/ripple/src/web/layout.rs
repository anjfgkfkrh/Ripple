//! 화면 공통 틀: 예산 배너(`budget-banner.tsx`)와 없는 주소(404).
use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use crate::server::budget::report::{BudgetLevel, budget_level};
use crate::server::budget::usage::month_spend_usd;
use crate::server::log::log_error;
use crate::server::settings::{BUDGET_MONTHLY_USD, read_num};
use crate::server::text::{js_number, js_to_fixed};
use crate::web::assets::AssetUrls;
use crate::web::response::render_with_status;
use crate::web::state::WebState;

pub struct BannerView {
    pub over: bool,
    pub text: String,
    pub link: &'static str,
}

/// 월 예산 80% 넘음(노랑)·한도 도달(빨강)을 모든 화면 위에 알린다(D-07).
/// DB를 읽지 못하면 배너 없이 화면을 낸다(화면 본문이 오류를 보여 준다).
pub fn budget_banner(state: &WebState) -> Option<BannerView> {
    let now = state.now();
    let numbers = state.db.with(|conn| {
        Ok((
            month_spend_usd(conn, now)?,
            read_num(conn, &BUDGET_MONTHLY_USD)?,
        ))
    });
    let (spent, limit) = match numbers {
        Ok(numbers) => numbers,
        Err(error) => {
            log_error("예산 배너를 만들지 못했습니다", &error);
            return None;
        }
    };
    banner_for(spent, limit)
}

pub fn banner_for(spent: f64, limit: f64) -> Option<BannerView> {
    match budget_level(spent, limit) {
        BudgetLevel::Ok => None,
        BudgetLevel::Over => Some(BannerView {
            over: true,
            text: format!(
                "이번 달 AI 예산(${})을 다 써서 기사 평가·키워드 만들기·종목 찾기를 멈췄습니다. 수집은 계속합니다. ",
                js_number(limit)
            ),
            link: "설정에서 한도 올리기",
        }),
        BudgetLevel::Warn => Some(BannerView {
            over: false,
            text: format!(
                "이번 달 AI 예산의 {}%를 썼습니다(${} / ${}). ",
                (spent / limit * 100.0).floor(),
                js_to_fixed(spent, 2),
                js_number(limit)
            ),
            link: "설정 보기",
        }),
    }
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundTemplate {
    assets: AssetUrls,
    banner: Option<BannerView>,
}

/// 없는 주소(로그인한 사용자). 로그인하지 않았으면 이 앞의 미들웨어가 로그인 화면으로 보낸다.
pub async fn not_found(State(state): State<Arc<WebState>>) -> Response {
    render_with_status(
        StatusCode::NOT_FOUND,
        &NotFoundTemplate {
            assets: state.assets.urls.clone(),
            banner: budget_banner(&state),
        },
    )
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
