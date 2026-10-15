use crate::clock::Clock;
use crate::policy::{PolicyDecision, PolicyEngine};
use crate::registry::ModelRegistry;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Clone)]
pub struct GatewayState {
    pub policy_engine: Arc<PolicyEngine>,
    pub registry: Arc<ModelRegistry>,
    pub clock: Arc<dyn Clock>,
    pub http_client: reqwest::Client,
    pub event_tx: Option<tokio::sync::broadcast::Sender<String>>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ChatCompletionRequest {
    model: String,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Serialize)]
struct PolicyBlockedResponse {
    error: PolicyBlockedDetails,
}

#[derive(Serialize)]
struct PolicyBlockedDetails {
    code: &'static str,
    message: String,
    provider_id: String,
    model_id: String,
    mode: &'static str,
    next_off_peak_utc: Option<String>,
    retry_automatically: bool,
}

pub fn create_gateway_router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(handle_chat_completions))
        .route("/chat/completions", post(handle_chat_completions))
        .route("/v1/models", get(handle_list_models))
        .route("/models", get(handle_list_models))
        .route("/health", get(handle_health))
        .with_state(state)
}

async fn handle_health() -> &'static str {
    "OK"
}

async fn handle_list_models() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            { "id": "deepseek-chat", "object": "model", "owned_by": "deepseek" },
            { "id": "deepseek-reasoner", "object": "model", "owned_by": "deepseek" },
            { "id": "deepseek-v4.1-flash", "object": "model", "owned_by": "opencode-go" }
        ]
    }))
}

