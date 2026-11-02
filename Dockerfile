# syntax=docker/dockerfile:1
# 한 Dockerfile에서 타깃별 이미지를 만든다: rust-worker(마이그레이션 포함, 14단계 R9부터 compose가 씀) / web.
# worker 타깃은 Rust 전환 중 되돌릴 수 있게 남겨 둔다(R13에서 TS와 함께 지운다).

# ── 공통 베이스 ─────────────────────────────────────
FROM node:24-trixie-slim AS base
# 외부로 사용 통계를 보내지 않는다(Next.js 텔레메트리).
ENV NEXT_TELEMETRY_DISABLED=1
WORKDIR /app
# 볼륨 마운트 지점. 빈 named volume은 이 디렉터리의 소유자(node)를 물려받는다.
RUN mkdir -p /data && chown node:node /data

# ── 의존성 설치 (빌드용, dev 포함) ──────────────────
FROM base AS deps
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile

# ── 빌드 ────────────────────────────────────────────
FROM deps AS build
COPY . .
RUN pnpm build && pnpm build:worker
# worker 실행에 필요한 네이티브 모듈만 모은다(나머지는 번들에 들어 있다).
RUN node scripts/collect-worker-modules.mjs /out

# ── worker (compose의 migrate 서비스도 이 이미지를 쓴다) ──
FROM base AS worker
ENV NODE_ENV=production
COPY --from=build /out/node_modules ./node_modules
COPY --from=build /app/dist/worker ./dist/worker
COPY --from=build /app/drizzle ./drizzle
# 네이티브 모듈이 이 이미지에서 실제로 로드되는지 빌드 때 확인한다.
RUN node -e "new (require('better-sqlite3'))(':memory:').close()"
USER node
CMD ["node", "--enable-source-maps", "dist/worker/index.mjs"]

# ── Rust 실행 파일(14단계 R9, 결정 "R9 결정 ①") ──────
FROM rust:1.99.1-trixie AS rust-build
WORKDIR /src
COPY rust/ ./
RUN ./build.sh --release

# compose의 migrate·worker 서비스가 쓴다. web은 R10~R12까지 위의 TS 이미지를 그대로 쓴다.
FROM debian:trixie-slim AS rust-worker
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    # 기존 볼륨(/data)과 web(node 이미지)이 쓰는 사용자와 같은 uid로 DB 파일을 만든다(web이 -shm·-wal을 읽어야 한다).
    && groupadd --gid 1000 node \
    && useradd --uid 1000 --gid node --create-home node \
    && mkdir -p /data && chown node:node /data
COPY --from=rust-build /src/target/release/ripple /usr/local/bin/ripple
USER node
CMD ["ripple", "worker"]

# ── web ─────────────────────────────────────────────
FROM base AS web
ENV NODE_ENV=production \
    PORT=3000 \
    HOSTNAME=0.0.0.0
COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
USER node
EXPOSE 3000
CMD ["node", "server.js"]
