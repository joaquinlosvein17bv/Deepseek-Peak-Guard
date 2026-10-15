use crate::clock::Clock;
use crate::policy::{OperatingMode, PolicyEngine, StopKey};
use crate::registry::{ActiveSession, ModelRegistry, SessionRegistry};
use crate::schedule::Schedule;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::{get, post, put},
    Json, Router,
};
use futures_util::stream::Stream;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

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
    pub observed_traffic_count: usize,
    pub coverage_details: serde_json::Value,
}

#[derive(Deserialize)]
pub struct StopAckRequest {
    #[serde(default)]
    pub command_id: Option<String>,
    #[serde(default)]
    pub client_instance_id: Option<String>,
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
    #[serde(default)]
    pub client_instance_id: Option<String>,
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct SessionUnregisterRequest {
    #[serde(default)]
    pub client_instance_id: Option<String>,
    pub session_id: String,
}

#[derive(Deserialize)]
pub struct PendingCommandsQuery {
    pub client_instance_id: String,
}

pub fn create_control_router(state: ControlApiState) -> Router {
    Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/status", get(handle_status))
        .route("/api/v1/events", get(handle_events))
        .route("/api/v1/adapters/commands/pending", get(handle_pending_commands))
        .route("/api/v1/adapters/stop-ack", post(handle_stop_ack))
        .route("/api/v1/config/mode", put(handle_set_mode))
        .route("/api/v1/sessions/register", post(handle_session_register))
        .route("/api/v1/sessions/heartbeat", post(handle_session_heartbeat))
        .route("/api/v1/sessions/unregister", post(handle_session_unregister))
        .route("/api/v1/control/shutdown", post(handle_shutdown))
        .with_state(state)
}

/// Validación estricta de autenticación Bearer Token (P0).
/// - Nunca acepta un token esperado vacío (falla cerrado).
/// - Exige cabecera 'Authorization: Bearer <token>'.
/// - Elimina explícitamente tokens en parámetros query por seguridad.
fn check_auth(state: &ControlApiState, headers: &HeaderMap) -> bool {
    let expected = state.auth_token.as_str().trim();
    if expected.is_empty() {
        warn!("Seguridad: auth_token configurado está vacío. Petición rechazada por fail-closed.");
        return false;
    }

    if let Some(auth) = headers.get("authorization").and_then(|h| h.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            let token = token.trim();
            if !token.is_empty() && token == expected {
                return true;
            }
        }
    }

    false
}

async fn handle_health() -> &'static str {
    "OK"
}

