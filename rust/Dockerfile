# Ripple Rust 이미지(R0 시험용 뼈대). 빌드 문맥은 rust/ 폴더다: docker build -t ripple-rust-check rust
FROM rust:1.99.1-trixie AS build
WORKDIR /src
COPY . .
RUN ./build.sh --release

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data
COPY --from=build /src/target/release/ripple /usr/local/bin/ripple
ENTRYPOINT []
CMD ["ripple", "--help"]
