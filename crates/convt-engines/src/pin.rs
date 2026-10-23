//! What makes a document-pack pin usable, shared by `build.rs` (which refuses
//! a packaged build without one) and the tests.

/// The `CONVT_DOCUMENT_PACK_*` values a build was given.
#[derive(Debug, Default, Clone, Copy)]
pub struct Pin<'a> {
    pub url: Option<&'a str>,
    pub sha256: Option<&'a str>,
    pub size: Option<&'a str>,
    pub version: Option<&'a str>,
}

/// Why a packaged build can't ship with this pin, if it can't. Packaged
/// builds must offer document support: a release URL (or the Windows
/// bundled archive), a SHA-256, the download size for onboarding, and a
/// version for Settings.
pub fn release_problem(pin: Pin) -> Option<String> {
    let url = pin.url.unwrap_or("");
    let secure = url.starts_with("https://") && !url.contains("documents.invalid");
    if !(secure || url == "bundle:documents.tar.gz") {
        return Some(format!(
            "CONVT_DOCUMENT_PACK_URL must be the published https:// pack, got {url:?}"
        ));
    }
    let sha = pin.sha256.unwrap_or("");
    if !(sha.len() == 64 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))) {
        return Some("CONVT_DOCUMENT_PACK_SHA256 must be 64 lowercase hex digits".into());
    }
    if pin.size.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) == 0 {
        return Some("CONVT_DOCUMENT_PACK_SIZE must be the archive's size in bytes".into());
    }
    match pin.version.map(str::trim) {
        None | Some("" | "unconfigured") => {
            Some("CONVT_DOCUMENT_PACK_VERSION must name the pack's LibreOffice".into())
        }
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "70c9e2b08fd2812f78f8898d8997cfd00aeeb47ebfb4b21579897ca0bdf9d09e";

    fn pin(url: &'static str) -> Pin<'static> {
        Pin {
            url: Some(url),
            sha256: Some(SHA),
            size: Some("298475146"),
            version: Some("LibreOffice 25.8.7 macOS arm64"),
        }
    }

    #[test]
    fn a_published_pack_passes() {
        let url = "https://github.com/opencoredev/convt/releases/download/v0.6.0/convt-0.6.0-macos-arm64-documents.tar.gz";
        assert_eq!(release_problem(pin(url)), None);
        assert_eq!(release_problem(pin("bundle:documents.tar.gz")), None);
    }

    #[test]
    fn a_build_without_a_pack_fails() {
        let problem = release_problem(Pin::default()).unwrap();
        assert!(problem.contains("CONVT_DOCUMENT_PACK_URL"), "{problem}");
        for url in [
            "https://documents.invalid/PLACEHOLDER/documents.tar.gz",
            "http://example.com/documents.tar.gz",
            "file:///tmp/documents.tar.gz",
        ] {
            assert!(release_problem(pin(url)).is_some(), "{url}");
        }
    }

    #[test]
    fn every_part_of_the_pin_is_required() {
        let url = "https://example.com/documents.tar.gz";
        for (pin, needs) in [
            (
                Pin {
                    sha256: None,
                    ..pin(url)
                },
                "SHA256",
            ),
            (
                Pin {
                    sha256: Some(&SHA[1..]),
                    ..pin(url)
                },
                "SHA256",
            ),
            (
                Pin {
                    sha256: Some(
                        "70C9E2B08FD2812F78F8898D8997CFD00AEEB47EBFB4B21579897CA0BDF9D09E",
                    ),
                    ..pin(url)
                },
                "SHA256",
            ),
            (
                Pin {
                    size: None,
                    ..pin(url)
                },
                "SIZE",
            ),
            (
                Pin {
                    size: Some("0"),
                    ..pin(url)
                },
                "SIZE",
            ),
            (
                Pin {
                    version: Some("unconfigured"),
                    ..pin(url)
                },
                "VERSION",
            ),
            (
                Pin {
                    version: None,
                    ..pin(url)
                },
                "VERSION",
            ),
        ] {
            let problem = release_problem(pin).unwrap();
            assert!(problem.contains(needs), "{needs}: {problem}");
        }
    }
}
