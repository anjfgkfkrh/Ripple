use std::future::Future;

use serde::Deserialize;

use crate::server::db::schema::{Impact, Market, MatchKind, NatureProbs};

/// 판단 대상: 종목, 시장(한국·미국 증시), 주제(사용자 주제·자동 토픽, 증시 기준으로 판단) (11단계)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JudgeTarget {
    Stock,
    Market,
    Topic,
}

impl From<crate::server::db::schema::ItemKind> for JudgeTarget {
    fn from(kind: crate::server::db::schema::ItemKind) -> Self {
        use crate::server::db::schema::ItemKind;
        match kind {
            ItemKind::Stock => JudgeTarget::Stock,
            ItemKind::Market => JudgeTarget::Market,
            ItemKind::Topic => JudgeTarget::Topic,
        }
    }
}

impl JudgeTarget {
    pub const ALL: [JudgeTarget; 3] = [JudgeTarget::Stock, JudgeTarget::Market, JudgeTarget::Topic];

    pub fn as_str(self) -> &'static str {
        match self {
            JudgeTarget::Stock => "stock",
            JudgeTarget::Market => "market",
            JudgeTarget::Topic => "topic",
        }
    }
}

/// 판단 대상 항목. 시장·주제는 market이 기준 증시다.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeItem {
    pub kind: JudgeTarget,
    pub name: String,
    /// 미국 종목의 공식 영문 이름 등. 한글 이름과 같으면 생략
    pub official_name: Option<String>,
    pub market: Market,
    pub exchange: String,
    pub code: String,
    /// 주제의 한 줄 설명(자동 토픽)
    #[serde(default)]
    pub summary: Option<String>,
}

