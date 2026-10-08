cask "convt" do
  version "0.3.0"
  sha256 "97784e68a1c5a8ae63630608559d1cab681f695f1c714dac222230b57643fe9f"
  url "https://github.com/opencoredev/convt/releases/download/v0.3.0/convt-macos-arm64.dmg",
      verified: "github.com/opencoredev/convt/"
  name "convt"
  desc "Convert files locally without uploading"
  homepage "https://convt.app/"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on arch: :arm64
  depends_on macos: :ventura

  app "convt.app"
  binary "#{appdir}/convt.app/Contents/MacOS/convt"

  uninstall quit: "app.convt.desktop"

  zap trash: [
    "~/Library/Application Support/convt",
    "~/Library/Caches/app.convt.desktop",
    "~/Library/HTTPStorages/app.convt.desktop",
    "~/Library/Logs/convt",
    "~/Library/Preferences/app.convt.desktop.plist",
    "~/Library/Saved Application State/app.convt.desktop.savedState",
  ]
end
