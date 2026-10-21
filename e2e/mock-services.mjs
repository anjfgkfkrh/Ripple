// E2E 전용 가짜 외부 서비스: 공식 종목 목록(KIND·NASDAQ 표본), OpenRouter, 언론사 RSS, Finnhub, TypeSafe(Jev).
// 실제 사이트와 유료 API를 부르지 않고 화면 흐름을 검사하기 위한 것이다.
import { readFileSync } from "node:fs";
import { createServer } from "node:http";

const port = Number(process.env.MOCK_PORT ?? 4200);
const fixture = (name) =>
  readFileSync(new URL(`../tests/fixtures/${name}`, import.meta.url));

const SAMSUNG = {
  market: "KR",
  code: "005930",
  name: "삼성전자",
  nameKo: "삼성전자",
};
const NVIDIA = {
  market: "US",
  code: "NVDA",
  name: "NVIDIA Corporation",
  nameKo: "엔비디아",
};
const KAKAO = {
  market: "KR",
  code: "035720",
  name: "카카오",
  nameKo: "카카오",
};
const HYUNDAI = {
  market: "KR",
  code: "000720",
  name: "현대건설",
  nameKo: "현대건설",
};
const HYNIX = {
  market: "KR",
  code: "000660",
  name: "SK하이닉스",
  nameKo: "SK하이닉스",
};
const TESLA = {
  market: "US",
  code: "TSLA",
  name: "Tesla, Inc.",
  nameKo: "테슬라",
};
const ANSWERS = {
  삼전: [SAMSUNG],
  삼성전자: [SAMSUNG],
  NVDA: [NVIDIA],
  엔비디아: [NVIDIA],
  카카오: [KAKAO],
  현대건설: [HYUNDAI],
  TSLA: [TESLA],
  테슬라: [TESLA],
  하닉: [HYNIX],
  SK하이닉스: [HYNIX],
};

const HOUR = 60 * 60 * 1000;
const xml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");

/** 가짜 언론사 RSS. 발행 시각은 요청할 때마다 지금을 기준으로 만든다. */
function rssFeed(name, base) {
  const now = Date.now();
  const items =
    name === "economy"
      ? [
          ["SK하이닉스, HBM 공급 확대", `${base}/news/1`, now - HOUR],
          [
            "AI 반도체 수요 급증에 데이터센터 투자 확대",
            `${base}/news/2`,
            now - 2 * HOUR,
          ],
          ["SK하이닉스 오래된 기사", `${base}/news/old`, now - 72 * HOUR],
          ["주말 날씨", `${base}/news/3`, now - HOUR],
        ]
      : [
          [
            "SK하이닉스 3분기 실적 발표",
            `${base}/news/4?utm_source=rss`,
            now - 3 * HOUR,
          ],
        ];
  const date = (ms) => {
    if (name === "economy") return new Date(ms).toUTCString();
    // 노컷뉴스처럼 월을 숫자로 쓰는 형식(한국 시간)
    const k = new Date(ms + 9 * HOUR);
    const p = (n) => String(n).padStart(2, "0");
    return `Tue, ${p(k.getUTCDate())} ${p(k.getUTCMonth() + 1)} ${k.getUTCFullYear()} ${p(k.getUTCHours())}:${p(k.getUTCMinutes())}:${p(k.getUTCSeconds())} +0900`;
  };
  return `<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>가짜 ${name}</title>${items
    .map(
      ([title, link, at]) =>
        `<item><title><![CDATA[${title}]]></title><link>${xml(link)}</link><description>${xml(`<p>${title} 관련 기사</p>`)}</description><pubDate>${date(at)}</pubDate></item>`,
    )
    .join("")}</channel></rss>`;
}

/** 가짜 Finnhub 뉴스 */
function finnhubNews(id, headline, base) {
  return {
    id,
    headline,
    summary: `${headline} summary`,
    url: `${base}/finnhub-news/${id}`,
    source: "Yahoo",
    datetime: Math.floor((Date.now() - HOUR) / 1000),
    related: "",
    category: "company",
  };
}

// 키워드 트리(깊이 2, 단계마다 5개). [키워드, 영어 표기, 부모, 이유]
const KEYWORD_TREE = [
  [1, "AI 반도체", "AI chips", "", "AI 서버 수요와 직결"],
  [1, "데이터센터", "Data centers", "", "주요 수요처"],
  [1, "환율", "Exchange rates", "", "해외 매출 비중이 큼"],
  [1, "금리", "Interest rates", "", "성장주 평가에 영향"],
  [1, "공급망", "Supply chain", "", "부품 조달 위험"],
  [2, "HBM", "HBM", "AI 반도체", "AI 반도체 핵심 부품"],
  [2, "GPU", "GPUs", "AI 반도체", "AI 연산 장치"],
  [2, "클라우드", "Cloud computing", "데이터센터", "데이터센터 최대 고객"],
  [2, "전력 설비", "Power equipment", "데이터센터", "증설의 병목"],
  [2, "원달러 환율", "USD/KRW", "환율", "원화 기준 실적 변동"],
];
/** 키워드를 만드는 동안의 화면(만드는 중)을 볼 수 있게 조금 늦게 답한다. */
const KEYWORD_DELAY_MS = 800;
/** 이 종목은 첫 키워드 생성만 실패시킨다(실패 표시와 다시 만들기 검사용). */
const FAIL_FIRST_KEYWORDS = "종목: 현대건설";
let failedOnce = false;

function keywordAnswer(user) {
  const us = user.includes("(미국,");
  return {
    keywords: KEYWORD_TREE.map(
      ([level, keyword, keywordEn, parent, reason]) => ({
        level,
        keyword,
        keywordEn: us ? keywordEn : "",
        parent,
        reason,
      }),
    ),
  };
}