/// 기사가 항목에 걸린 경위
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DecisionLink {
    pub kind: MatchKind,
    /// 종목에서 걸린 키워드까지의 경로(D-16)
    pub path: String,
    /// 걸린 키워드가 윗단계와 이어지는 이유(키워드 연결일 때만)
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DecisionArticle {
    pub title: String,
    /// 피드가 준 요약 앞 300자
    pub summary: Option<String>,
}

/// 기사 하나가 항목 하나에 걸린 상황(Jev 입력, D-21)
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DecisionInput {
    pub stock: JudgeItem,
    pub link: DecisionLink,
    pub article: DecisionArticle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// 관련도 0~10
    pub relevance: f64,
    pub relevance_confidence: f64,
    /// 영향도 0~10(주가·시장을 얼마나 움직일까, 11단계)
    pub magnitude: f64,
    pub magnitude_confidence: f64,
    pub impact: Impact,
    pub impact_confidence: f64,
    /// positive·negative·neutral 순서
    pub impact_probabilities: [f64; 3],
    /// "다른 대상이 받는 영향을 보도하고 이 회사는 원인·언급으로만 나오는가"의 예일 확률 0~1.
    /// 종목 대상이면 Some, 그 밖은 None
    pub bystander: Option<f64>,
    /// 응답한 실제 모델 버전
    pub model: String,
    pub usage: TokenUsage,
}

/// 기사 하나에 걸린 시장·주제 항목을 한 요청으로 묻는 입력(17단계 eval-merge)
#[derive(Debug, Clone, PartialEq)]
pub struct MergedTopic {
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergedInput {
    /// 그 기사에 걸린 서로 다른 토픽(걸린 순서)
    pub topics: Vec<MergedTopic>,
    /// 시장 항목이 걸린 증시(관련도를 묻는다)
    pub market_items: Vec<Market>,
    /// 영향도·호재/악재를 물을 증시(시장·주제 항목이 걸린 증시 전부)
    pub markets: Vec<Market>,
    pub article: DecisionArticle,
}

/// 점수(0~10)와 확신도
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scored {
    pub score: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketVerdict {
    pub market: Market,
    /// 그 증시의 시장 항목이 걸렸을 때만
    pub relevance: Option<Scored>,
    pub magnitude: Scored,
    pub impact: Impact,
    pub impact_confidence: f64,
    /// positive·negative·neutral 순서
    pub impact_probabilities: [f64; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergedDecision {
    /// `MergedInput.topics`와 같은 순서의 관련도
    pub topics: Vec<Scored>,
    /// `MergedInput.markets`와 같은 순서
    pub markets: Vec<MarketVerdict>,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeadSubjectType {
    Keyword,
    Topic,
}

/// 키워드 트리 후보나 토픽이 대상과 얼마나 이어지는지 묻는 대상
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LeadSubject {
    #[serde(rename = "type")]
    pub kind: LeadSubjectType,
    /// 키워드 또는 토픽 이름
    pub text: String,
    /// 키워드: 대상에서 키워드까지의 경로
    #[serde(default)]
    pub path: Option<String>,
    /// 토픽: 한 줄 설명과 검색어
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub terms: Option<Vec<String>>,
    /// 앞 단계(또는 대상)와 이어지는 이유
    pub reason: Option<String>,
}

/// 키워드 트리 후보나 토픽이 대상과 얼마나 이어지는지 묻는 입력(11단계)
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LeadInput {
    pub stock: JudgeItem,
    pub subject: LeadSubject,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeadRating {
    pub relevance: f64,
    pub relevance_confidence: f64,
    pub magnitude: f64,
    pub magnitude_confidence: f64,
    pub model: String,
    pub usage: TokenUsage,
}

/// 기사 성격 6확률(예/아니오 질문의 예일 확률)
#[derive(Debug, Clone, PartialEq)]
pub struct NatureDecision {
    pub probs: NatureProbs,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeciderErrorKind {
    NoApiKey,
    Unauthorized,
    RateLimited,
    Timeout,
    Upstream,
    InvalidRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct DeciderError {
    pub message: String,
    pub kind: DeciderErrorKind,
}

impl DeciderError {
    pub fn new(message: impl Into<String>, kind: DeciderErrorKind) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    /// 다음 호출도 똑같이 실패할 오류(키 문제). 이번 회차 평가를 멈춘다.
    pub fn fatal(&self) -> bool {
        matches!(
            self.kind,
            DeciderErrorKind::NoApiKey | DeciderErrorKind::Unauthorized
        )
    }
}

/// 기사 평가 인터페이스(SPEC 6.2 ai/decider). MVP는 TypeSafe 직접 호출,
/// 추후 OpenRouter를 거치는 구현으로 바꿀 수 있게 이 뒤에 둔다(CLAUDE.md).
pub trait Decider: Send + Sync {
    /// 기사 평가. model은 설정 값(고정한 버전, D-23). 바꾸면 다음 평가부터 바로 쓴다.
    fn decide(
        &self,
        input: &DecisionInput,
        model: &str,
    ) -> impl Future<Output = Result<Decision, DeciderError>> + Send;

    /// 한 기사에 걸린 시장·주제 항목을 한 요청으로 평가(17단계 eval-merge)
    fn decide_merged(
        &self,
        input: &MergedInput,
        model: &str,
    ) -> impl Future<Output = Result<MergedDecision, DeciderError>> + Send;

    /// 키워드 트리 후보·토픽 평가(관련도·영향도, 11단계)
    fn rate(
        &self,
        input: &LeadInput,
        model: &str,
    ) -> impl Future<Output = Result<LeadRating, DeciderError>> + Send;

    /// 기사 성격 6문항(사실·해석·전망·의견·사진영상·뉴스 모음)을 한 요청으로 묻는다.
    fn classify_nature(
        &self,
        article: &DecisionArticle,
        model: &str,
    ) -> impl Future<Output = Result<NatureDecision, DeciderError>> + Send;
}

/// 하나를 여러 곳(수집·키워드 대기열·토픽 갱신)이 나눠 쓸 때
impl<T: Decider> Decider for std::sync::Arc<T> {
    fn decide(
        &self,
        input: &DecisionInput,
        model: &str,
    ) -> impl Future<Output = Result<Decision, DeciderError>> + Send {
        (**self).decide(input, model)
    }

    fn decide_merged(
        &self,
        input: &MergedInput,
        model: &str,
    ) -> impl Future<Output = Result<MergedDecision, DeciderError>> + Send {
        (**self).decide_merged(input, model)
    }

    fn rate(
        &self,
        input: &LeadInput,
        model: &str,
    ) -> impl Future<Output = Result<LeadRating, DeciderError>> + Send {
        (**self).rate(input, model)
    }

    fn classify_nature(
        &self,
        article: &DecisionArticle,
        model: &str,
    ) -> impl Future<Output = Result<NatureDecision, DeciderError>> + Send {
        (**self).classify_nature(article, model)
    }
}