/// Evaluación de cobertura dinámica y honesta (Paso 3).
/// No asume protección basándose únicamente en la configuración declarada.
async fn handle_status(State(state): State<ControlApiState>) -> Json<StatusResponse> {
    let now = state.clock.now();
    let mode = state.policy_engine.get_mode();

    // Consultar horario de deepseek_direct
    let schedule = state.schedules.get("deepseek_direct_verified");
    let is_peak = schedule.map(|s| s.is_peak(now)).unwrap_or(false);
    let next_peak = schedule.and_then(|s| s.next_peak(now));
    let next_off = schedule.and_then(|s| s.next_off_peak(now));

    let traffic = state.registry.get_traffic_tracker();
    let total_observed = traffic.get_total_observed();
    let active_sessions = state.session_registry.count_active();

    // Determinar cobertura dinámica honesta (nunca estática)
    let (coverage, coverage_details) = {
        let providers = state.registry.get_providers();
        let any_enabled = providers.values().any(|p| p.enabled);

        if !any_enabled {
            (
                "UNCOVERED".to_string(),
                serde_json::json!({
                    "reason": "no_providers_enabled",
                    "observed_requests": total_observed,
                    "gateway_operational": true,
                }),
            )
        } else {
            let direct_provider = providers.get("deepseek_direct");
            let direct_enabled = direct_provider.map(|p| p.enabled).unwrap_or(false);
            let go_provider = providers.get("opencode_go");
            let go_enabled = go_provider.map(|p| p.enabled).unwrap_or(false);

            let direct_active_sessions = state
                .session_registry
                .count_active_for_provider("deepseek_direct", &state.registry);
            let direct_traffic_observed = traffic.has_observed_provider("deepseek_direct");

            // OpenCode Go gestiona inferencia nativamente y no pasa por el gateway por defecto
            if go_enabled && !direct_enabled {
                (
                    "UNVERIFIED".to_string(),
                    serde_json::json!({
                        "reason": "opencode_go_managed_inference_unverified",
                        "observed_requests": total_observed,
                        "requires_custom_base_url": true,
                        "gateway_operational": true,
                    }),
                )
            } else if direct_enabled {
                // T18.1.1: Solo declarar PROTECTED si el gateway ha intermediado tráfico real de DeepSeek
                // o hay sesión activa conectada ESPECÍFICAMENTE de DeepSeek (no de Claude, GPT u otros)
                if direct_traffic_observed || direct_active_sessions > 0 {
                    (
                        "PROTECTED".to_string(),
                        serde_json::json!({
                            "verified_route": "deepseek_direct",
                            "observed_requests_for_deepseek": total_observed,
                            "active_sessions_for_deepseek": direct_active_sessions,
                            "total_active_sessions": active_sessions,
                            "gateway_operational": true,
                        }),
                    )
                } else {
                    (
                        "PARTIAL".to_string(),
                        serde_json::json!({
                            "reason": "route_configured_awaiting_first_observed_request",
                            "configured_route": "deepseek_direct",
                            "observed_requests_for_deepseek": 0,
                            "active_sessions_for_deepseek": 0,
                            "total_active_sessions": active_sessions,
                            "gateway_operational": true,
                        }),
                    )
                }
            } else {
                (
                    "PARTIAL".to_string(),
                    serde_json::json!({
                        "reason": "provider_enabled_without_verified_route",
                        "observed_requests": total_observed,
                        "gateway_operational": true,
                    }),
                )
            }
        }
    };

    Json(StatusResponse {
        current_time_utc: now.to_rfc3339(),
        mode: mode.as_str().to_string(),
        is_peak,
        next_peak_utc: next_peak.map(|(dt, _)| dt.to_rfc3339()),
        seconds_to_peak: next_peak.map(|(_, s)| s),
        next_off_peak_utc: next_off.map(|(dt, _)| dt.to_rfc3339()),
        coverage,
        active_sessions_count: active_sessions,
        observed_traffic_count: total_observed,
        coverage_details,
    })
}

async fn handle_stop_ack(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<StopAckRequest>,
) -> Response {
    if !check_auth(&state, &headers) {
        warn!("Intento no autorizado de enviar Stop ACK (Bearer Token ausente o incorrecto).");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized", "message": "Bearer token inválido"})),
        )
            .into_response();
    }

    let cmd_mgr = state.session_registry.get_command_manager();
    let ack_res = cmd_mgr.record_ack(
        payload.command_id.as_deref(),
        payload.client_instance_id.as_deref(),
        &payload.session_id,
        payload.turn_id.as_deref(),
        payload.success,
        payload.error.clone(),
    );

    let recorded_cmd = match ack_res {
        Ok(c) => c,
        Err(err_msg) => {
            warn!("Rechazo de Stop ACK por discordancia o falta de autorización: {}", err_msg);
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": err_msg,
                    "session_id": payload.session_id
                })),
            )
                .into_response();
        }
    };

    let key = StopKey {
        session_id: payload.session_id.clone(),
        turn_id: payload.turn_id.clone().unwrap_or_else(|| "default".to_string()),
        transition_utc: payload.transition_utc.clone(),
    };

    if payload.success {
        info!(
            "Stop ACK exitoso confirmado por adaptador: cmd='{}', session_id='{}', turn_id='{:?}'",
            recorded_cmd.command_id, payload.session_id, payload.turn_id
        );
        state.policy_engine.acknowledge_stop(key);
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "acknowledged",
                "success": true,
                "command_id": recorded_cmd.command_id,
                "session_id": payload.session_id
            })),
        )
            .into_response()
    } else {
        warn!(
            "Stop ACK con FALLO reportado por adaptador: cmd='{}', session_id='{}', error='{:?}'. Stop NO será marcado como exitoso.",
            recorded_cmd.command_id, payload.session_id, payload.error
        );
        let _ = state.event_tx.send(format!(
            "{{\"type\":\"stop_failed\",\"command_id\":\"{}\",\"session_id\":\"{}\",\"error\":\"{}\"}}",
            recorded_cmd.command_id,
            payload.session_id,
            payload.error.unwrap_or_else(|| "Abort failed".to_string())
        ));
        (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "status": "failed",
                "success": false,
                "command_id": recorded_cmd.command_id,
                "session_id": payload.session_id,
                "fallback": "gateway_cancel"
            })),
        )
            .into_response()
    }
}

