# Release convt

P11 builds signed release metadata and reproducible Linux artifacts. Publication requires a passing `source-audit.json` for every included platform. Verification mode always marks its manifest unavailable for distribution.

## Build and compare

Use Rust 1.95.0, Bun 1.4.2, Docker and the pinned packaging inputs. No host sudo is needed. From the checkout:

```sh
export PATH=$HOME/.cargo/bin:$HOME/.bun/bin:$PATH CONVT_LICENSE_STORE=file
export SOURCE_DATE_EPOCH=1791331200
bun run update:keygen /path/outside/checkout/update.seed
export CONVT_UPDATE_SIGNING_KEY=/path/outside/checkout/update.seed
python3 scripts/release/source.py snapshot /tmp/convt-release-source
export CONVT_RELEASE_SOURCE_TREE=/tmp/convt-release-source
bun run release:linux --verification-only packaging/out/rebuild-a
bun run release:linux --verification-only packaging/out/rebuild-b
python3 scripts/release/compare.py packaging/out/rebuild-a/0.1.0 packaging/out/rebuild-b/0.1.0
bash scripts/release/rebuild-cli.sh packaging/out/rebuild-a/0.1.0/convt-0.1.0-source.tar.gz
```

Choose one fixed UTC timestamp for the release. The example is 2026-10-07 UTC, Wednesday. `CONVT_BUILD_DATE` must be that timestamp's UTC date. The date becomes the license coverage cutoff, so never backdate a rebuilt release to fit an expired license. The package version comes from `[workspace.package].version` in Cargo.toml and must use `MAJOR.MINOR.PATCH`. Increase the version for any changed published payload. Existing output directories are refused.

Freeze once when agents share the checkout. The snapshot includes tracked and unignored source changes, excludes caches and private environment files, and records file hashes, symlink targets and permission bits in `release-tree.json`. Both builds use that snapshot. `CONVT_RELEASE_BUILD_WORK` optionally retains a private compilation directory; use distinct directories for independent clean rebuilds. Normal CLI source builds remain unrestricted.

Each version directory contains the Linux payload tarball, AppImage, deb, rpm, source archive, website manifest, signed update envelope, source audit and package-manager staging templates. The pipeline only prints the upload plan. `--verification-only` signs with a temporary key if no external key was supplied, sets `distribution_ready=false`, and cannot be uploaded.

Production `--dry-run` additionally requires `CONVT_LICENSE_PUBKEY`, `CONVT_UPDATE_PUBKEY` and `CONVT_UPDATE_SIGNING_KEY`. It enforces licenses and fails on missing corresponding sources or notices. These gates are implementation checks, not environment flags that can be waived.

## Corresponding source

The Linux source audit validates the frozen convt tree against the exact FFmpeg 7.0.2 and codec sources, Ubuntu patches, matching native source RPMs and GCC recipes, the source-built AppImage runtime closure, and the exact PDFium revision, DEPS archives and builder scripts. Hash validation ties those corresponding sources to the actual payload and AppImage runtime. The public source archive contains the compact frozen checkout; generated audit staging under `third-party` is excluded from that archive and remains represented by `source-audit.json`. General compiler binaries and SDKs remain declared build prerequisites.

The audit's Rust closure under `third-party/rust` covers all Linux CLI and app compilation units, including build dependencies and proc macros. Archive-only Cargo manifests and the lockfile are derived to exclude unrelated platform packages. `third-party/linux-source-scope` preserves original metadata, package pins, patches, source hashes and the identical before/after Cargo unit graph. Upstream source bytes remain unchanged. The SDK-derived objc2 sources retain their exact source archives, upstream `LICENSE.md` declaration and Apple SDK caveat, authors/copyright lines, and canonical SPDX terms reproduced from the hash-pinned SPDX license-list-data. The release audit retains target-specific inventories for `aarch64-apple-darwin` and `x86_64-apple-darwin`, including those crates.

`scripts/release/rebuild-cli.sh` rebuilds and runs the archived CLI in a clean, network-disabled container with empty Cargo and target directories. It mounts only the Rust compiler as a declared prerequisite and checks a real SVG-to-PNG conversion. The local release pipeline runs that actual archive check before generating manifests. The source audit also compiles the full derived CLI/app release graph with empty caches before creating the archive.

The Mac release is Apple silicon (arm64) only and uses FFmpeg built from [pinned sources](../packaging/release/macos-source-notes.md). No x86_64 slice is built or shipped. `packaging/macos/release-status.json` lists what blocks Mac artifacts, and the audit copies it into `platform_gaps`. A Linux-ready source audit cannot authorize Mac or Windows artifacts. Adding an uncovered platform makes the manifest unavailable for distribution.

Document packs are separate artifacts. They require matching sources, pinned platform digests and explicit installation. Update discovery must never fetch them. [The document-pack contract](document-pack.md) owns that flow.

## Keys and manifests

`bun run update:keygen PATH` writes a base64url 32-byte Ed25519 seed with mode 0600 outside the repository. It refuses existing paths and repository paths, including symlinked parent directories. Store the production seed in a password manager. The license signing seed stays with billing. Use a different update seed and public key. Package clients embed `CONVT_UPDATE_PUBKEY`; this is a trust root and must never come from the downloaded manifest.

The website consumes [manifest.schema.json](../packaging/release/manifest.schema.json). Each build has its version, UTC build date, one artifact record per platform/kind and matching source record. Records contain HTTPS URLs, byte sizes and lowercase SHA-256 digests. The publisher writes the unsigned public manifest as `release-manifest.json` and the signed updater envelope as `update-manifest.json`; both names are release assets on every successful publication. Merge notarized Mac and any successful Windows artifacts into the same version directory before final generation. Omit platforms with no artifacts and show them as unavailable. Never expose a download when `distribution_ready` is false. Historical builds remain available with their own source so lapsed licenses can select the newest covered build.

