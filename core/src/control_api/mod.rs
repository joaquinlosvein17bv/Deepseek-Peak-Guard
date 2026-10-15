use crate::clock::Clock;
use crate::policy::{OperatingMode, PolicyEngine, StopKey};
use crate::registry::{ActiveSession, ModelRegistry, SessionRegistry};
use crate::schedule::Schedule;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::{get, post, put},
    Json, Router,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

#[derive(Clone)]
pub struct ControlApiState {
    pub policy_engine: Arc<PolicyEngine>,
    pub registry: Arc<ModelRegistry>,
    pub schedules: Arc<HashMap<String, Schedule>>,
    pub session_registry: Arc<SessionRegistry>,
    pub event_tx: broadcast::Sender<String>,
    pub clock: Arc<dyn Clock>,
    pub auth_token: Arc<String>,
    pub shutdown_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub current_time_utc: String,
    pub mode: String,
    pub is_peak: bool,
    pub next_peak_utc: Option<String>,
    pub seconds_to_peak: Option<u64>,
    pub next_off_peak_utc: Option<String>,
    pub coverage: String,
    pub active_sessions_count: usize,
}

#[derive(Deserialize)]
pub struct StopAckRequest {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    pub transition_utc: String,
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct SetModeRequest {
    pub mode: String,
}

#[derive(Deserialize)]
pub struct SessionHeartbeatRequest {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct SessionUnregisterRequest {
    pub session_id: String,
}

pub fn create_control_router(state: ControlApiState) -> Router {
    Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/status", get(handle_status))
        .route("/api/v1/events", get(handle_events))
        .route("/api/v1/adapters/stop-ack", post(handle_stop_ack))
        .route("/api/v1/config/mode", put(handle_set_mode))
        .route("/api/v1/sessions/register", post(handle_session_register))
        .route("/api/v1/sessions/heartbeat", post(handle_session_heartbeat))
        .route("/api/v1/sessions/unregister", post(handle_session_unregister))
        .route("/api/v1/control/shutdown", post(handle_shutdown))
        .with_state(state)
}

fn check_auth(
    state: &ControlApiState,
    headers: &HeaderMap,
    query: Option<&HashMap<String, String>>,
) -> bool {
    let expected = state.auth_token.as_str();
    if expected.is_empty() {
        return true;
    }
    // 1. Cabecera Authorization: Bearer <token>
    if let Some(auth) = headers.get("authorization").and_then(|h| h.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            if token.trim() == expected {
                return true;
            }
        }
    }
    // 2. Parámetro query ?token=... (útil para EventSource de SSE)
    if let Some(q) = query {
        if let Some(token) = q.get("token") {
            if token.trim() == expected {
                return true;
            }
        }
    }
    false
}

async fn handle_health() -> &'static str {
    "OK"
}

async fn handle_status(State(state): State<ControlApiState>) -> Json<StatusResponse> {
    let now = state.clock.now();
    let mode = state.policy_engine.get_mode();

    // Consultar horario de deepseek_direct
    let schedule = state.schedules.get("deepseek_direct_verified");
    let is_peak = schedule.map(|s| s.is_peak(now)).unwrap_or(false);
    let next_peak = schedule.and_then(|s| s.next_peak(now));
    let next_off = schedule.and_then(|s| s.next_off_peak(now));

    // Determinar cobertura dinámica honesta (nunca hardcoded)
    let coverage = {
        let providers = state.registry.get_providers();
        let any_enabled = providers.values().any(|p| p.enabled);
        if !any_enabled {
            "UNCOVERED".to_string()
        } else {
            let direct_enabled = providers
                .get("deepseek_direct")
                .map(|p| p.enabled)
                .unwrap_or(false);
            let go_enabled = providers
                .get("opencode_go")
                .map(|p| p.enabled)
                .unwrap_or(false);

            if direct_enabled {
                "PROTECTED".to_string()
            } else if go_enabled {
                "UNVERIFIED".to_string()
            } else {
                "PARTIAL".to_string()
            }
        }
    };

    let active_sessions_count = state.session_registry.count_active();

    Json(StatusResponse {
        current_time_utc: now.to_rfc3339(),
        mode: mode.as_str().to_string(),
        is_peak,
        next_peak_utc: next_peak.map(|(dt, _)| dt.to_rfc3339()),
        seconds_to_peak: next_peak.map(|(_, s)| s),
        next_off_peak_utc: next_off.map(|(dt, _)| dt.to_rfc3339()),
        coverage,
        active_sessions_count,
    })
}

