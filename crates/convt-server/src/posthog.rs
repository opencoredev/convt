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
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn capture(error: &str, context: &str) {
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
        let value = scrub("Bearer secret-token failed for user@gmail.com");
        assert_eq!(value, "Bearer <redacted> failed for <email>");
        assert!(!value.contains("secret-token"));
        assert!(!value.contains("gmail.com"));
    }
}
