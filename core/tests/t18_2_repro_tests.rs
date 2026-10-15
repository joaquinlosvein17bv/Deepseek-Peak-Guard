use chrono::Utc;
use peakguard_core::clock::{Clock, SystemClock};
use peakguard_core::control_api::{create_control_router, ControlApiState};
use peakguard_core::policy::{OperatingMode, PolicyEngine};
use peakguard_core::registry::{ActiveSession, ModelRegistry, SessionRegistry, StopCommandStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_repro_session_registration_alone_must_not_grant_protected_coverage() {
    // T18.2.1:
    // Demuestra que registrar una sesión de DeepSeek (active_sessions > 0)
    // NO debe hacer que la Control API declare PROTECTED si el Gateway aún no ha
    // observado ni una sola petición real en su ruta loopback (127.0.0.1:8765).
    // Antes de T18.2, `direct_traffic_observed || direct_active_sessions > 0`
    // declaraba erróneamente PROTECTED solo con registrar la sesión.
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

    // REGISTRAR SESIÓN ACTIVA DE DEEPSEEK SIN TRÁFICO OBSERVADO EN EL GATEWAY
    session_registry.register(ActiveSession {
        session_id: "session-ds-no-traffic".to_string(),
        client_type: "opencode".to_string(),
        client_instance_id: "inst_repro_1".to_string(),
        turn_id: Some("turn-1".to_string()),
        provider_id: "deepseek_direct".to_string(),
        model: "deepseek-chat".to_string(),
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
        auth_token: Arc::new("token_test_123".to_string()),
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

    println!("[TEST REPRO T18.2.1] Cobertura reportada con sesión activa pero 0 tráfico gateway: {}", coverage);

    // DEBE SER PARTIAL (nunca PROTECTED)
    assert_eq!(
        coverage,
        "PARTIAL",
        "Registrar una sesión de DeepSeek NO debe activar PROTECTED sin tráfico observado por el gateway"
    );

    // Solo tras registrar tráfico observado real a través del gateway debe pasar a PROTECTED
    registry.record_observed_traffic("deepseek_direct", "deepseek-chat");

    let res2 = client
        .get(format!("http://{}/api/v1/status", addr))
        .send()
        .await
        .unwrap();
    let json2: serde_json::Value = res2.json().await.unwrap();
    assert_eq!(
        json2["coverage"].as_str().unwrap(),
        "PROTECTED",
        "Solo al observar tráfico real intermediado por el gateway se declara PROTECTED"
    );
}

#[tokio::test]
async fn test_repro_stop_ack_must_require_mandatory_client_instance_and_prevent_terminal_mutation() {
    // T18.2.3:
    // Demuestra que:
    // 1. Un Stop ACK sin client_instance_id debe ser rechazado (400 Bad Request o error explícito).
    // 2. Un Stop ACK con turn_id que no coincida con el comando original debe ser rechazado.
    // 3. Un Stop ACK repetido sobre un comando ya terminado (Failed) no debe mutar su estado terminal a Acknowledged.
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

    let auth_token = "token_ack_test_456";
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
        "inst_valid_01",
        "session_ack_1",
        Some("turn-10".to_string()),
        "deepseek_direct",
        "deepseek-chat",
        &Utc::now().to_rfc3339(),
        "Prueba de Stop ACK estricto",
    );

    // 1. Intento de enviar Stop ACK SIN client_instance_id: DEBE RECHAZARSE (400)
    let res_missing_instance = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "session_id": "session_ack_1",
            "turn_id": "turn-10",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    println!("[TEST REPRO T18.2.3] Status con client_instance_id ausente: {}", res_missing_instance.status());
    assert_eq!(
        res_missing_instance.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "Stop ACK sin client_instance_id obligatorio debe responder 400 Bad Request"
    );

    // 2. Intento de enviar Stop ACK con turn_id que no coincide con el turno de la orden: DEBE RECHAZARSE
    let res_wrong_turn = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_valid_01",
            "session_id": "session_ack_1",
            "turn_id": "turn-99-wrong",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    println!("[TEST REPRO T18.2.3] Status con turn_id discordante: {}", res_wrong_turn.status());
    assert!(
        res_wrong_turn.status() == reqwest::StatusCode::BAD_REQUEST
            || res_wrong_turn.status() == reqwest::StatusCode::FORBIDDEN,
        "Stop ACK con turn_id discordante debe ser rechazado"
    );

    // 3. Registrar primero un ACK con FALLO (success = false)
    let res_fail_ack = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_valid_01",
            "session_id": "session_ack_1",
            "turn_id": "turn-10",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": false,
            "error": "Error nativo en abort"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res_fail_ack.status(), reqwest::StatusCode::ACCEPTED);

    // 4. Intentar enviar un segundo ACK alegando success=true: NO debe alterar el estado terminal 'Failed'
    let res_mutate_attempt = client
        .post(format!("http://{}/api/v1/adapters/stop-ack", addr))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({
            "command_id": cmd.command_id,
            "client_instance_id": "inst_valid_01",
            "session_id": "session_ack_1",
            "turn_id": "turn-10",
            "transition_utc": Utc::now().to_rfc3339(),
            "success": true
        }))
        .send()
        .await
        .unwrap();

    // El resultado debe reflejar el estado terminal previo (Failed), no mutado
    let json_mut: serde_json::Value = res_mutate_attempt.json().await.unwrap();
    println!("[TEST REPRO T18.2.3] Respuesta ante intento de mutar comando terminal: {:?}", json_mut);
    assert_eq!(json_mut["status"], "failed", "Un comando terminal no debe mutar su resultado previo");
}

