//! 화면(web): `src/app/*`, `src/proxy.ts`, `src/components/*`를 옮긴 것(14단계 R10~R12, 설계 5장).
pub mod assets;
pub mod config;
pub mod guard;
pub mod index;
pub mod layout;
pub mod login;
pub mod response;
pub mod settings;
pub mod state;
pub mod status;
pub mod stocks;
pub mod timeline;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use state::WebState;

/// web 라우터. 로그인 확인과 Origin 검사는 모든 요청에 걸린 미들웨어(`guard`)가 한다.
pub fn web_router(state: Arc<WebState>) -> Router {
    Router::new()
        .route("/", get(timeline::page))
        .route("/articles/read", post(timeline::set_read))
        .route("/favicon.ico", get(assets::favicon))
        .route("/static/{name}", get(assets::serve))
        .route("/login", get(login::page).post(login::submit))
        .route("/logout", post(login::logout))
        .route("/status", get(status::page))
        .route("/status/system", get(status::system))
        .route("/status/collection", get(status::collection))
        .route("/status/collect", post(status::collect_now))
        .route("/status/topics", get(status::topics))
        .route("/status/topics/refresh", post(status::refresh_topics))
        .route("/stocks", get(stocks::page))
        .route("/stocks/match", post(stocks::match_stocks))
        .route("/stocks/register", post(stocks::register))
        .route("/stocks/list", get(stocks::list))
        .route("/stocks/delete", post(stocks::delete))
        .route("/stocks/keywords", post(stocks::regenerate))
        .route("/stocks/markets", get(stocks::markets))
        .route("/stocks/market", post(stocks::set_market))
        .route("/stocks/topics", post(stocks::add_topic))
        .route("/stocks/topics/list", get(stocks::topic_list))
        .route("/stocks/topics/delete", post(stocks::delete_topic))
        .route("/settings", get(settings::page).post(settings::save))
        .route("/settings/rebuild", post(settings::rebuild))
        .fallback(layout::not_found)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::guard,
        ))
        .with_state(state)
}

#[cfg(test)]
pub mod test_support {
    //! web 시험용: 실제 포트에 라우터를 띄우고 reqwest(리다이렉트 안 따라감)로 부른다.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use crate::server::auth::config::read_auth_config;
    use crate::server::auth::session::create_session_token;
    use crate::server::env::Env;
    use crate::test_support::{TestDbFile, test_db_file};
    use crate::web::config::WebConfig;

    pub const PASSWORD: &str = "correct-horse-battery";
    pub const T0: i64 = 1_792_368_000_000;

    pub struct TestWeb {
        pub base: String,
        pub client: reqwest::Client,
        pub state: Arc<WebState>,
        pub file: TestDbFile,
        pub clock: Arc<AtomicI64>,
    }

    impl TestWeb {
        pub fn url(&self, path: &str) -> String {
            format!("{}{path}", self.base)
        }

        pub fn cookie(&self) -> String {
            format!(
                "ripple_session={}",
                create_session_token(&self.state.auth, self.clock.load(Ordering::SeqCst))
            )
        }

        /// 로그인한 요청
        pub fn get(&self, path: &str) -> reqwest::RequestBuilder {
            self.client
                .get(self.url(path))
                .header("cookie", self.cookie())
        }

        pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
            self.client
                .post(self.url(path))
                .header("cookie", self.cookie())
        }

        pub fn writer(&self) -> rusqlite::Connection {
            crate::server::db::client::open_db(
                &self.file.path,
                crate::server::db::client::DbAccess::ReadWrite,
            )
            .unwrap()
        }
    }

    pub async fn start_web(worker_url: Option<String>) -> TestWeb {
        start_web_with_tunnel(worker_url, None, None).await
    }

    pub async fn start_web_with_tunnel(
        worker_url: Option<String>,
        tunnel_metrics_url: Option<String>,
        public_url: Option<String>,
    ) -> TestWeb {
        let file = test_db_file();
        let env = Env::from_pairs([
            ("APP_PASSWORD", PASSWORD),
            ("SESSION_SECRET", "test-session-secret-0123456789abcdef"),
        ]);
        let config = WebConfig {
            auth: read_auth_config(&env).unwrap(),
            worker_token: "worker-test-token-0123456789abcdef".to_string(),
            host: "127.0.0.1".to_string(),
            port: 0,
            database_path: Some(file.path.to_string_lossy().into_owned()),
            worker_url,
            tunnel_metrics_url,
            public_url,
        };
        let clock = Arc::new(AtomicI64::new(T0));
        let c = clock.clone();
        let state = Arc::new(WebState::new(
            &config,
            Some(Arc::new(move || c.load(Ordering::SeqCst))),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = web_router(state.clone());
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        TestWeb {
            base,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            state,
            file,
            clock,
        }
    }
}
