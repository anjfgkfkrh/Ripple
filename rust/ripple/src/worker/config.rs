use std::fmt;

use serde::Deserialize;

use crate::server::ai::decider::typesafe::TYPESAFE_BASE_URL;
use crate::server::ai::llm::openrouter::OPENROUTER_BASE_URL;
use crate::server::auth::worker_token::read_worker_api_token;
use crate::server::collect::feeds::{RssFeed, rss_feeds};
use crate::server::collect::finnhub::FINNHUB_BASE_URL;
use crate::server::env::{ConfigError, Env, read_port, require_env};
use crate::server::listings::sync::{ListingSources, listing_sources};
use crate::server::prices::kis::KisEnv;
use crate::server::text::js_trim;

/// 한국 종목 주가(13단계, 한국투자증권 KIS). 키·시크릿 중 하나라도 없으면 설정 값이 None
#[derive(Clone, PartialEq, Eq)]
pub struct KisCredentials {
    pub app_key: String,
    pub app_secret: String,
    pub env: KisEnv,
}

// 키가 로그·오류 출력에 섞이지 않게 값을 가린다(설계 1장 규칙 8).
impl fmt::Debug for KisCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KisCredentials")
            .field("app_key", &"***")
            .field("app_secret", &"***")
            .field("env", &self.env)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct WorkerConfig {
    pub database_path: String,
    pub host: String,
    pub port: u16,
    pub api_token: String,
    /// 없으면 worker는 뜨지만 AI 기능(종목 찾기 등)은 오류를 돌려준다.
    pub open_router_api_key: Option<String>,
    pub open_router_base_url: String,
    pub listing_sources: ListingSources,
    /// 없으면 미국 뉴스(Finnhub)는 건너뛴다.
    pub finnhub_api_key: Option<String>,
    pub finnhub_base_url: String,
    pub rss_feeds: Vec<RssFeed>,
    /// 없으면 기사는 모으고 Jev 평가는 미룬다.
    pub typesafe_api_key: Option<String>,
    pub typesafe_base_url: String,
    /// false면 정기 수집·키워드 자동 재생성을 하지 않는다(E2E용 SCHEDULER_DISABLED).
    pub scheduler_jobs_enabled: bool,
    /// 자동 백업 폴더(D-27). 없으면 백업하지 않는다(Docker는 /backups).
    pub backup_dir: Option<String>,
    pub kis: Option<KisCredentials>,
    /// 없으면 KIS_ENV에 맞는 공식 주소
    pub kis_base_url: Option<String>,
}

// 키·토큰 값은 Debug 출력에 나오지 않게 한다.
impl fmt::Debug for WorkerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secret = |key: &Option<String>| key.as_ref().map(|_| "***");
        f.debug_struct("WorkerConfig")
            .field("database_path", &self.database_path)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("api_token", &"***")
            .field("open_router_api_key", &secret(&self.open_router_api_key))
            .field("open_router_base_url", &self.open_router_base_url)
            .field("listing_sources", &self.listing_sources)
            .field("finnhub_api_key", &secret(&self.finnhub_api_key))
            .field("finnhub_base_url", &self.finnhub_base_url)
            .field("rss_feeds", &self.rss_feeds.len())
            .field("typesafe_api_key", &secret(&self.typesafe_api_key))
            .field("typesafe_base_url", &self.typesafe_base_url)
            .field("scheduler_jobs_enabled", &self.scheduler_jobs_enabled)
            .field("backup_dir", &self.backup_dir)
            .field("kis", &self.kis)
            .field("kis_base_url", &self.kis_base_url)
            .finish()
    }
}

/// RSS_FEEDS_JSON 한 줄. zod `z.object`처럼 모르는 키는 버리고, 네 칸 모두 글자여야 한다.
#[derive(Deserialize)]
struct RssFeedInput {
    id: String,
    publisher: String,
    section: String,
    url: String,
}

