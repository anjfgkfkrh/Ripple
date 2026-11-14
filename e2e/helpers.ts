import Database from "better-sqlite3";
import { expect, type Page, type TestInfo } from "@playwright/test";
import { E2E_APP_PASSWORD, E2E_DB_PATH, E2E_PORTS } from "./test-env";

/**
 * 테스트마다 다른 접속자 IP로 보이게 한다. 로그인 실패 제한(IP별)이 테스트끼리 섞이지 않게 하기 위함.
 * 실제로는 Cloudflare가 이 헤더를 넣는다.
 */
export async function asClient(page: Page, testInfo: TestInfo, label: string) {
  // --repeat-each로 되풀이할 때도 서로 다른 접속자로 보이게 한다(실패 제한 기록이 web 메모리에 남음).
  const repeat = testInfo.repeatEachIndex ? `-${testInfo.repeatEachIndex}` : "";
  await page.setExtraHTTPHeaders({
    "cf-connecting-ip": `e2e-${testInfo.project.name}-${label}${repeat}`,
  });
}

/**
 * 로그인 화면에서 비밀번호를 내고, 서버 응답과 화면 갱신이 끝날 때까지 기다린다.
 * React는 제출이 끝난 뒤 폼을 비우므로, 앞 제출이 끝나기 전에 넣은 값은 지워진다(빈 칸이면 브라우저가 제출을 막음).
 * 그래서 넣기 전과 낸 뒤 모두 "확인 중…"이 사라질 때까지 기다린다.
 */
export async function submitPassword(page: Page, password: string) {
  const pending = page.getByRole("button", { name: "확인 중…" });
  await expect(pending).toHaveCount(0);
  await page.getByLabel("비밀번호").fill(password);
  await Promise.all([
    page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === "/login",
    ),
    page.getByRole("button", { name: "로그인" }).click(),
  ]);
  await expect(pending).toHaveCount(0);
}

export async function login(page: Page, path = "/") {
  // 손님도 홈·종목을 볼 수 있으므로 로그인 화면으로 바로 간다.
  await page.goto(`/login?next=${encodeURIComponent(path)}`);
  await expect(page).toHaveURL(/\/login/);
  await submitPassword(page, E2E_APP_PASSWORD);
  await expect(page).not.toHaveURL(/\/login/);
}

/** worker가 시작하면서 받는 공식 종목 목록이 준비될 때까지 기다린다(로컬 E2E 전용). */
export async function waitForListings() {
  await expect
    .poll(async () => {
      const response = await fetch(
        `http://127.0.0.1:${E2E_PORTS.worker}/health`,
      );
      const body = (await response.json()) as {
        listings?: { KR: number; US: number };
      };
      return (body.listings?.KR ?? 0) > 0 && (body.listings?.US ?? 0) > 0;
    })
    .toBe(true);
}

export async function searchStock(page: Page, query: string) {
  await page.getByLabel("종목 이름 또는 티커").fill(query);
  await page.getByRole("button", { name: "찾기" }).click();
}

/** 찾은 후보에서 종목을 등록하고, 등록한 종목 줄을 돌려준다. */
export async function registerStock(page: Page, query: string, name: string) {
  await searchStock(page, query);
  const candidate = page.getByTestId("candidate").filter({ hasText: name });
  await candidate.getByRole("button", { name: `${name} 등록` }).click();
  await expect(candidate).toContainText("등록함");
  return page.getByTestId("stock").filter({ hasText: name });
}

/**
 * 기사 성격 탭 시험용으로 프로젝트(데스크톱·모바일)마다 종목 하나와 성격이 정해진 기사 셋을 넣는다(이미 있으면 그대로 둔다).
 * 성격은 가짜 TypeSafe의 규칙(mock-services.mjs `natureAnswer`)과 같은 값으로 직접 넣는다:
 * "[포토]"는 media 0.9, "전망"은 forecast 0.9·fact 0.2, 그 밖은 fact 0.9. 나머지는 0.1.
 * 기사는 6시간 전이라 다른 시험의 맨 위 30개를 밀어내지 않고, 이 종목 칩으로만 본다.
 */
export function seedNatureFixtures(project: string): {
  stockName: string;
  photoTitle: string;
  forecastTitle: string;
  factTitle: string;
} {
  const stockName = `성격시험${project}`;
  const photoTitle = "[포토] 현장 사진 모음";
  const forecastTitle = "업황 전망 보고서 발간";
  const factTitle = "일반 공시 소식 전달";
  const db = new Database(E2E_DB_PATH, { timeout: 10_000 });
  try {
    db.transaction(() => {
      const code = `NATURE-${project}`;
      if (
        db
          .prepare("select id from stocks where market = 'KR' and code = ?")
          .get(code)
      ) {
        return;
      }
      const now = Date.now();
      const stock = Number(
        db
          .prepare(
            `insert into stocks (market, code, name, name_ko, exchange, user_input, created_at, keywords_status, keywords_generated_at)
             values ('KR', ?, ?, ?, 'KOSDAQ', ?, ?, 'ready', ?)`,
          )
          .run(code, stockName, stockName, stockName, now, now).lastInsertRowid,
      );
      const article = (
        title: string,
        serial: number,
        chances: [number, number, number, number, number, number],
      ) => {
        const key = `nature.example/${project}/${serial}`;
        const at = now - (6 + serial) * 60 * 60 * 1000;
        const articleId = Number(
          db
            .prepare(
              `insert into articles (url_key, url, title, publisher, feed, published_at, collected_at,
                 nature_fact, nature_analysis, nature_forecast, nature_opinion, nature_media, nature_roundup, nature_at)
               values (?, ?, ?, '가짜경제', 'rss:mock:economy', ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
            )
            .run(key, `https://${key}`, title, at, now, ...chances, now)
            .lastInsertRowid,
        );
        const matchId = Number(
          db
            .prepare(
              `insert into article_matches (article_id, stock_id, keyword_path, match_kind, level, created_at)
               values (?, ?, ?, 'name', 0, ?)`,
            )
            .run(articleId, stock, stockName, now).lastInsertRowid,
        );
        db.prepare(
          `insert into evaluations (match_id, article_id, stock_id, relevance, relevance_confidence, magnitude, impact, impact_confidence, impact_probabilities, model, evaluated_at)
           values (?, ?, ?, 8, 0.7, 7, 'positive', 0.8, '{}', 'jev-1.13.0', ?)`,
        ).run(matchId, articleId, stock, now);
      };
      article(photoTitle, 0, [0.1, 0.1, 0.1, 0.1, 0.9, 0.1]);
      article(forecastTitle, 1, [0.2, 0.1, 0.9, 0.1, 0.1, 0.1]);
      article(factTitle, 2, [0.9, 0.1, 0.1, 0.1, 0.1, 0.1]);
    }).immediate();
  } finally {
    db.close();
  }
  return { stockName, photoTitle, forecastTitle, factTitle };
}
