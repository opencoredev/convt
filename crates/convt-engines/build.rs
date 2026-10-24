//! Refuses a packaged build that couldn't download document support.
//!
//! Packaged builds set `CONVT_LICENSE_ENFORCE=1` (see `convt-license`'s
//! build script). Such a build must also carry a usable document-pack pin
//! (`CONVT_DOCUMENT_PACK_URL`, `_SHA256`, `_SIZE`, `_VERSION`), or onboarding
//! has nothing to offer and Word, Excel and PowerPoint files can't convert.
//! A local build that enforces licenses only to try the trial can opt out
//! with `CONVT_DOCUMENT_PACK_OPTIONAL=1`; release jobs never set it.

#[path = "src/pin.rs"]
mod pin;

fn main() {
    let vars = [
        "CONVT_LICENSE_ENFORCE",
        "CONVT_DOCUMENT_PACK_OPTIONAL",
        "CONVT_DOCUMENT_PACK_URL",
        "CONVT_DOCUMENT_PACK_SHA256",
        "CONVT_DOCUMENT_PACK_SIZE",
        "CONVT_DOCUMENT_PACK_VERSION",
    ];
    for var in vars {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let get = |name| std::env::var(name).ok().filter(|v| !v.is_empty());
    let packaged = get("CONVT_LICENSE_ENFORCE").as_deref() == Some("1");
    let optional = get("CONVT_DOCUMENT_PACK_OPTIONAL").as_deref() == Some("1");
    if !packaged || optional {
        return;
    }
    let (url, sha256, size, version) = (
        get("CONVT_DOCUMENT_PACK_URL"),
        get("CONVT_DOCUMENT_PACK_SHA256"),
        get("CONVT_DOCUMENT_PACK_SIZE"),
        get("CONVT_DOCUMENT_PACK_VERSION"),
    );
    if let Some(problem) = pin::release_problem(pin::Pin {
        url: url.as_deref(),
        sha256: sha256.as_deref(),
        size: size.as_deref(),
        version: version.as_deref(),
    }) {
        panic!(
            "a packaged build (CONVT_LICENSE_ENFORCE=1) must offer document support: {problem}. \
             Release jobs build and publish the pack first (docs/document-pack.md)."
        );
    }
}