`bun run release:manifest generate DIR VERSION DATE BASE_URL [HISTORY_JSON]` writes the website JSON. `SOURCE_DATE_EPOCH` supplies `issued_at`; `CONVT_MANIFEST_SEQUENCE` defaults to that epoch and must increase on metadata revisions. `CONVT_MANIFEST_EXPIRES` defaults to 90 days after issuance. Refresh and re-sign metadata before expiry even if there is no new application build. CI accepts the previous signed update envelope as `history_url`, authenticates it with the compiled update public key, and preserves its builds at both generation stages. A production first release requires explicit `initial_release` confirmation. Preserve historical covered builds when refreshing it. `bun run release:manifest sign MANIFEST EXTERNAL_SEED OUTPUT` writes the signed envelope.

The envelope contains `payload` and `signature`, both unpadded base64url. Payload is the exact JSON byte sequence. Signatures cover the ASCII bytes of `convt-update-v1\n` followed by the base64url payload. This domain separates update signatures from license signatures. No JSON canonicalization is needed because verifiers authenticate the encoded bytes before parsing.

## Signing and publishing

Compare unsigned artifacts first. `scripts/release/sign-linux.sh UNSIGNED SIGNED` signs copies with `debsigs` and `rpmsign`, using `CONVT_REPO_SIGNING_KEY_ID`. Regenerate checksums and manifests from signed copies because signatures change bytes. The apt layout has pool packages, Packages, Packages.gz and Release; sign Release into InRelease and Release.gpg. The dnf template signs packages, creates deterministic repodata with `createrepo_c`, then signs repomd.xml. Homebrew and winget manifests are rendered only when the matching Mac or Windows artifact exists. Their unfilled templates live in `packaging/release`. The tap users install is `Casks/convt.rb` in this repository (`brew tap opencoredev/convt https://github.com/opencoredev/convt`). After a successful GitHub release, the `Update Homebrew cask` job rewrites that file from `release-manifest.json` and pushes it. The job is `continue-on-error: true`, so a tap or branch-protection failure never blocks publication. Set repository secret `HOMEBREW_TAP_TOKEN` (a PAT that can write `opencoredev/homebrew-tap`) to also publish `brew install --cask opencoredev/tap/convt`.

`bun run release:upload --dry-run VERSION_DIRECTORY` makes no network writes. `--upload` is a separate operation, rejects failed source gates and mismatched artifact bytes, and needs a bucket created by Leo plus account ID and scoped R2 S3 credentials. Account and bucket management use the `cf` CLI. Object transfer uses the S3-compatible endpoint and an atomic If-None-Match precondition, with a 5 GiB single-object limit, instead of the cf REST object's 300 MB limit. Split larger corresponding-source deliveries before publication. This upload path has not been exercised against an account.

The uploader stages immutable version objects. It does not move a stable update pointer. Publish immutable artifacts and their matching source first, verify the objects and signatures, then atomically replace the stable manifests. Never delete a covered historical release. Publish source links from the download page, About and server API for the matching build. The web, app and server owners implement those consumers.

The three manual GitHub Actions workflows are written locally. They are not pushed or run. Linux produces two builds and compares them; macOS calls the Mac-owned arm64 bundle/sign/notarize recipes; Windows is optional and remains excluded until its native dependencies, source notices and installer signing gates pass. Successful authentication or signing alone does not bypass publication gates.

## Launch checklist

- Confirm the workspace version, UTC epoch and embedded build date agree across platforms. Rebuild twice and compare unsigned hashes.
- Close every source and notice gap, retain replaceable LGPL libraries and required relinking material, and review codec patent obligations separately.
- Rebuild the source CLI offline and exercise bundled engine conversions on each supported OS.
- Generate separate license and update production keys, verify the embedded public keys, and store signing seeds outside the checkout.
- Provision the public GitHub repository, R2 release bucket and download domain. Leo owns bucket creation and DNS approval.
- Store scoped R2 S3 credentials as `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`. Set `CONVT_R2_ACCOUNT_ID` and `CONVT_R2_BUCKET` for the uploader. Keep these credentials outside the checkout; no live upload has been verified.
- Provision Apple Developer membership, Developer ID Application certificate with private key, team ID and App Store Connect API key. GitHub secrets: APPLE_DEVELOPER_ID_P12, APPLE_DEVELOPER_ID_PASSWORD, APPLE_KEYCHAIN_PASSWORD and APPLE_API_KEY_P8. Variables: APPLE_SIGNING_IDENTITY, APPLE_TEAM_ID, APPLE_API_KEY_ID and APPLE_API_ISSUER_ID.
- Set CONVT_UPDATE_SIGNING_KEY and CONVT_REPO_SIGNING_KEY secrets; CONVT_LICENSE_PUBKEY, CONVT_UPDATE_PUBKEY and CONVT_REPO_SIGNING_KEY_ID variables. Do not put the license signing seed in release CI.
- Verify signatures and source links, sign Linux repository metadata, review generated Homebrew/winget manifests, and retain all matching sources beside binaries.
- Refresh manifest expiry on schedule and test covered, uncovered, expired and rollback states before moving stable pointers.

## Rollback

A faulty release is removed from future recommendations by issuing a higher-sequence signed manifest that omits it. Keep its immutable downloads and matching source available. The client never offers a lower version or earlier build date than the running build; therefore an automatic downgrade is intentionally impossible. Ship a corrective build with a higher version and date. A user may explicitly install an older retained build, but update discovery cannot perform that rollback. Compromised update-key recovery requires a new trust root shipped through independently verified installers; a remote manifest cannot replace its own root of trust.