async fn handle_stop_ack(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<StopAckRequest>,
) -> Response {
    if !check_auth(&state, &headers, None) {
        warn!("Intento no autorizado de enviar Stop ACK (Bearer Token ausente o incorrecto).");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized", "message": "Bearer token inválido"})),
        )
            .into_response();
    }

    let key = StopKey {
        session_id: payload.session_id.clone(),
        turn_id: payload.turn_id.clone().unwrap_or_else(|| "default".to_string()),
        transition_utc: payload.transition_utc.clone(),
    };

    if payload.success {
        info!(
            "Stop ACK exitoso confirmado por adaptador: session_id='{}', turn_id='{:?}'",
            payload.session_id, payload.turn_id
        );
        state.policy_engine.acknowledge_stop(key);
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "acknowledged",
                "success": true,
                "session_id": payload.session_id
            })),
        )
            .into_response()
    } else {
        warn!(
            "Stop ACK con FALLO reportado por adaptador: session_id='{}', error='{:?}'. Stop NO será marcado como exitoso.",
            payload.session_id, payload.error
        );
        let _ = state.event_tx.send(format!(
            "{{\"type\":\"stop_failed\",\"session_id\":\"{}\",\"error\":\"{}\"}}",
            payload.session_id,
            payload.error.unwrap_or_else(|| "Abort failed".to_string())
        ));
        (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "status": "failed",
                "success": false,
                "session_id": payload.session_id,
                "fallback": "gateway_cancel"
            })),
        )
            .into_response()
    }
}

async fn handle_set_mode(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<SetModeRequest>,
) -> Response {
    if !check_auth(&state, &headers, None) {
        warn!("Intento no autorizado de cambiar modo de protección (Bearer Token ausente o incorrecto).");
        return (
            StatusCode::UNAUTHORIZED,
            "No autorizado: Bearer token requerido",
        )
            .into_response();
    }

    if let Some(new_mode) = OperatingMode::parse(&payload.mode) {
        state.policy_engine.set_mode(new_mode);
        info!("Modo cambiado a '{}' vía Control API.", new_mode.as_str());
        let _ = state
            .event_tx
            .send(format!("{{\"type\":\"mode_changed\",\"mode\":\"{}\"}}", new_mode.as_str()));
        (StatusCode::OK, "Modo actualizado").into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Modo inválido").into_response()
    }
}

async fn handle_session_register(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<ActiveSession>,
) -> Response {
    if !check_auth(&state, &headers, None) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    info!(
        "Sesión registrada en Control API: id='{}', cliente='{}', modelo='{}'",
        payload.session_id, payload.client_type, payload.model
    );
    state.session_registry.register(payload);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "registered" })),
    )
        .into_response()
}

async fn handle_session_heartbeat(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<SessionHeartbeatRequest>,
) -> Response {
    if !check_auth(&state, &headers, None) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    let updated = state.session_registry.heartbeat(
        &payload.session_id,
        payload.turn_id,
        payload.is_active,
    );
    if updated {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "heartbeat_updated" })),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "session_not_found" })),
        )
            .into_response()
    }
}

async fn handle_session_unregister(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<SessionUnregisterRequest>,
) -> Response {
    if !check_auth(&state, &headers, None) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    state.session_registry.unregister(&payload.session_id);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "unregistered" })),
    )
        .into_response()
}

async fn handle_shutdown(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
) -> Response {
    if !check_auth(&state, &headers, None) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    info!("Petición autorizada de apagado ordenada recibida vía Control API.");
    if let Some(ref tx) = state.shutdown_tx {
        let _ = tx.send(()).await;
    }
    (StatusCode::OK, Json(serde_json::json!({ "status": "shutting_down" }))).into_response()
}

async fn handle_events(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if !check_auth(&state, &headers, Some(&query)) {
        warn!("Intento no autorizado de suscribirse a SSE en Control API.");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut rx = state.event_tx.subscribe();
    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            yield Ok(Event::default().data(msg));
        }
    };
    Ok(Sse::new(stream))
}

pub fn resolve_or_create_control_token(token_env_var: &str) -> String {
    if let Ok(val) = std::env::var(token_env_var) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    // Revisar o crear archivo ~/.peakguard/control.token
    let token_dir = match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        Ok(home) => std::path::PathBuf::from(home).join(".peakguard"),
        Err(_) => std::path::PathBuf::from(".peakguard"),
    };
    let token_file = token_dir.join("control.token");
    if let Ok(content) = std::fs::read_to_string(&token_file) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let token = format!("pg_{:x}{:x}{:08x}", pid, now, (now ^ 0x5a5a5a5a) as u32);
    let _ = std::fs::create_dir_all(&token_dir);
    let _ = std::fs::write(&token_file, &token);
    token
}