async fn handle_pending_commands(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<PendingCommandsQuery>,
) -> Response {
    if !check_auth(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    let cmd_mgr = state.session_registry.get_command_manager();
    let commands = cmd_mgr.get_pending_commands_for_instance(&query.client_instance_id);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "client_instance_id": query.client_instance_id,
            "commands": commands
        })),
    )
        .into_response()
}

async fn handle_set_mode(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
    Json(payload): Json<SetModeRequest>,
) -> Response {
    if !check_auth(&state, &headers) {
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
    if !check_auth(&state, &headers) {
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
    if !check_auth(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    let updated = state.session_registry.heartbeat(
        payload.client_instance_id.as_deref(),
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
    if !check_auth(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    info!(
        "Sesión desregistrada en Control API: id='{}', inst='{:?}'",
        payload.session_id, payload.client_instance_id
    );
    state.session_registry.unregister(payload.client_instance_id.as_deref(), &payload.session_id);
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
    if !check_auth(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "No autorizado").into_response();
    }
    info!("Petición autorizada de apagado ordenada recibida vía Control API.");
    if let Some(ref tx) = state.shutdown_tx {
        let _ = tx.send(()).await;
    }
    (StatusCode::OK, Json(serde_json::json!({ "status": "shutting_down" }))).into_response()
}

/// Suscripción a eventos SSE mediante transporte HTTP autenticado con cabeceras (P0).
/// No admite parámetros query de token en URL para evitar filtraciones en logs y proxies.
async fn handle_events(
    State(state): State<ControlApiState>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if !check_auth(&state, &headers) {
        warn!("Intento no autorizado de suscribirse a SSE en Control API (Bearer Token ausente o incorrecto).");
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

/// Genera un token criptográficamente seguro utilizando el CSPRNG del sistema operativo (P0).
pub fn generate_secure_token() -> Result<String, Box<dyn std::error::Error>> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes)
        .map_err(|e| format!("Fallo en CSPRNG del sistema operativo: {}", e))?;
    let hex_str: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    Ok(format!("pg_{}", hex_str))
}

/// Carga el token existente desde variable de entorno o archivo, o genera uno nuevo
/// de forma atómica con permisos restringidos.
/// Si falla la generación o guardado, retorna Err para fallar cerrado (P0).
pub fn resolve_or_create_control_token(token_env_var: &str) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(val) = std::env::var(token_env_var) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let token_dir = match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        Ok(home) => std::path::PathBuf::from(home).join(".peakguard"),
        Err(_) => std::path::PathBuf::from(".peakguard"),
    };
    let token_file = token_dir.join("control.token");
    
    // T18.1.5: Distinguir archivo inexistente (NotFound) de errores de lectura/permisos
    match std::fs::read_to_string(&token_file) {
        Ok(content) => {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            } else {
                return Err(format!(
                    "Archivo de token existente está vacío ('{}'). Requiere corrección o eliminación manual (fail-closed).",
                    token_file.display()
                ).into());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // El archivo genuinamente no existe: política explícita de creación inicial
            info!("Archivo de token no existe en '{}'. Creando token nuevo de forma segura...", token_file.display());
        }
        Err(e) => {
            // Error de permisos, bloqueo u otro: FALLAR CERRADO, nunca sobrescribir credencial existente
            error!("Error de I/O o permisos accediendo a '{}': {}", token_file.display(), e);
            return Err(format!("Error accediendo al archivo de token existente ('{}'): {}", token_file.display(), e).into());
        }
    }

    let token = generate_secure_token()?;
    std::fs::create_dir_all(&token_dir)?;

    // Escritura atómica para evitar condiciones de carrera y escrituras parciales
    let tmp_file = token_dir.join(format!("control.token.tmp.{}", std::process::id()));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp_file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = f.metadata()?.permissions();
            perms.set_mode(0o600);
            f.set_permissions(perms)?;
        }
        f.write_all(token.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_file, &token_file)?;

    Ok(token)
}
