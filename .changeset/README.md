# Release notes

Run `bun run changeset` in a feature PR and describe what users will notice. Choose a patch for fixes or a minor for new features. The private `@convt/desktop`, `@convt/cli` and `@convt/web` packages form one fixed group; any bump versions all three. The shared billing, database, license and mail packages also have versions so Changesets can resolve the website dependency graph; they are outside the fixed group. The desktop and CLI package files are release metadata for their Rust binaries, not npm packages.

The Version packages workflow keeps one `chore(release): version packages` PR on `changeset-release/main`. It consumes pending changesets, writes changelogs, synchronizes every Rust workspace crate and updates both lockfiles. Merging that PR starts the release pipeline. The only npm package is `@convt/sdk`: a changeset for it bumps `packages/sdk/package.json` in the version PR, and the Publish SDK workflow publishes that version to npm when the PR merges. Changesets creates no package tags.

See [the release guide](../docs/release.md) for runners, signing credentials, publication gates and the website manifest update.
