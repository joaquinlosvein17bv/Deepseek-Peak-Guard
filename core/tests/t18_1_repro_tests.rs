use chrono::Utc;
use peakguard_core::clock::{Clock, SystemClock};
use peakguard_core::control_api::{create_control_router, resolve_or_create_control_token, ControlApiState};
use peakguard_core::policy::{OperatingMode, PolicyEngine};
use peakguard_core::registry::{ActiveSession, ModelRegistry, SessionRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_repro_foreign_provider_session_causes_false_protected() {
    // T18.1.1 Reproducción:
    // Demuestra que una sesión activa de Claude / Anthropic no debe hacer
    // que la Control API declare PROTECTED para DeepSeek cuando no hay tráfico ni sesiones de DeepSeek.
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

    // REGISTRAR UNA SESIÓN DE OTRO PROVEEDOR (ej. Anthropic Claude)
    session_registry.register(ActiveSession {
        session_id: "session-claude-99".to_string(),
        client_type: "opencode".to_string(),
        client_instance_id: "inst_test_claude".to_string(),
        turn_id: Some("turn-c1".to_string()),
        provider_id: "anthropic".to_string(),
        model: "claude-3-5-sonnet".to_string(),
        schedule_id: None,
        is_active: true,
        last_heartbeat: Utc::now(),
        registered_at: Utc::now(),
    });

    let state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: registry.clone(),
        schedules: Arc::new(schedules),
        event_tx,
        session_registry: session_registry.clone(),
        clock,
        auth_token: Arc::new("pg_valid_token_12345".to_string()),
        shutdown_tx: None,
    };

    let router = create_control_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{}/api/v1/status", addr))
        .send()
        .await
        .unwrap();

    let json: serde_json::Value = res.json().await.unwrap();
    let coverage = json["coverage"].as_str().unwrap();

    // T18.1.1: Una sesión de Claude activa NO debe activar cobertura para DeepSeek. Debe ser PARTIAL.
    println!("[VERIF T18.1.1] Cobertura reportada con sesión de Claude activa: {}", coverage);
    assert_eq!(
        coverage,
        "PARTIAL",
        "Una sesión activa de Claude no debe activar PROTECTED para DeepSeek"
    );

    // Ahora registrar una sesión genuina de DeepSeek: debe transicionar a PROTECTED
    session_registry.register(ActiveSession {
        session_id: "session-deepseek-01".to_string(),
        client_type: "opencode".to_string(),
        client_instance_id: "inst_test_1".to_string(),
        turn_id: Some("turn-ds1".to_string()),
        provider_id: "deepseek_direct".to_string(),
        model: "deepseek-chat".to_string(),
        schedule_id: None,
        is_active: true,
        last_heartbeat: Utc::now(),
        registered_at: Utc::now(),
    });

    let res2 = client
        .get(format!("http://{}/api/v1/status", addr))
        .send()
        .await
        .unwrap();
    let json2: serde_json::Value = res2.json().await.unwrap();
    // T18.2.1: Registrar sesión de DeepSeek NO otorga PROTECTED por sí sola; permanece en PARTIAL
    assert_eq!(
        json2["coverage"].as_str().unwrap(),
        "PARTIAL",
        "Registrar sesión sin tráfico observado por gateway debe reportar PARTIAL"
    );

    // Tras observar tráfico real a través del gateway para DeepSeek, recién transiciona a PROTECTED
    registry.record_observed_traffic("deepseek_direct", "deepseek-chat");
    let res3 = client
        .get(format!("http://{}/api/v1/status", addr))
        .send()
        .await
        .unwrap();
    let json3: serde_json::Value = res3.json().await.unwrap();
    assert_eq!(
        json3["coverage"].as_str().unwrap(),
        "PROTECTED",
        "Con tráfico real observado en gateway para DeepSeek transiciona a PROTECTED"
    );
}

