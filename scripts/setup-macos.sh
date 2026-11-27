#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
  echo "This setup script is for macOS. See docs/platform/ for Windows and Linux."
  exit 1
fi

xcode-select -p >/dev/null
for tool in cargo rustc node pnpm cmake; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing $tool. Install the prerequisites in docs/platform/macos.md and run this script again."
    exit 1
  fi
done

pnpm install --frozen-lockfile
pnpm -C apps/desktop build
cargo build --workspace
echo "Ready. Run pnpm dev, or CI=true pnpm build to create a macOS app and DMG."
echo "Download local models with: cargo run -p parlance-cli -- download whisper-base-q8 silero-vad-5.1.2 qwen3.5-2b-q4 llama-server"
