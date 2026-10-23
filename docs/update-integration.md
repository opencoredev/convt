# Use the offline update verifier in P8

`convt-update` has no transport or download code. Add the workspace dependency to the app when wiring the consumer. Packaged builds embed the separate `CONVT_UPDATE_PUBKEY`; source builds can have no key. Do not reuse a license key or trust a key advertised by downloaded metadata.

After an opt-in check fetches the envelope bytes, call:

```rust
let trusted = convt_update::public_key().ok_or("update key is not configured")?;
let verified = convt_update::verify(&bytes, &trusted, now_unix_seconds, highest_sequence)?;
let choice = verified.select(
    env!("CARGO_PKG_VERSION"),
    convt_license::BUILD_DATE,
    if license.plan == convt_license::Plan::Desktop {
        convt_license::LIFETIME_UPDATES_UNTIL
    } else {
        &license.updates_until
    },
    "linux-x86_64",
    "AppImage",
)?;
```

Persist `verified.manifest().sequence` as the highest accepted metadata revision. Invalid, expired, future-issued, rolled-back or verification-only manifests produce errors before selection. The API rejects unknown JSON fields and malformed dates, versions, HTTPS links, sizes and hashes. Selection never decreases the running version or build date, including same-day versions. `covered` and `covered_artifact` identify the newest update within the license window. `uncovered` and `purchase_url` supply the purchase state when a newer release is outside coverage. An equal running build produces no update. Choose the installed platform and artifact kind; unsupported platforms return no matching update.

The app owns the update setting, checks at every launch and every five hours while automatic checks are enabled, timeout, byte limit, persisted sequence and error presentation. It may download a covered artifact in the background, then checks the received byte count and SHA-256 before offering installation. Update discovery must not install or download document packs. About should link to the selected build's matching `source.url`. This note supplies the API contract and does not change app code or claim that its HTTP integration has been tested.

Paid Desktop licenses are lifetime entitlements. New and reissued Desktop keys use
`9999-12-31` as their signed update date; the clients also treat legacy Desktop keys as
lifetime once verified. Pro keys keep their subscription coverage date.
