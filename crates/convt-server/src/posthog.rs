//! Best-effort server exception capture. It never participates in a response.
use serde_json::json;

pub fn capture(error: &str, context: &str) {
    let Ok(key) = std::env::var("POSTHOG_KEY") else {
        return;
    };
    let scrub = |s: &str| {
        let mut out = s.replace("Bearer ", "Bearer <redacted>");
        while let Some(at) = out.find('@') {
            let start = out[..at].rfind([' ', '\n']).map_or(0, |i| i + 1);
            out.replace_range(start..at + 1, "<email>");
        }
        out
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
