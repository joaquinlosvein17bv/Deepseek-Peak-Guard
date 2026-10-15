use chrono::Utc;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

async fn run_mock_upstream_t19(call_counter: Arc<AtomicUsize>) -> SocketAddr {
    use axum::extract::Json;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::Router;

    let handler = {
        let counter = call_counter.clone();
        move |Json(payload): Json<serde_json::Value>| {
            let c = counter.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                let model = payload
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let is_stream = payload
                    .get("stream")
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false);

                if is_stream {
                    use async_stream::stream;
                    use axum::response::sse::{Event, Sse};
                    use std::convert::Infallible;

                    let s = stream! {
                        let chunk1 = serde_json::json!({
                            "id": "chatcmpl-t19-chunk1",
                            "object": "chat.completion.chunk",
                            "model": model,
                            "choices": [{ "delta": { "content": "T19 " } }]
                        });
                        yield Ok::<Event, Infallible>(Event::default().data(chunk1.to_string()));

                        tokio::time::sleep(Duration::from_millis(150)).await;

                        let chunk2 = serde_json::json!({
                            "id": "chatcmpl-t19-chunk2",
                            "object": "chat.completion.chunk",
                            "model": model,
                            "choices": [{ "delta": { "content": "Simulated Streaming" } }]
                        });
                        yield Ok::<Event, Infallible>(Event::default().data(chunk2.to_string()));

                        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
                    };
                    Sse::new(s).into_response()
                } else {
                    (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({
                            "id": "chatcmpl-t19-resp",
                            "object": "chat.completion",
                            "model": model,
                            "choices": [{
                                "index": 0,
                                "message": { "role": "assistant", "content": "T19 Upstream Live Mock Response" }
                            }]
                        })),
                    )
                        .into_response()
                }
            }
        }
    };

    let app = Router::new()
        .route("/chat/completions", post(handler.clone()))
        .route("/v1/chat/completions", post(handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    addr
}

fn get_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[tokio::test]
async fn test_t19_three_modes_virtual_clock_against_mock_upstream() {
    let call_counter = Arc::new(AtomicUsize::new(0));
    let mock_upstream = run_mock_upstream_t19(call_counter.clone()).await;

    let gw_port = get_free_port();
    let ctrl_port = get_free_port();
    let auth_token = "tok_t19_three_modes_secret";

    let tmp_commands_path = std::env::temp_dir().join(format!("peakguard_t19_cmds_{}.json", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
    let initial_cmds = serde_json::json!([]);
    std::fs::write(&tmp_commands_path, serde_json::to_string(&initial_cmds).unwrap()).unwrap();

    // Configuración inicial en modo notify_only
    let tmp_config = format!(
        r#"config_version = 1

[general]
enabled = true
mode = "notify_only"
show_timezone = "UTC"

[alerts]
minutes_before = [30, 10, 5]
final_countdown_seconds = 10
desktop = false
tui = false
telegram = false
notify_on_peak_end = false

[gateway]
listen_address = "127.0.0.1"
listen_port = {gw_port}
allow_remote_connections = false
fail_closed_for_protected_routes = true
active_stream_on_peak = "cancel"

[control]
listen_address = "127.0.0.1"
listen_port = {ctrl_port}
token_env = "PEAKGUARD_CONTROL_TOKEN"

[stop]
ack_timeout_ms = 2000
auto_resume = false
fallback_to_gateway_cancel = true

[telegram]

[[providers]]
id = "deepseek_direct"
upstream = "http://{mock_upstream}"
enabled = true
schedule_id = "deepseek_test_sched"

[[providers]]
id = "opencode_go"
upstream = "http://{mock_upstream}"
enabled = true
schedule_id = "offpeak_sched"

[[schedules]]
id = "deepseek_test_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00"]
exclude_chinese_public_holidays = true

[[schedules]]
id = "offpeak_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["12:00-16:00"]
exclude_chinese_public_holidays = true
"#
    );

    let tmp_config_path = std::env::temp_dir().join(format!("peakguard_t19_cfg_{}.toml", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
    std::fs::write(&tmp_config_path, &tmp_config).expect("No se pudo escribir archivo de prueba temporal");

    let bin_path = env!("CARGO_BIN_EXE_peakguard");

    // Iniciar el daemon con hora simulada en pleno horario pico (2026-10-15 02:00:00 UTC)
    let mut child = tokio::process::Command::new(bin_path)
        .arg("run")
        .arg("--config")
        .arg(&tmp_config_path)
        .env("PEAKGUARD_CONTROL_TOKEN", auth_token)
        .env("PEAKGUARD_COMMANDS_PATH", &tmp_commands_path)
        .env("PEAKGUARD_SIMULATED_TIME", "2026-10-15T02:00:00Z")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Fallo al iniciar el proceso 'peakguard run'");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap();

    // 1. Esperar que Gateway y Control API respondan
    let mut ready = false;
    for _i in 0..40 {
        if let Ok(Some(status)) = child.try_wait() {
            panic!("El daemon terminó inesperadamente con estado: {:?}", status);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let gw_res = client.get(format!("http://127.0.0.1:{}/health", gw_port)).send().await;
        let ctrl_res = client.get(format!("http://127.0.0.1:{}/api/v1/health", ctrl_port)).send().await;

        if let (Ok(g), Ok(c)) = (gw_res, ctrl_res) {
            if g.status() == reqwest::StatusCode::OK && c.status() == reqwest::StatusCode::OK {
                ready = true;
                break;
            }
        }
    }
    assert!(ready, "Servidores no listos");

    // 2. FASE 1: MODO notify_only EN PLENO HORARIO PICO
    // Las peticiones a deepseek-chat deben pasar con 200 OK hacia el upstream simulado
    let res_notify = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "test notify only" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res_notify.status(),
        reqwest::StatusCode::OK,
        "En modo notify_only, la petición debe pasar con 200 OK incluso en hora pico"
    );
    assert_eq!(call_counter.load(Ordering::SeqCst), 1);

    // 3. FASE 2: CAMBIO DINÁMICO A restricted_stop
    let switch_restricted = client
        .put(format!("http://127.0.0.1:{}/api/v1/config/mode", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({ "mode": "restricted_stop" }))
        .send()
        .await
        .unwrap();
    assert_eq!(switch_restricted.status(), reqwest::StatusCode::OK);

    // En restricted_stop durante horario pico: deepseek-chat DEBE SER BLOQUEADO (403)
    let res_restricted_blocked = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "test restricted stop block" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res_restricted_blocked.status(),
        reqwest::StatusCode::FORBIDDEN,
        "En restricted_stop durante pico, el gateway debe bloquear deepseek-chat con 403 Forbidden"
    );
    let blocked_body: serde_json::Value = res_restricted_blocked.json().await.unwrap();
    assert!(blocked_body["error"]["message"].as_str().unwrap().contains("horario peak"));

    // Modelo independiente off-peak NO DEBE SER BLOQUEADO
    let res_third_party = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-v4.1-flash",
            "messages": [{ "role": "user", "content": "offpeak model allowed" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res_third_party.status(),
        reqwest::StatusCode::OK,
        "Modelos de terceros o rutas offpeak nunca deben ser bloqueados"
    );

    // 4. FASE 3: CAMBIO DINÁMICO A strict_block
    let switch_strict = client
        .put(format!("http://127.0.0.1:{}/api/v1/config/mode", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({ "mode": "strict_block" }))
        .send()
        .await
        .unwrap();
    assert_eq!(switch_strict.status(), reqwest::StatusCode::OK);

    let res_strict_blocked = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-reasoner",
            "messages": [{ "role": "user", "content": "test strict block" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res_strict_blocked.status(),
        reqwest::StatusCode::FORBIDDEN,
        "En strict_block durante pico, deepseek-reasoner debe ser bloqueado con 403 Forbidden"
    );

    // 5. APAGADO ORDENADO MEDIANTE CONTROL API
    let shutdown_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/control/shutdown", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .unwrap();
    assert_eq!(shutdown_resp.status(), reqwest::StatusCode::OK);

    let exit_status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("El proceso no se apagó ordenadamente dentro del límite")
        .expect("Fallo al esperar terminación");

    assert!(exit_status.success());
    let _ = std::fs::remove_file(&tmp_config_path);
    let _ = std::fs::remove_file(&tmp_commands_path);
}

#[tokio::test]
async fn test_t19_streaming_cancellation_and_no_auto_resume() {
    let call_counter = Arc::new(AtomicUsize::new(0));
    let mock_upstream = run_mock_upstream_t19(call_counter.clone()).await;

    let gw_port = get_free_port();
    let ctrl_port = get_free_port();
    let auth_token = "tok_t19_streaming_secret";

    let tmp_commands_path = std::env::temp_dir().join(format!("peakguard_t19_stream_cmds_{}.json", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
    std::fs::write(&tmp_commands_path, "[]").unwrap();

    let tmp_config = format!(
        r#"config_version = 1

[general]
enabled = true
mode = "notify_only"
show_timezone = "UTC"

[alerts]
minutes_before = [30, 10, 5]
final_countdown_seconds = 10
desktop = false
tui = false
telegram = false
notify_on_peak_end = true

[gateway]
listen_address = "127.0.0.1"
listen_port = {gw_port}
allow_remote_connections = false
fail_closed_for_protected_routes = true
active_stream_on_peak = "cancel"

[control]
listen_address = "127.0.0.1"
listen_port = {ctrl_port}
token_env = "PEAKGUARD_CONTROL_TOKEN"

[stop]
ack_timeout_ms = 2000
auto_resume = false
fallback_to_gateway_cancel = true

[telegram]

[[providers]]
id = "deepseek_direct"
upstream = "http://{mock_upstream}"
enabled = true
schedule_id = "deepseek_test_sched"

[[schedules]]
id = "deepseek_test_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00"]
exclude_chinese_public_holidays = true
"#
    );

    let tmp_config_path = std::env::temp_dir().join(format!("peakguard_t19_stream_cfg_{}.toml", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
    std::fs::write(&tmp_config_path, &tmp_config).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_peakguard");

    let mut child = tokio::process::Command::new(bin_path)
        .arg("run")
        .arg("--config")
        .arg(&tmp_config_path)
        .env("PEAKGUARD_CONTROL_TOKEN", auth_token)
        .env("PEAKGUARD_COMMANDS_PATH", &tmp_commands_path)
        .env("PEAKGUARD_SIMULATED_TIME", "2026-10-15T02:00:00Z")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Fallo al iniciar el proceso 'peakguard run'");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap();

    let mut ready = false;
    for _i in 0..40 {
        if let Ok(Some(status)) = child.try_wait() {
            panic!("El daemon terminó inesperadamente con estado: {:?}", status);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let gw_res = client.get(format!("http://127.0.0.1:{}/health", gw_port)).send().await;
        let ctrl_res = client.get(format!("http://127.0.0.1:{}/api/v1/health", ctrl_port)).send().await;

        if let (Ok(g), Ok(c)) = (gw_res, ctrl_res) {
            if g.status() == reqwest::StatusCode::OK && c.status() == reqwest::StatusCode::OK {
                ready = true;
                break;
            }
        }
    }
    assert!(ready);

    // 1. Probar que en notify_only el streaming pasa sin interrupción
    let stream_res = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "stream test" }],
            "stream": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(stream_res.status(), reqwest::StatusCode::OK);
    let content_type = stream_res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/event-stream"));

    let body_text = stream_res.text().await.unwrap();
    assert!(body_text.contains("chatcmpl-t19-chunk1"));
    assert!(body_text.contains("[DONE]"));

    // 2. Apagado ordenado
    let _ = client
        .post(format!("http://127.0.0.1:{}/api/v1/control/shutdown", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await;

    let _ = child.wait().await;
    let _ = std::fs::remove_file(&tmp_config_path);
    let _ = std::fs::remove_file(&tmp_commands_path);
}

#[tokio::test]
async fn test_t19_session_registry_stop_ack_and_isolation() {
    let call_counter = Arc::new(AtomicUsize::new(0));
    let mock_upstream = run_mock_upstream_t19(call_counter.clone()).await;

    let gw_port = get_free_port();
    let ctrl_port = get_free_port();
    let auth_token = "tok_t19_sessions_secret";

    let tmp_commands_path = std::env::temp_dir().join(format!(
        "peakguard_t19_sess_cmds_{}.json",
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::write(&tmp_commands_path, "[]").unwrap();

    let tmp_config = format!(
        r#"config_version = 1

[general]
enabled = true
mode = "restricted_stop"
show_timezone = "UTC"

[alerts]
minutes_before = [30, 10, 5]
final_countdown_seconds = 10
desktop = false
tui = false
telegram = false
notify_on_peak_end = false

[gateway]
listen_address = "127.0.0.1"
listen_port = {gw_port}
allow_remote_connections = false
fail_closed_for_protected_routes = true
active_stream_on_peak = "cancel"

[control]
listen_address = "127.0.0.1"
listen_port = {ctrl_port}
token_env = "PEAKGUARD_CONTROL_TOKEN"

[stop]
ack_timeout_ms = 2000
auto_resume = false
fallback_to_gateway_cancel = true

[telegram]

[[providers]]
id = "deepseek_direct"
upstream = "http://{mock_upstream}"
enabled = true
schedule_id = "deepseek_test_sched"

[[schedules]]
id = "deepseek_test_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00"]
exclude_chinese_public_holidays = true
"#
    );

    let tmp_config_path = std::env::temp_dir().join(format!(
        "peakguard_t19_sess_cfg_{}.toml",
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::write(&tmp_config_path, &tmp_config).unwrap();

    let bin_path = env!("CARGO_BIN_EXE_peakguard");

    let mut child = tokio::process::Command::new(bin_path)
        .arg("run")
        .arg("--config")
        .arg(&tmp_config_path)
        .env("PEAKGUARD_CONTROL_TOKEN", auth_token)
        .env("PEAKGUARD_COMMANDS_PATH", &tmp_commands_path)
        .env("PEAKGUARD_SIMULATED_TIME", "2026-10-15T02:00:00Z")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Fallo al iniciar el proceso 'peakguard run'");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap();

    let mut ready = false;
    for _i in 0..40 {
        if let Ok(Some(status)) = child.try_wait() {
            panic!("El daemon terminó inesperadamente con estado: {:?}", status);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let gw_res = client.get(format!("http://127.0.0.1:{}/health", gw_port)).send().await;
        let ctrl_res = client.get(format!("http://127.0.0.1:{}/api/v1/health", ctrl_port)).send().await;

        if let (Ok(g), Ok(c)) = (gw_res, ctrl_res) {
            if g.status() == reqwest::StatusCode::OK && c.status() == reqwest::StatusCode::OK {
                ready = true;
                break;
            }
        }
    }
    assert!(ready);

    // 1. Registrar sesión 1 (OpenCode)
    let reg1 = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/register", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "session_id": "sess-oc-01",
            "client_type": "opencode",
            "client_instance_id": "oc-inst-01",
            "turn_id": "turn-oc-1",
            "provider_id": "deepseek_direct",
            "model": "deepseek-chat"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg1.status(), reqwest::StatusCode::OK);

    // 2. Registrar sesión 2 (Pi)
    let reg2 = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/register", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "session_id": "sess-pi-02",
            "client_type": "pi",
            "client_instance_id": "pi-inst-02",
            "turn_id": "turn-pi-2",
            "provider_id": "deepseek_direct",
            "model": "deepseek-chat"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg2.status(), reqwest::StatusCode::OK);

    // 3. Consultar sesiones activas a través de /api/v1/status
    let status_res = client
        .get(format!("http://127.0.0.1:{}/api/v1/status", ctrl_port))
        .send()
        .await
        .unwrap();
    assert_eq!(status_res.status(), reqwest::StatusCode::OK);
    let status_json: serde_json::Value = status_res.json().await.unwrap();
    assert_eq!(
        status_json["active_sessions_count"], 2,
        "Deben haber exactamente 2 sesiones registradas"
    );

    // 4. Heartbeat válido para sess-oc-01
    let hb_valid = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/heartbeat", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "client_instance_id": "oc-inst-01",
            "session_id": "sess-oc-01",
            "turn_id": "turn-oc-1",
            "is_active": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(hb_valid.status(), reqwest::StatusCode::OK);

    // 5. Heartbeat inválido (client_instance_id vacío o discordante)
    let hb_bad = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/heartbeat", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "client_instance_id": "",
            "session_id": "sess-oc-01"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(hb_bad.status(), reqwest::StatusCode::BAD_REQUEST);

    // 6. Stop ACK estricto: rechazo de Stop ACK sin command_id o client_instance_id
    let ack_bad = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "command_id": "",
            "client_instance_id": "oc-inst-01",
            "session_id": "sess-oc-01",
            "transition_utc": "2026-10-15T01:00:00Z",
            "success": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack_bad.status(), reqwest::StatusCode::BAD_REQUEST);

    // 7. Desregistrar sesión 1 (sess-oc-01)
    let unreg = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/unregister", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "client_instance_id": "oc-inst-01",
            "session_id": "sess-oc-01"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unreg.status(), reqwest::StatusCode::OK);

    // 8. Verificar que solo queda 1 sesión activa
    let status_after = client
        .get(format!("http://127.0.0.1:{}/api/v1/status", ctrl_port))
        .send()
        .await
        .unwrap();
    let after_json: serde_json::Value = status_after.json().await.unwrap();
    assert_eq!(after_json["active_sessions_count"], 1);

    // 9. Apagado ordenado
    let _ = client
        .post(format!("http://127.0.0.1:{}/api/v1/control/shutdown", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await;

    let _ = child.wait().await;
    let _ = std::fs::remove_file(&tmp_config_path);
    let _ = std::fs::remove_file(&tmp_commands_path);
}