/// 원천 주소 재정의(OPENROUTER_BASE_URL, LISTING_*_URL, FINNHUB_BASE_URL, TYPESAFE_BASE_URL, KIS_BASE_URL, RSS_FEEDS_JSON)는
/// 테스트에서 가짜 서버를 쓰기 위한 것이다. SCHEDULER_DISABLED도 테스트용이다(E2E가 정기 수집과 겹치지 않게).
pub fn read_worker_config(env: &Env) -> Result<WorkerConfig, ConfigError> {
    let defaults = listing_sources();
    let text = |name: &str| env.get_trimmed(name).map(str::to_string);
    Ok(WorkerConfig {
        database_path: require_env("DATABASE_PATH", env)?,
        // 로컬 실행 때는 밖에서 접근하지 못하게 127.0.0.1, Docker에서는 compose가 0.0.0.0을 준다.
        host: text("WORKER_HOST").unwrap_or_else(|| "127.0.0.1".to_string()),
        port: read_port("WORKER_PORT", 4000, env)?,
        api_token: read_worker_api_token(env)?,
        open_router_api_key: text("OPENROUTER_API_KEY"),
        open_router_base_url: text("OPENROUTER_BASE_URL")
            .unwrap_or_else(|| OPENROUTER_BASE_URL.to_string()),
        listing_sources: ListingSources {
            kind: text("LISTING_KIND_URL").unwrap_or(defaults.kind),
            nasdaq_listed: text("LISTING_NASDAQ_URL").unwrap_or(defaults.nasdaq_listed),
            other_listed: text("LISTING_OTHER_URL").unwrap_or(defaults.other_listed),
        },
        finnhub_api_key: text("FINNHUB_API_KEY"),
        finnhub_base_url: text("FINNHUB_BASE_URL").unwrap_or_else(|| FINNHUB_BASE_URL.to_string()),
        rss_feeds: read_rss_feeds(env.get("RSS_FEEDS_JSON"))?,
        typesafe_api_key: text("TYPESAFE_API_KEY"),
        typesafe_base_url: text("TYPESAFE_BASE_URL")
            .unwrap_or_else(|| TYPESAFE_BASE_URL.to_string()),
        scheduler_jobs_enabled: !matches!(
            env.get("SCHEDULER_DISABLED")
                .map(|v| js_trim(v).to_lowercase())
                .as_deref(),
            Some("1" | "true")
        ),
        backup_dir: text("BACKUP_DIR"),
        kis: read_kis(env)?,
        kis_base_url: text("KIS_BASE_URL"),
    })
}

fn read_kis(env: &Env) -> Result<Option<KisCredentials>, ConfigError> {
    let (Some(app_key), Some(app_secret)) = (
        env.get_trimmed("KIS_APP_KEY"),
        env.get_trimmed("KIS_APP_SECRET"),
    ) else {
        return Ok(None);
    };
    let kind = env
        .get_trimmed("KIS_ENV")
        .map_or_else(|| "demo".to_string(), str::to_lowercase);
    let kis_env = match kind.as_str() {
        "demo" => KisEnv::Demo,
        "real" => KisEnv::Real,
        _ => {
            return Err(ConfigError(
                "KIS_ENV는 demo(모의투자) 또는 real(실전)이어야 합니다.".to_string(),
            ));
        }
    };
    Ok(Some(KisCredentials {
        app_key: app_key.to_string(),
        app_secret: app_secret.to_string(),
        env: kis_env,
    }))
}

fn read_rss_feeds(json: Option<&str>) -> Result<Vec<RssFeed>, ConfigError> {
    let Some(json) = json.filter(|j| !js_trim(j).is_empty()) else {
        return Ok(rss_feeds());
    };
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| ConfigError("RSS_FEEDS_JSON이 올바른 JSON이 아닙니다.".to_string()))?;
    let format_error = || {
        ConfigError("RSS_FEEDS_JSON 형식이 다릅니다([{id, publisher, section, url}]).".to_string())
    };
    let items: Vec<RssFeedInput> = serde_json::from_value(value).map_err(|_| format_error())?;
    // zod: 배열은 1개 이상, id·publisher·section은 1자 이상, url은 올바른 주소(z.url())
    let valid = !items.is_empty()
        && items.iter().all(|item| {
            !item.id.is_empty()
                && !item.publisher.is_empty()
                && !item.section.is_empty()
                && url::Url::parse(&item.url).is_ok()
        });
    if !valid {
        return Err(format_error());
    }
    Ok(items
        .into_iter()
        .map(|item| RssFeed {
            id: item.id,
            publisher: item.publisher,
            section: item.section,
            url: item.url,
        })
        .collect())
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
