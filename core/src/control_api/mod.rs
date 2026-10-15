use crate::policy::{OperatingMode, PolicyEngine, StopKey};
use crate::registry::ModelRegistry;
use crate::schedule::Schedule;
use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

#[derive(Clone)]
pub struct ControlApiState {
    pub policy_engine: Arc<PolicyEngine>,
    pub registry: Arc<ModelRegistry>,
    pub schedules: Arc<HashMap<String, Schedule>>,
    pub event_tx: broadcast::Sender<String>,
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
}

#[derive(Deserialize)]
pub struct StopAckRequest {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub transition_utc: String,
}

#[derive(Deserialize)]
pub struct SetModeRequest {
    pub mode: String,
}

pub fn create_control_router(state: ControlApiState) -> Router {
    Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/status", get(handle_status))
        .route("/api/v1/events", get(handle_events))
        .route("/api/v1/adapters/stop-ack", post(handle_stop_ack))
        .route("/api/v1/config/mode", put(handle_set_mode))
        .with_state(state)
}

async fn handle_health() -> &'static str {
    "OK"
}

async fn handle_status(State(state): State<ControlApiState>) -> Json<StatusResponse> {
    let now = Utc::now();
    let mode = state.policy_engine.get_mode();

    // Consultar horario de deepseek_direct
    let schedule = state.schedules.get("deepseek_direct_verified");
    let is_peak = schedule.map(|s| s.is_peak(now)).unwrap_or(false);
    let next_peak = schedule.and_then(|s| s.next_peak(now));
    let next_off = schedule.and_then(|s| s.next_off_peak(now));

    Json(StatusResponse {
        current_time_utc: now.to_rfc3339(),
        mode: mode.as_str().to_string(),
        is_peak,
        next_peak_utc: next_peak.map(|(dt, _)| dt.to_rfc3339()),
        seconds_to_peak: next_peak.map(|(_, s)| s),
        next_off_peak_utc: next_off.map(|(dt, _)| dt.to_rfc3339()),
        coverage: "PROTEGIDO".to_string(),
    })
}

async fn handle_stop_ack(
    State(state): State<ControlApiState>,
    Json(payload): Json<StopAckRequest>,
) -> StatusCode {
    info!(
        "Stop ACK recibido del adaptador: session_id='{}', turn_id='{:?}'",
        payload.session_id, payload.turn_id
    );

    let key = StopKey {
        session_id: payload.session_id,
        turn_id: payload.turn_id.unwrap_or_else(|| "default".to_string()),
        transition_utc: payload.transition_utc,
    };

    state.policy_engine.acknowledge_stop(key);
    StatusCode::OK
}

async fn handle_set_mode(
    State(state): State<ControlApiState>,
    Json(payload): Json<SetModeRequest>,
) -> (StatusCode, &'static str) {
    if let Some(new_mode) = OperatingMode::parse(&payload.mode) {
        state.policy_engine.set_mode(new_mode);
        info!("Modo cambiado a '{}' vía Control API.", new_mode.as_str());
        let _ = state
            .event_tx
            .send(format!("{{\"type\":\"mode_changed\",\"mode\":\"{}\"}}", new_mode.as_str()));
        (StatusCode::OK, "Modo actualizado")
    } else {
        (StatusCode::BAD_REQUEST, "Modo inválido")
    }
}

async fn handle_events(
    State(state): State<ControlApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_tx.subscribe();
    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            yield Ok(Event::default().data(msg));
        }
    };
    Sse::new(stream)
}
