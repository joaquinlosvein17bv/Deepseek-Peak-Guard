use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init_logger() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,peakguard_core=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
}

/// Sanitiza strings para que no aparezcan tokens o claves sensibles en los logs
pub fn sanitize_log(input: &str) -> String {
    let mut out = input.to_string();
    if let Some(pos) = out.find("Bearer ") {
        let after = &out[pos + 7..];
        let token_len = after.find(' ').unwrap_or(after.len());
        let token = &after[..token_len];
        if !token.is_empty() {
            out = out.replace(token, "********");
        }
    }
    out
}
