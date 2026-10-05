//! Embeds what a build checks licenses against:
//!
//! - `CONVT_LICENSE_PUBKEY`: the base64url Ed25519 public key. Without it, a
//!   local build uses `.convt-dev/license.pub` from the workspace root if it
//!   exists (see `examples/dev-keys.rs`), else it has no key.
//! - `CONVT_LICENSE_ENFORCE=1`: packaged builds turn on the license check.
//!   Builds from source leave it off. Enforcing needs `CONVT_LICENSE_PUBKEY`
//!   so a release never ships with a dev key.
//! - `CONVT_BUILD_DATE` (`YYYY-MM-DD`): the date license update windows are
//!   compared with. Defaults to the UTC day of `SOURCE_DATE_EPOCH`. If both
//!   are set they must name the same day. Enforcing builds need one of them;
//!   only unenforced builds fall back to today.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "src/date.rs"]
mod date;

fn main() {
    for var in [
        "CONVT_LICENSE_PUBKEY",
        "CONVT_LICENSE_ENFORCE",
        "CONVT_BUILD_DATE",
        "SOURCE_DATE_EPOCH",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let dev_key = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.convt-dev/license.pub");
    println!("cargo:rerun-if-changed={}", dev_key.display());

    let release_key = std::env::var("CONVT_LICENSE_PUBKEY")
        .ok()
        .filter(|k| !k.is_empty());
    let enforce = std::env::var("CONVT_LICENSE_ENFORCE").is_ok_and(|v| v == "1");
    if enforce && release_key.is_none() {
        panic!("CONVT_LICENSE_ENFORCE=1 needs CONVT_LICENSE_PUBKEY");
    }
    let key = release_key
        .or_else(|| std::fs::read_to_string(&dev_key).ok())
        .map(|k| k.trim().to_string())
        .unwrap_or_default();

    let epoch_day = std::env::var("SOURCE_DATE_EPOCH").ok().map(|s| {
        let secs: u64 = s
            .parse()
            .unwrap_or_else(|_| panic!("SOURCE_DATE_EPOCH {s:?} is not a Unix time"));
        date::from_days((secs / 86_400) as i64)
    });
    let build_date = match (std::env::var("CONVT_BUILD_DATE").ok(), epoch_day) {
        (Some(d), epoch) => {
            assert!(
                date::to_days(&d).is_some(),
                "CONVT_BUILD_DATE {d:?} is not YYYY-MM-DD"
            );
            if let Some(e) = epoch {
                assert!(
                    e == d,
                    "CONVT_BUILD_DATE {d} disagrees with SOURCE_DATE_EPOCH ({e})"
                );
            }
            d
        }
        (None, Some(e)) => e,
        (None, None) => {
            // The build date is the license update cutoff, so a release must
            // choose it rather than inherit the day it happened to build.
            assert!(
                !enforce,
                "CONVT_LICENSE_ENFORCE=1 needs CONVT_BUILD_DATE or SOURCE_DATE_EPOCH"
            );
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after 1970")
                .as_secs();
            date::from_days((secs / 86_400) as i64)
        }
    };
    println!("cargo:rustc-env=CONVT_EMBEDDED_PUBKEY={key}");
    println!(
        "cargo:rustc-env=CONVT_EMBEDDED_ENFORCE={}",
        u8::from(enforce)
    );
    println!("cargo:rustc-env=CONVT_EMBEDDED_BUILD_DATE={build_date}");
}
