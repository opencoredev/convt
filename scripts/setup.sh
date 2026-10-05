#!/usr/bin/env bash
# One-shot dev setup: system packages, Rust toolchain, JS deps, PDFium.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)" in
  Linux) bash scripts/setup-linux.sh ;;
  Darwin) bash scripts/setup-macos.sh ;;
  *) echo "On Windows run scripts/setup-windows.ps1 in PowerShell." >&2; exit 1 ;;
esac

command -v rustup >/dev/null || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
export PATH="$HOME/.cargo/bin:$PATH"
rustup show active-toolchain >/dev/null  # installs the version in rust-toolchain.toml

command -v bun >/dev/null || curl -fsSL https://bun.sh/install | bash
export PATH="$HOME/.bun/bin:$PATH"
bun install

bash scripts/fetch-pdfium.sh
echo "Done. Try: cargo run -p convt-cli -- formats"
