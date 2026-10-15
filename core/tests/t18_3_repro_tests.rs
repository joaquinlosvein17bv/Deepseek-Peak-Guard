use chrono::Utc;
use peakguard_core::clock::{Clock, SystemClock};
use peakguard_core::control_api::{create_control_router, ControlApiState};
use peakguard_core::policy::{OperatingMode, PolicyEngine};
use peakguard_core::registry::{
    ActiveSession, ModelRegistry, SessionRegistry, StopCommandManager,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_repro_stop_ack_mandatory_identity_and_no_fallback_search() {
    // T18.3.2:
    // 1. command_id es obligatorio (400 Bad Request si falta o está vacío)
    // 2. No debe haber búsqueda alternativa de comandos pendientes si falta command_id
    // 3. Si la orden original tenía turn_id, el ACK debe proveer el mismo turn_id
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

    let auth_token = "tok_strict_ack_18_3";
    let state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: registry.clone(),
        schedules: Arc::new(schedules),
        event_tx,
        session_registry: session_registry.clone(),
        clock,
        auth_token: Arc::new(auth_token.to_string()),
        shutdown_tx: None,
    };

    let router = create_control_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let cmd_mgr = session_registry.get_command_manager();
    let cmd = cmd_mgr.create_command(
        "opencode",
        "inst_strict_01",
        "sess_strict_01",
        Some("turn-xyz".to_string()),
        "deepseek_direct",
        "deepseek-chat",
        &Utc::now().to_rfc3339(),
        "Orden para prueba de ACK estricto",
    ).unwrap();

    // 1. Envío de ACK sin command_id: DEBE RECHAZARSE (400 Bad Request)
    let res_no_cmd = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "client_instance_id": "inst_strict_01",
            "session_id": "sess_strict_01",
            "turn_id": "turn-xyz",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    println!("[TEST T18.3.2] Status sin command_id: {}", res_no_cmd.status());
    assert_eq!(
        res_no_cmd.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "Stop ACK sin command_id debe ser rechazado con 400 Bad Request"
    );

    // 2. Envío de ACK con turn_id ausente cuando la orden requería 'turn-xyz': DEBE RECHAZARSE
    let res_missing_turn = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_strict_01",
            "session_id": "sess_strict_01",
            // turn_id omitido
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    println!("[TEST T18.3.2] Status con turn_id ausente: {}", res_missing_turn.status());
    assert!(
        res_missing_turn.status() == reqwest::StatusCode::BAD_REQUEST
            || res_missing_turn.status() == reqwest::StatusCode::FORBIDDEN,
        "Stop ACK con turn_id ausente cuando la orden original requería turno debe ser rechazado"
    );

    // 3. Envío de ACK completo válido: DEBE ACEPTARSE (200 OK)
    let res_valid = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_strict_01",
            "session_id": "sess_strict_01",
            "turn_id": "turn-xyz",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res_valid.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_repro_heartbeat_and_unregister_isolation_no_global_fallback() {
    // T18.3.3:
    // Demuestra que:
    // 1. Heartbeat con client_instance_id incorrecto debe fallar y NO modificar sesiones de otra instancia
    // 2. Unregister con client_instance_id incorrecto debe fallar y NO borrar sesiones de otra instancia
    // 3. Dos clientes con el mismo session_id están aislados por su client_instance_id
    let session_registry = SessionRegistry::new();

    // Registrar cliente 1: session_id = "shared-session-name", client_instance_id = "inst_A"
    session_registry.register(ActiveSession {
        session_id: "shared-session-name".to_string(),
        client_type: "opencode".to_string(),
        client_instance_id: "inst_A".to_string(),
        turn_id: Some("turn-A1".to_string()),
        provider_id: "deepseek_direct".to_string(),
        model: "deepseek-chat".to_string(),
        schedule_id: None,
        is_active: true,
        last_heartbeat: Utc::now(),
        registered_at: Utc::now(),
    });

    // Registrar cliente 2: session_id = "shared-session-name", client_instance_id = "inst_B"
    session_registry.register(ActiveSession {
        session_id: "shared-session-name".to_string(),
        client_type: "opencode".to_string(),
        client_instance_id: "inst_B".to_string(),
        turn_id: Some("turn-B1".to_string()),
        provider_id: "deepseek_direct".to_string(),
        model: "deepseek-chat".to_string(),
        schedule_id: None,
        is_active: true,
        last_heartbeat: Utc::now(),
        registered_at: Utc::now(),
    });

    // 1. Intento de heartbeat con instancia inexistente "inst_IMPOSTOR": DEBE RETORNAR FALSE
    let hb_result = session_registry.heartbeat(
        "inst_IMPOSTOR",
        "shared-session-name",
        Some("turn-hijack".to_string()),
        Some(false),
    );
    assert!(
        !hb_result,
        "Heartbeat con client_instance_id que no coincide NO debe tener fallback global ni actualizar nada"
    );

    // Verificar que ni inst_A ni inst_B fueron mutados
    let sessions = session_registry.get_active_sessions();
    let sess_a = sessions.iter().find(|s| s.client_instance_id == "inst_A").unwrap();
    assert_eq!(sess_a.turn_id.as_deref(), Some("turn-A1"), "Sesión A no debe ser mutada");
    assert!(sess_a.is_active, "Sesión A debe seguir activa");

    let sess_b = sessions.iter().find(|s| s.client_instance_id == "inst_B").unwrap();
    assert_eq!(sess_b.turn_id.as_deref(), Some("turn-B1"), "Sesión B no debe ser mutada");
    assert!(sess_b.is_active, "Sesión B debe seguir activa");

    // 2. Intento de unregister con instancia inexistente: NO debe borrar nada
    session_registry.unregister("inst_IMPOSTOR", "shared-session-name");
    assert_eq!(
        session_registry.get_active_sessions().len(),
        2,
        "Unregister con instancia ajena no debe eliminar ninguna sesión"
    );

    // 3. Unregister legítimo de inst_A: solo debe borrar inst_A; inst_B debe permanecer viva
    session_registry.unregister("inst_A", "shared-session-name");
    let remaining = session_registry.get_active_sessions();
    assert_eq!(remaining.len(), 1, "Solo la sesión de inst_A debe ser eliminada");
    assert_eq!(remaining[0].client_instance_id, "inst_B");
}

#[tokio::test]
async fn test_repro_persistence_corrupt_file_fail_closed() {
    // T18.3.4:
    // Demuestra que si el archivo de comandos existe pero está corrupto (JSON truncado o inválido):
    // 1. StopCommandManager::with_persistence DEBE retornar Err (fallar cerrado)
    // 2. NO DEBE sobrescribir el archivo corrupto con un JSON vacío []
    let temp_dir = std::env::temp_dir().join(format!("pg_test_corrupt_{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
    let _ = std::fs::create_dir_all(&temp_dir);
    let corrupt_file = temp_dir.join("corrupt_commands.json");

    let corrupt_content = "{\"this_is_not_valid_json\": [";
    std::fs::write(&corrupt_file, corrupt_content).unwrap();

    let result = StopCommandManager::with_persistence(corrupt_file.clone());
    println!("[TEST T18.3.4] Resultado al cargar archivo corrupto: {:?}", result.is_err());
    assert!(
        result.is_err(),
        "Cargar archivo de comandos corrupto DEBE fallar cerrado (retornar Err)"
    );

    // Verificar que el archivo no fue reemplazado por [] vacío
    let content_after = std::fs::read_to_string(&corrupt_file).unwrap();
    assert_eq!(
        content_after, corrupt_content,
        "El archivo corrupto NO debe ser destruido ni sobrescrito silenciosamente con estado vacío"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
