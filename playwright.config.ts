import { defineConfig, devices } from "@playwright/test";
import {
  E2E_APP_PASSWORD,
  E2E_DB_PATH,
  E2E_PORTS,
  E2E_SESSION_SECRET,
  E2E_WORKER_API_TOKEN,
} from "./e2e/test-env";

// E2E_BASE_URL을 주면(예: Docker로 띄운 http://127.0.0.1:3000) 서버를 띄우지 않고 그 주소를 검사한다.
const externalBaseUrl = process.env.E2E_BASE_URL;

const E2E_DB = E2E_DB_PATH;
const MOCK = `http://127.0.0.1:${E2E_PORTS.mock}`;

const baseEnv = Object.fromEntries(
  Object.entries(process.env).filter(
    (entry): entry is [string, string] => entry[1] !== undefined,
  ),
);

export default defineConfig({
  testDir: "e2e",
  // R10~R11 동안 Rust web에 없는 화면의 시험(R12에서 지움, DECISIONS.md 1장 "14단계 R10 결정")
  testIgnore: [
    "**/timeline.spec.ts",
    "**/stocks.spec.ts",
    "**/keywords.spec.ts",
    "**/settings.spec.ts",
    "**/collect.spec.ts",
  ],
  outputDir: "test-results",
  reporter: "list",
  use: {
    baseURL: externalBaseUrl ?? `http://127.0.0.1:${E2E_PORTS.web}`,
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile", use: { ...devices["Pixel 7"] } },
  ],
  webServer: externalBaseUrl
    ? undefined
    : [
        {
          // 공식 종목 목록·OpenRouter 대신 쓰는 가짜 외부 서비스(유료 API를 부르지 않음)
          command: "node e2e/mock-services.mjs",
          url: `${MOCK}/health`,
          env: { ...baseEnv, MOCK_PORT: String(E2E_PORTS.mock) },
        },
        {
          // 테스트용 DB를 새로 만들고 마이그레이션한 뒤 worker를 띄운다(14단계 R9부터 Rust 실행 파일, `cd rust && ./build.sh --release`로 먼저 빌드).
          command: `node e2e/wait-for.mjs ${MOCK}/health && rm -rf .e2e && rust/target/release/ripple migrate && rust/target/release/ripple worker`,
          url: `http://127.0.0.1:${E2E_PORTS.worker}/health`,
          env: {
            ...baseEnv,
            DATABASE_PATH: E2E_DB,
            WORKER_HOST: "127.0.0.1",
            WORKER_PORT: String(E2E_PORTS.worker),
            WORKER_API_TOKEN: E2E_WORKER_API_TOKEN,
            OPENROUTER_API_KEY: "e2e-fake-key",
            OPENROUTER_BASE_URL: `${MOCK}/openrouter`,
            LISTING_KIND_URL: `${MOCK}/kind`,
            LISTING_NASDAQ_URL: `${MOCK}/nasdaqlisted.txt`,
            LISTING_OTHER_URL: `${MOCK}/otherlisted.txt`,
            // 실제 언론사·Finnhub 대신 가짜 피드를 쓴다.
            RSS_FEEDS_JSON: JSON.stringify([
              {
                id: "rss:mock:economy",
                publisher: "가짜경제",
                section: "경제",
                url: `${MOCK}/rss/economy.xml`,
              },
              {
                id: "rss:mock:numeric",
                publisher: "가짜뉴스",
                section: "전체",
                url: `${MOCK}/rss/numeric.xml`,
              },
            ]),
            FINNHUB_API_KEY: "e2e-fake-finnhub-key",
            FINNHUB_BASE_URL: `${MOCK}/finnhub`,
            // 실제 TypeSafe 대신 가짜 Jev를 쓴다(유료 호출 없음).
            TYPESAFE_API_KEY: "e2e-fake-typesafe-key",
            TYPESAFE_BASE_URL: `${MOCK}/typesafe`,
            // 정기 수집이 테스트의 "지금 수집"과 겹치지 않게 끈다(계획 표시는 그대로 검사한다).
            SCHEDULER_DISABLED: "1",
          },
          gracefulShutdown: { signal: "SIGTERM", timeout: 5000 },
        },
        {
          // 14단계 R10부터 Rust web(`cd rust && ./build.sh --release`로 먼저 빌드).
          command: "rust/target/release/ripple web",
          // DB를 건드리지 않는 정적 파일로 기동 여부만 본다.
          url: `http://127.0.0.1:${E2E_PORTS.web}/favicon.ico`,
          // .env의 실제 값보다 이 값이 우선한다(Next는 이미 있는 환경변수를 덮어쓰지 않음).
          env: {
            ...baseEnv,
            DATABASE_PATH: E2E_DB,
            HOSTNAME: "127.0.0.1",
            PORT: String(E2E_PORTS.web),
            WORKER_URL: `http://127.0.0.1:${E2E_PORTS.worker}`,
            WORKER_API_TOKEN: E2E_WORKER_API_TOKEN,
            APP_PASSWORD: E2E_APP_PASSWORD,
            SESSION_SECRET: E2E_SESSION_SECRET,
          },
          gracefulShutdown: { signal: "SIGTERM", timeout: 5000 },
        },
      ],
});
