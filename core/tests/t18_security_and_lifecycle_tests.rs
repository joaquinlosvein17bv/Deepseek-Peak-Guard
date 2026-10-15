use chrono::Utc;
use peakguard_core::clock::{Clock, SystemClock};
use peakguard_core::control_api::{generate_secure_token, create_control_router, ControlApiState};
use peakguard_core::policy::{OperatingMode, PolicyEngine};
use peakguard_core::registry::{ActiveSession, ModelRegistry, ProviderInfo, SessionRegistry};
use peakguard_core::schedule::Schedule;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::net::TcpListener;

#[test]
fn test_security_csprng_token_generation_and_uniqueness() {
    // T18 Paso 2: Generar tokens mediante CSPRNG del SO con alta entropía
    let mut tokens = HashSet::new();
    for _ in 0..100 {
        let token = generate_secure_token().expect("Fallo al generar token CSPRNG");
        assert!(token.starts_with("pg_"), "Token debe tener prefijo pg_");
        assert_eq!(token.len(), 3 + 64, "Token debe contener 32 bytes (64 hex)");
        assert!(tokens.insert(token), "Colisión detectada: el generador debe tener entropía criptográfica");
    }
}

#[tokio::test]
async fn test_security_empty_expected_token_rejected_fail_closed() {
    // T18 Paso 2: Nunca aceptar tokens esperados vacíos (fail closed con 401)
    let registry = Arc::new(ModelRegistry::default());
    let schedules = HashMap::new();
    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::RestrictedStop,
        registry.clone(),
        schedules.clone(),
    ));
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let session_registry = Arc::new(SessionRegistry::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    // Estado con token VACÍO
    let state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: registry.clone(),
        schedules: Arc::new(schedules),
        event_tx,
        session_registry,
        clock,
        auth_token: Arc::new("".to_string()), // TOKEN VACÍO
        shutdown_tx: None,
    };

    let router = create_control_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    
    // 1. Solicitud sin cabecera de autenticación -> DEBE SER RECHAZADA (401)
    let res1 = client
        .put(format!("http://{}/api/v1/config/mode", addr))
        .json(&serde_json::json!({ "mode": "notify_only" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res1.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 2. Solicitud con token vacío 'Bearer ' -> DEBE SER RECHAZADA (401)
    let res2 = client
        .put(format!("http://{}/api/v1/config/mode", addr))
        .header("Authorization", "Bearer ")
        .json(&serde_json::json!({ "mode": "notify_only" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res2.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 3. Solicitud con token aleatorio -> DEBE SER RECHAZADA (401)
    let res3 = client
        .put(format!("http://{}/api/v1/config/mode", addr))
        .header("Authorization", "Bearer invalid-token")
        .json(&serde_json::json!({ "mode": "notify_only" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res3.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_security_sse_query_token_rejected_and_bearer_header_accepted() {
    // T18 Paso 2: Eliminar tokens en parámetros query de SSE (?token=...)
    // Exigir exclusivamente cabecera 'Authorization: Bearer <token>'
    let registry = Arc::new(ModelRegistry::default());
    let schedules = HashMap::new();
    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::RestrictedStop,
        registry.clone(),
        schedules.clone(),
    ));
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let session_registry = Arc::new(SessionRegistry::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let token = "valid-secret-token-t18";

    let state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: registry.clone(),
        schedules: Arc::new(schedules),
        event_tx,
        session_registry,
        clock,
        auth_token: Arc::new(token.to_string()),
        shutdown_tx: None,
    };

    let router = create_control_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Conexión a SSE con token en parámetro URL (?token=...) -> RECHAZADA (401)
    let query_res = client
        .get(format!("http://{}/api/v1/events?token={}", addr, token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        query_res.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "Tokens en query params deben ser rechazados en SSE"
    );

    // 2. Conexión a SSE con cabecera Authorization: Bearer <token> -> ACEPTADA (200)
    let bearer_res = client
        .get(format!("http://{}/api/v1/events", addr))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(
        bearer_res.status(),
        reqwest::StatusCode::OK,
        "Cabecera Bearer debe ser aceptada en SSE"
    );
}

#[tokio::test]
async fn test_verifiable_coverage_transitions_from_partial_to_protected() {
    // T18 Paso 3: Cobertura comprobable sin asumir PROTECTED por configuración estática
    let registry = Arc::new(ModelRegistry::default());
    let mut schedules = HashMap::new();
    let default_sched = Schedule::default_deepseek();
    schedules.insert(default_sched.id.clone(), default_sched);

    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::RestrictedStop,
        registry.clone(),
        schedules.clone(),
    ));
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let session_registry = Arc::new(SessionRegistry::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    let state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: registry.clone(),
        schedules: Arc::new(schedules),
        event_tx,
        session_registry: session_registry.clone(),
        clock,
        auth_token: Arc::new("test-token-123".to_string()),
        shutdown_tx: None,
    };

    let router = create_control_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();

    // 1. Estado inicial antes de observar tráfico o sesiones -> PARTIAL
    let res1 = client.get(format!("http://{}/api/v1/status", addr)).send().await.unwrap();
    let json1: serde_json::Value = res1.json().await.unwrap();
    assert_eq!(json1["coverage"], "PARTIAL");
    assert_eq!(json1["observed_traffic_count"], 0);
    assert_eq!(json1["coverage_details"]["reason"], "route_configured_awaiting_first_observed_request");

    // 2. Al observar la primera petición a través del gateway para deepseek_direct -> Transición a PROTECTED
    registry.record_observed_traffic("deepseek_direct", "deepseek-chat");

    let res2 = client.get(format!("http://{}/api/v1/status", addr)).send().await.unwrap();
    let json2: serde_json::Value = res2.json().await.unwrap();
    assert_eq!(json2["coverage"], "PROTECTED");
    assert_eq!(json2["observed_traffic_count"], 1);
    assert_eq!(json2["coverage_details"]["verified_route"], "deepseek_direct");
}

#[tokio::test]
async fn test_opencode_go_reports_unverified_without_custom_gateway_traffic() {
    // T18 Paso 3: OpenCode Go debe reportar UNVERIFIED si no hay tráfico intermediado
    let mut reg = ModelRegistry::default();
    // Deshabilitar deepseek_direct y habilitar solo opencode_go
    reg.register_provider(ProviderInfo {
        id: "deepseek_direct".to_string(),
        upstream_url: "https://api.deepseek.com".to_string(),
        schedule_id: "deepseek_direct_verified".to_string(),
        enabled: false,
    });
    reg.register_provider(ProviderInfo {
        id: "opencode_go".to_string(),
        upstream_url: "https://opencode.ai/zen/go/v1".to_string(),
        schedule_id: "opencode_go_verified".to_string(),
        enabled: true,
    });
    let registry = Arc::new(reg);
    let schedules = HashMap::new();
    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::RestrictedStop,
        registry.clone(),
        schedules.clone(),
    ));
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let session_registry = Arc::new(SessionRegistry::new());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    let state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: registry.clone(),
        schedules: Arc::new(schedules),
        event_tx,
        session_registry,
        clock,
        auth_token: Arc::new("test-token".to_string()),
        shutdown_tx: None,
    };

    let router = create_control_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res = client.get(format!("http://{}/api/v1/status", addr)).send().await.unwrap();
    let json: serde_json::Value = res.json().await.unwrap();

    assert_eq!(json["coverage"], "UNVERIFIED");
    assert_eq!(json["coverage_details"]["requires_custom_base_url"], true);
}

#[test]
fn test_lifecycle_active_reasoning_turn_preserved_across_silence() {
    // T18 Paso 4: Conservación de sesiones activas de larga duración
    // Una sesión en turno de razonamiento silencioso (75s) NO debe eliminarse
    let registry = SessionRegistry::new();
    let old_time = Utc::now() - chrono::Duration::seconds(75);

    // 1. Turno de razonamiento activo
    registry.register(ActiveSession {
        session_id: "reasoning-turn-deepseek-r1".to_string(),
        client_type: "pi".to_string(),
        client_instance_id: "inst_test_pi".to_string(),
        turn_id: Some("turn-reasoning-1".to_string()),
        provider_id: "deepseek_direct".to_string(),
        model: "deepseek-reasoner".to_string(),
        schedule_id: Some("deepseek_direct_verified".to_string()),
        is_active: true,
        last_heartbeat: old_time,
        registered_at: old_time,
    });

    // 2. Sesión finalizada inactiva
    registry.register(ActiveSession {
        session_id: "idle-finished-session".to_string(),
        client_type: "pi".to_string(),
        client_instance_id: "inst_test_pi".to_string(),
        turn_id: None,
        provider_id: "deepseek_direct".to_string(),
        model: "deepseek-chat".to_string(),
        schedule_id: Some("deepseek_direct_verified".to_string()),
        is_active: false,
        last_heartbeat: old_time,
        registered_at: old_time,
    });

    assert_eq!(registry.count_active(), 1);

    // Ejecutar prune_stale con inactive_timeout de 60s y active_max_silence de 30 min
    registry.prune_stale(chrono::Duration::seconds(60), chrono::Duration::minutes(30));

    // La sesión activa DEBE estar viva
    assert_eq!(registry.count_active(), 1);
    let active_sessions = registry.get_active_sessions_for_schedule(
        "deepseek_direct_verified",
        &ModelRegistry::default(),
    );
    assert_eq!(active_sessions.len(), 1);
    assert_eq!(active_sessions[0].session_id, "reasoning-turn-deepseek-r1");
}
