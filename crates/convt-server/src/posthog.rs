//! Best-effort server exception capture. It never participates in a response.
use serde_json::json;

fn scrub(input: &str) -> String {
    let mut redact_next = false;
    input
        .split_whitespace()
        .map(|word| {
            if redact_next {
                redact_next = false;
                return "<redacted>".to_string();
            }
            if word.eq_ignore_ascii_case("Bearer") {
                redact_next = true;
                return "Bearer".to_string();
            }
            let trimmed = word.trim_matches(|c: char| c == ',' || c == ';' || c == ')' || c == ']');
            if trimmed.contains('@')
                && trimmed
                    .rsplit_once('@')
                    .is_some_and(|(_, domain)| domain.contains('.'))
            {
                word.replacen(trimmed, "<email>", 1)
            } else if trimmed.to_ascii_lowercase().starts_with("token=")
                || trimmed.to_ascii_lowercase().starts_with("api_key=")
                || trimmed.to_ascii_lowercase().starts_with("x-api-key=")
                || trimmed.to_ascii_lowercase().starts_with("access_token=")
                || trimmed.to_ascii_lowercase().starts_with("refresh_token=")
                || trimmed.to_ascii_lowercase().starts_with("license_key=")
                || trimmed.to_ascii_lowercase().starts_with("secret=")
            {
                "<credential>=<redacted>".to_string()
            } else if trimmed.to_ascii_lowercase().starts_with("cvt_")
                || trimmed.to_ascii_lowercase().starts_with("convt_")
                || (trimmed.len() >= 19
                    && trimmed.matches('-').count() >= 3
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-'))
            {
                "<license-key>".to_string()
            } else if trimmed.starts_with('/')
                || trimmed.contains("\\")
                || (trimmed.contains('.')
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'))
            {
                "<path>".to_string()
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn capture(error: &str, context: &str) {
    if std::env::var("POSTHOG_ERRORS").ok().as_deref() == Some("0") {
        return;
    }
    let Ok(key) = std::env::var("POSTHOG_KEY") else {
        return;
    };
    let body = json!({"api_key":key,"event":"$exception","properties":{"$lib":"convt-server","$lib_version":env!("CARGO_PKG_VERSION"),"error_context":context,"$exception_list":[{"type":"Error","value":scrub(error),"stacktrace":{"raw":scrub(error),"frames":[]}}]}});
    tokio::spawn(async move {
        let _ = reqwest::Client::new()
            .post("https://us.i.posthog.com/capture/")
            .json(&body)
            .send()
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::scrub;

    #[test]
    fn scrubs_tokens_and_complete_emails() {
        let mailbox = format!("{}@{}", "user", "gmail.com");
        let value = scrub(
            &format!(
                "/Users/alice/input.pdf Bearer secret-token {mailbox} license_key=cvt_PROD_12345678 token=api-secret"
            ),
        );
        assert_eq!(
            value,
            "<path> Bearer <redacted> <email> <credential>=<redacted> <credential>=<redacted>"
        );
        assert!(
            !value.contains("alice")
                && !value.contains("secret-token")
                && !value.contains("gmail.com")
        );
    }
}
