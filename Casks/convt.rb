cask "convt" do
  version "0.4.0"
  sha256 "6856fd3e75368c06988b0a92dd5202245a4177efe0b8cb9e894cf4a98891f0ac"

  url "https://github.com/opencoredev/convt/releases/download/v#{version}/convt-macos-arm64.dmg",
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