/**
 * 가짜 Jev 답(공식 API 응답 모양). 종목 이름으로 걸린 기사는 관련(4단계 → 8점), 그 밖은 1.5단계(3점).
 * 제목에 '확대'·'실적'·'rise'가 있으면 호재, 아니면 중립.
 */
function jevAnswer({ model, state }) {
  const named = state.link.path === state.stock.name;
  const positive = /확대|실적|rise/.test(state.article.title);
  return {
    model,
    answers: {
      relevance: {
        type: "score",
        score: named ? 4 : 1.5,
        confidence: 0.7,
        legend: {},
        probabilities: {},
      },
      impact: {
        type: "choice",
        choice: positive ? "positive" : "neutral",
        confidence: 0.8,
        probabilities: positive
          ? { positive: 0.85, negative: 0.05, neutral: 0.1 }
          : { positive: 0.1, negative: 0.05, neutral: 0.85 },
      },
    },
    usage: { input_tokens: 900, output_tokens: 50 },
  };
}

function sendJson(response, status, body) {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(JSON.stringify(body));
}

const files = {
  "/kind": ["application/vnd.ms-excel; charset=EUC-KR", "kind-sample.xls"],
  "/nasdaqlisted.txt": ["text/plain", "nasdaqlisted-sample.txt"],
  "/otherlisted.txt": ["text/plain", "otherlisted-sample.txt"],
};

createServer(async (request, response) => {
  const url = new URL(request.url ?? "/", "http://mock");
  const { pathname } = url;
  const base = `http://127.0.0.1:${port}`;
  if (pathname === "/health") return response.end("ok");
  if (pathname === "/robots.txt") {
    response.writeHead(200, { "content-type": "text/plain" });
    return response.end("User-agent: *\nAllow: /\n");
  }
  const feed = pathname.match(/^\/rss\/(economy|numeric)\.xml$/);
  if (request.method === "GET" && feed) {
    response.writeHead(200, {
      "content-type": "application/rss+xml; charset=utf-8",
    });
    return response.end(rssFeed(feed[1], base));
  }
  if (request.method === "GET" && pathname.startsWith("/finnhub/")) {
    if (!request.headers["x-finnhub-token"]) {
      return sendJson(response, 401, { error: "no token" });
    }
    if (pathname === "/finnhub/company-news") {
      const symbol = url.searchParams.get("symbol") ?? "?";
      return sendJson(response, 200, [
        finnhubNews(
          Number.parseInt(symbol, 36) || 1,
          `${symbol} shares rise`,
          base,
        ),
      ]);
    }
    if (pathname === "/finnhub/news") {
      return sendJson(response, 200, [
        finnhubNews(900, "Global AI chips demand surges", base),
        finnhubNews(901, "Oil prices fall", base),
      ]);
    }
  }

  if (request.method === "GET" && files[pathname]) {
    const [type, name] = files[pathname];
    response.writeHead(200, { "content-type": type });
    return response.end(fixture(name));
  }

  // OpenRouter 공개 모델 목록(설정 화면의 모델 ID 확인용, 실제 응답 모양: { data, links })
  if (request.method === "GET" && pathname === "/openrouter/models") {
    return sendJson(response, 200, {
      data: [{ id: "anthropic/claude-haiku-5.5" }, { id: "e2e/other-model" }],
      links: { next: null },
    });
  }

  if (request.method === "POST" && pathname === "/typesafe/v1/systemone") {
    if (!request.headers.authorization?.startsWith("Bearer ")) {
      return sendJson(response, 401, { detail: "no key" });
    }
    let body = "";
    for await (const chunk of request) body += chunk;
    const parsed = JSON.parse(body);
    if (
      !parsed.model ||
      parsed.questions?.relevance?.type !== "score" ||
      parsed.questions?.impact?.type !== "choice" ||
      !parsed.state?.article?.title
    ) {
      return sendJson(response, 422, { detail: "bad request" });
    }
    return sendJson(response, 200, jevAnswer(parsed));
  }

  if (
    request.method === "POST" &&
    pathname === "/openrouter/chat/completions"
  ) {
    if (!request.headers.authorization?.startsWith("Bearer ")) {
      return sendJson(response, 401, {
        error: { message: "no key", code: 401 },
      });
    }
    let body = "";
    for await (const chunk of request) body += chunk;
    const { model, messages, response_format } = JSON.parse(body);
    const user = messages.at(-1)?.content ?? "";

    if (response_format?.json_schema?.name === "keyword_tree") {
      await new Promise((resolve) => setTimeout(resolve, KEYWORD_DELAY_MS));
      if (user.startsWith(FAIL_FIRST_KEYWORDS) && !failedOnce) {
        failedOnce = true;
        return sendJson(response, 502, {
          error: { message: "mock upstream error", code: 502 },
        });
      }
      return sendJson(response, 200, {
        model,
        choices: [
          { message: { content: JSON.stringify(keywordAnswer(user)) } },
        ],
        usage: { prompt_tokens: 600, completion_tokens: 500, cost: 0.0003 },
      });
    }

    return sendJson(response, 200, {
      model,
      choices: [
        {
          message: {
            content: JSON.stringify({ candidates: ANSWERS[user] ?? [] }),
          },
        },
      ],
      usage: { prompt_tokens: 420, completion_tokens: 40, cost: 0.00006 },
    });
  }

  response.writeHead(404);
  response.end();
}).listen(port, "127.0.0.1", () => {
  console.log(`가짜 외부 서비스 대기 중: http://127.0.0.1:${port}`);
});