#[tokio::test]
async fn test_repro_stop_recovery_after_daemon_restart() {
    // T18.2.4:
    // Demuestra que:
    // 1. Los comandos de stop pendientes se persisten en disco (pending_commands.json).
    // 2. Al reiniciar el daemon:
    //    - Comandos pendientes recientes (<10 min) se restauran como Pending.
    //    - Comandos pendientes viejos (>10 min) pasan a estado Expired automáticamente.
    // 3. Los comandos terminales (Acknowledged, Failed) mantienen su estado terminal.
    let temp_dir = std::env::temp_dir().join(format!("pg_test_restart_{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
    let commands_file = temp_dir.join("pending_commands.json");

    // Instancia 1 del SessionRegistry con persistencia
    let reg1 = SessionRegistry::with_persistence_path(commands_file.clone());
    let cmd_mgr1 = reg1.get_command_manager();

    // 1. Crear comando reciente (debe recuperarse como Pending)
    let cmd_recent = cmd_mgr1.create_command(
        "opencode",
        "inst_restart_01",
        "sess_recent",
        Some("turn-1".to_string()),
        "deepseek_direct",
        "deepseek-chat",
        &Utc::now().to_rfc3339(),
        "Orden reciente",
    );

    // 2. Crear comando viejo (> 10 minutos)
    // Para simularlo de forma determinista, escribimos en el archivo JSON una orden con timestamp de hace 15 minutos
    let old_timestamp = Utc::now() - chrono::Duration::minutes(15);
    let cmd_old = cmd_mgr1.create_command(
        "opencode",
        "inst_restart_01",
        "sess_old",
        Some("turn-old".to_string()),
        "deepseek_direct",
        "deepseek-chat",
        &old_timestamp.to_rfc3339(),
        "Orden vieja",
    );

    // Modificar manualmente el JSON para que cmd_old tenga created_at hace 15 minutos
    let json_str = std::fs::read_to_string(&commands_file).unwrap();
    let mut cmds: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
    for c in &mut cmds {
        if c["command_id"].as_str() == Some(&cmd_old.command_id) {
            c["created_at"] = serde_json::json!(old_timestamp.to_rfc3339());
        }
    }
    std::fs::write(&commands_file, serde_json::to_string_pretty(&cmds).unwrap()).unwrap();

    // DROP de reg1: simulamos apagado del daemon
    drop(reg1);

    // REINICIO DEL DAEMON: Instancia 2 del SessionRegistry leyendo el mismo archivo
    let reg2 = SessionRegistry::with_persistence_path(commands_file.clone());
    let cmd_mgr2 = reg2.get_command_manager();

    // Verificar orden reciente: debe seguir Pending
    let recovered_recent = cmd_mgr2.get_command(&cmd_recent.command_id).expect("Debe existir");
    assert_eq!(recovered_recent.status, StopCommandStatus::Pending);

    // Verificar orden vieja (> 10m): debe haberse reconciliado a Expired
    let recovered_old = cmd_mgr2.get_command(&cmd_old.command_id).expect("Debe existir");
    assert_eq!(recovered_old.status, StopCommandStatus::Expired);
    assert!(recovered_old.ack_error.unwrap().contains(">10m"));

    // Procesar ACK exitoso para la orden reciente recuperada
    let ack_res = cmd_mgr2.record_ack(
        Some(&cmd_recent.command_id),
        Some("inst_restart_01"),
        "sess_recent",
        Some("turn-1"),
        true,
        None,
    ).unwrap();
    assert_eq!(ack_res.status, StopCommandStatus::Acknowledged);

    // REINICIO 2 DEL DAEMON: Verificar que el estado terminal Acknowledged persiste
    drop(reg2);
    let reg3 = SessionRegistry::with_persistence_path(commands_file.clone());
    let cmd_mgr3 = reg3.get_command_manager();
    let terminal_recovered = cmd_mgr3.get_command(&cmd_recent.command_id).expect("Debe existir");
    assert_eq!(terminal_recovered.status, StopCommandStatus::Acknowledged);

    // Limpieza
    let _ = std::fs::remove_dir_all(&temp_dir);
}

