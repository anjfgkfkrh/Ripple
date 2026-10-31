#!/usr/bin/env bash
# CSS(Tailwind) → cargo build 순서로 빌드한다. 인자는 cargo build에 그대로 넘긴다(예: --release).
set -euo pipefail

cd "$(dirname "$0")"
./tools/fetch-tailwind.sh
./tools/tailwindcss -i ripple/styles/input.css -o ripple/static/app.css --minify
cargo build "$@"