#[tokio::test]
async fn test_repro_token_read_error_not_failing_closed() {
    // T18.1.5:
    // Si el archivo de token existe pero está vacío o corrupto,
    // se debe fallar cerrado (Err) y no sobrescribir silenciosamente credenciales existentes.
    let temp_dir = std::env::temp_dir().join(format!("pg_test_token_repro_{}", std::process::id()));
    let pg_dir = temp_dir.join(".peakguard");
    let _ = std::fs::create_dir_all(&pg_dir);
    let token_file = pg_dir.join("control.token");

    // Crear archivo de token vacío (simulando error o archivo ilegible/corrupto)
    std::fs::write(&token_file, "").unwrap();

    std::env::set_var("USERPROFILE", &temp_dir);

    let res = resolve_or_create_control_token("NON_EXISTENT_ENV_VAR_12345");
    println!("[VERIF T18.1.5] Resultado con archivo de token vacío: {:?}", res);

    assert!(res.is_err(), "Debe fallar cerrado (Err) si el archivo de token existe pero está vacío");

    // Verificar que cuando el archivo genuinamente NO existe, sí genera un token válido
    let _ = std::fs::remove_file(&token_file);
    let res_create = resolve_or_create_control_token("NON_EXISTENT_ENV_VAR_12345");
    assert!(res_create.is_ok(), "Debe generar token nuevo cuando no existe el archivo");
    assert!(res_create.unwrap().starts_with("pg_"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_repro_pending_commands_and_instance_isolated_ack() {
    // T18.1.3:
    // Demuestra:
    // 1. Un adaptador que se reconecta puede consultar y recuperar órdenes Stop pendientes vía GET /api/v1/adapters/commands/pending?client_instance_id=...
    // 2. Un intento de ACK desde una instancia ajena es rechazado con 403 Forbidden.
    // 3. Un ACK de la instancia legítima es procesado exitosamente (200 OK) y es idempotente ante reintentos.
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

    let auth_token = "pg_auth_secret_token_abc";
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

    // 1. Crear un comando Stop en el StopCommandManager para inst_legit_01
    let cmd_mgr = session_registry.get_command_manager();
    let cmd = cmd_mgr.create_command(
        "opencode",
        "inst_legit_01",
        "session_recov_01",
        Some("turn_42".to_string()),
        "deepseek_direct",
        "deepseek-chat",
        &Utc::now().to_rfc3339(),
        "Transición a horario pico",
    );

    // 2. Consultar pendientes para inst_legit_01
    let res_pending = client
        .get(format!(
            "http://{}/api/v1/adapters/commands/pending?client_instance_id=inst_legit_01",
            addr
        ))
        .bearer_auth(auth_token)
        .send()
        .await
        .unwrap();

    assert_eq!(res_pending.status(), reqwest::StatusCode::OK);
    let json_pending: serde_json::Value = res_pending.json().await.unwrap();
    let cmds = json_pending["commands"].as_array().unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0]["command_id"], cmd.command_id);

    // 3. Consultar pendientes para otra instancia: no debe tener nada
    let res_other = client
        .get(format!(
            "http://{}/api/v1/adapters/commands/pending?client_instance_id=inst_other_99",
            addr
        ))
        .bearer_auth(auth_token)
        .send()
        .await
        .unwrap();
    let json_other: serde_json::Value = res_other.json().await.unwrap();
    assert_eq!(json_other["commands"].as_array().unwrap().len(), 0);

    // 4. Intentar enviar ACK desde una instancia extraña/impostora: DEBE RECHAZARSE (403)
    let res_imposter_ack = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_imposter",
            "session_id": "session_recov_01",
            "turn_id": "turn_42",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res_imposter_ack.status(),
        reqwest::StatusCode::FORBIDDEN,
        "Stop ACK con client_instance_id incorrecto debe responder 403 Forbidden"
    );

    // 5. Enviar ACK desde la instancia legítima: DEBE ACEPTARSE (200 OK)
    let res_valid_ack = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_legit_01",
            "session_id": "session_recov_01",
            "turn_id": "turn_42",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res_valid_ack.status(), reqwest::StatusCode::OK);

    // 6. Reintento de ACK es idempotente (200 OK)
    let res_dup_ack = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_legit_01",
            "session_id": "session_recov_01",
            "turn_id": "turn_42",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res_dup_ack.status(), reqwest::StatusCode::OK);

    // 7. Ya no debe figurar como pendiente para inst_legit_01
    let res_pending_after = client
        .get(format!(
            "http://{}/api/v1/adapters/commands/pending?client_instance_id=inst_legit_01",
            addr
        ))
        .bearer_auth(auth_token)
        .send()
        .await
        .unwrap();
    let json_after: serde_json::Value = res_pending_after.json().await.unwrap();
    assert_eq!(json_after["commands"].as_array().unwrap().len(), 0);
}