async fn handle_chat_completions(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // 1. Extraer encabezados de sesión y parsear body mínimo
    let session_header = headers
        .get("x-session-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let payload: Result<ChatCompletionRequest, _> = serde_json::from_slice(&body);
    let (model_name, session_id) = match payload {
        Ok(req) => (req.model, req.session_id.or(session_header)),
        Err(e) => {
            warn!("Payload JSON mal formado en petición de completions: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "message": "JSON mal formado", "type": "invalid_request_error" }
                })),
            )
                .into_response();
        }
    };

    let now = state.clock.now();

    // 2. Evaluar política horaria ANTES de conectar al upstream
    let decision = state.policy_engine.evaluate_request(
        &model_name,
        session_id.as_deref(),
        None,
        now,
        true, // pasa por el gateway
    );

    let (reason, next_off_peak) = match decision {
        PolicyDecision::Block {
            reason,
            next_off_peak_utc,
        } => (reason, next_off_peak_utc),
        PolicyDecision::StopRequired {
            reason,
            session_id: sess_id,
            turn_id,
        } => {
            if let Some(ref tx) = state.event_tx {
                let _ = tx.send(format!(
                    "{{\"type\":\"stop_requested\",\"session_id\":\"{}\",\"turn_id\":{},\"transition_utc\":\"{}\",\"reason\":\"{}\"}}",
                    sess_id,
                    turn_id.as_ref().map(|t| format!("\"{}\"", t)).unwrap_or_else(|| "null".to_string()),
                    now.to_rfc3339(),
                    reason
                ));
            }
            (reason, None)
        }
        PolicyDecision::Allow | PolicyDecision::NotifyOnly { .. } => {
            // Solicitud permitida: continúa abajo
            (String::new(), None)
        }
    };

    if !reason.is_empty() {
        info!(
            "BLOQUEO DE POLÍTICA: Modelo '{}' restringido por horario peak.",
            model_name
        );

        let provider_id = state
            .registry
            .get_provider_for_model(&model_name)
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let mode_str = state.policy_engine.get_mode().as_str();

        let err_body = PolicyBlockedResponse {
            error: PolicyBlockedDetails {
                code: "peakguard_policy_blocked",
                message: reason,
                provider_id,
                model_id: model_name,
                mode: mode_str,
                next_off_peak_utc: next_off_peak.map(|dt| dt.to_rfc3339()),
                retry_automatically: false,
            },
        };

        // Responder con HTTP 403 Forbidden para evitar reintentos automáticos
        return (StatusCode::FORBIDDEN, Json(err_body)).into_response();
    }


    // 3. Determinar upstream verificado (Allowlist estricta)
    let provider = match state.registry.get_provider_for_model(&model_name) {
        Some(p) => p,
        None => {
            warn!(
                "Petición rechazada: modelo '{}' no tiene un upstream configurado.",
                model_name
            );
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": { "message": "No upstream provider configured for model", "type": "bad_gateway" }
                })),
            )
                .into_response();
        }
    };

    let upstream_base = &provider.upstream_url;
    let target_url = format!("{}/chat/completions", upstream_base.trim_end_matches('/'));

    // 4. Preparar petición hacia el upstream (propagando headers y Authorization)
    let mut req = state.http_client.post(&target_url).body(body.clone());

    for (k, v) in &headers {
        let name = k.as_str();
        // Filtrar headers hop-by-hop
        if name != "host" && name != "content-length" && name != "connection" {
            req = req.header(k, v);
        }
    }

    match req.send().await {
        Ok(upstream_resp) => {
            let status = StatusCode::from_u16(upstream_resp.status().as_u16())
                .unwrap_or(StatusCode::BAD_GATEWAY);
            let resp_headers = upstream_resp.headers().clone();

            // Si es un stream SSE (`stream: true` o `text/event-stream`)
            let is_sse = resp_headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.contains("text/event-stream"))
                .unwrap_or(false);

            if is_sse {
                let mut stream_bytes = upstream_resp.bytes_stream();
                let policy_engine = state.policy_engine.clone();
                let clock = state.clock.clone();
                let model = model_name.clone();
                let event_sender = state.event_tx.clone();
                let sess_id = session_id.clone();

                let wrapped_stream = async_stream::stream! {
                    while let Some(item) = stream_bytes.next().await {
                        // Comprobar política horaria antes de reenviar cada chunk
                        let now = clock.now();
                        let decision = policy_engine.evaluate_request(
                            &model,
                            sess_id.as_deref(),
                            None,
                            now,
                            true,
                        );

                        match decision {
                            PolicyDecision::Block { reason, .. }
                            | PolicyDecision::StopRequired { reason, .. } => {
                                warn!(
                                    "CANCELACIÓN DE STREAM: Modelo '{}' entró en horario peak durante streaming activo. Cancelando conexión.",
                                    model
                                );
                                if let Some(ref tx) = event_sender {
                                    let _ = tx.send(format!(
                                        "{{\"type\":\"stream_cancelled\",\"model\":\"{}\",\"reason\":\"{}\"}}",
                                        model, reason
                                    ));
                                }
                                let err_chunk = format!(
                                    "data: {{\"error\":{{\"code\":\"peakguard_stream_cancelled_peak\",\"message\":\"{}\"}}}}\n\n",
                                    reason
                                );
                                yield Ok::<_, std::io::Error>(Bytes::from(err_chunk));
                                break;
                            }
                            PolicyDecision::NotifyOnly { .. } | PolicyDecision::Allow => {
                                match item {
                                    Ok(b) => yield Ok(b),
                                    Err(e) => {
                                        yield Err(std::io::Error::new(std::io::ErrorKind::Other, e));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                };

                let mut response = axum::response::Response::builder()
                    .status(status)
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .header("connection", "keep-alive")
                    .body(axum::body::Body::from_stream(wrapped_stream))
                    .unwrap();

                for (k, v) in resp_headers {
                    if let Some(name) = k {
                        if name.as_str() != "content-length"
                            && name.as_str() != "transfer-encoding"
                            && name.as_str() != "content-type"
                        {
                            response.headers_mut().insert(name, v);
                        }
                    }
                }
                return response;
            }

            match upstream_resp.bytes().await {
                Ok(resp_bytes) => {
                    let mut response = (status, resp_bytes).into_response();
                    for (k, v) in resp_headers {
                        if let Some(name) = k {
                            if name.as_str() != "content-length" && name.as_str() != "transfer-encoding" {
                                response.headers_mut().insert(name, v);
                            }
                        }
                    }
                    response
                }
                Err(e) => {
                    warn!("Error leyendo respuesta del upstream: {}", e);
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(serde_json::json!({
                            "error": { "message": "Failed reading upstream response" }
                        })),
                    )
                        .into_response()
                }
            }
        }

        Err(e) => {
            warn!("Error conectando con upstream '{}': {}", target_url, e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": { "message": format!("Upstream connection error: {}", e) }
                })),
            )
                .into_response()
        }
    }
}
