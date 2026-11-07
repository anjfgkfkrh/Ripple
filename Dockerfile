# syntax=docker/dockerfile:1
# 이미지는 하나다: Rust 실행 파일 `ripple`(14단계 R13). compose의 migrate·worker·web이 같은 이미지를 쓰고, 명령(command)만 다르다.

# ── 빌드: Tailwind CSS와 Rust 실행 파일 ──────────────
FROM rust:1.99.1-trixie AS build
WORKDIR /src
COPY rust/ ./
RUN ./build.sh --release

# ── 실행 ────────────────────────────────────────────
FROM debian:trixie-slim AS ripple
# 운영 확인용: sqlite3는 본 DB를 읽기 전용으로 조회하고 백업하며, curl은 worker 내부 API를 부른다(화면에 쓰는 것은 앱뿐).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl sqlite3 \
    && rm -rf /var/lib/apt/lists/* \
    # 기존 볼륨(/data)과 같은 uid·gid로 DB 파일을 만든다(web이 -shm·-wal을 읽어야 한다). 이름은 예전 node 이미지와 같다.
    && groupadd --gid 1000 node \
    && useradd --uid 1000 --gid node --create-home node \
    && mkdir -p /data && chown node:node /data
COPY --from=build /src/target/release/ripple /usr/local/bin/ripple
USER node
CMD ["ripple", "worker"]
