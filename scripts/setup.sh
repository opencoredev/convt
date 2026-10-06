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

# sqlx-cli for `cargo sqlx prepare` (convt-server's offline query data). Keep its
# version equal to the sqlx crate in Cargo.toml; it needs only Postgres over rustls.
SQLX_VERSION=0.9.0
if ! cargo install --list | grep -q "^sqlx-cli v$SQLX_VERSION:"; then
  cargo install sqlx-cli --version "$SQLX_VERSION" --locked \
    --no-default-features --features postgres,rustls
fi
echo "Done. Try: cargo run -p convt-cli -- formats"
