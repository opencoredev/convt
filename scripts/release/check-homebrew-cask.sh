#!/bin/bash
# Validate Casks/convt.rb with `brew style` and `brew audit`. Homebrew only
# accepts casks that live in a tap, so this copies the file into a throwaway
# local tap and removes it afterwards.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cask="$root/Casks/convt.rb"
test -f "$cask"
command -v brew >/dev/null

tap=convt/cask-audit
# brew tap-new writes into $(brew --repository)/Library/Taps/convt/homebrew-cask-audit
if brew tap-info "$tap" >/dev/null 2>&1; then
  brew untap "$tap"
fi
brew tap-new --no-git "$tap" >/dev/null
repo=$(brew --repository "$tap")
mkdir -p "$repo/Casks"
cp "$cask" "$repo/Casks/convt.rb"
cleanup() { brew untap "$tap" >/dev/null 2>&1 || true; }
trap cleanup EXIT

brew style --cask "$tap/convt"
brew audit --cask "$tap/convt"
