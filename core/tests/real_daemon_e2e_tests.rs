use axum::{response::IntoResponse, routing::post, Json, Router};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

async fn run_mock_upstream_daemon(call_counter: Arc<AtomicUsize>) -> SocketAddr {
    let app = Router::new().route(
        "/chat/completions",
        post(move |Json(payload): Json<serde_json::Value>| {
            let counter = call_counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                let model = payload.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");
                (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!({
                        "id": "cmpl-mock-daemon",
                        "object": "chat.completion",
                        "model": model,
                        "choices": [{
                            "index": 0,
                            "message": { "role": "assistant", "content": "Mock upstream live response" }
                        }]
                    })),
                )
                    .into_response()
            }
        }),
    );

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
async fn test_real_daemon_binary_lifecycle_and_ports() {
    let call_counter = Arc::new(AtomicUsize::new(0));
    let mock_upstream = run_mock_upstream_daemon(call_counter.clone()).await;

    // Asignación de puertos efímeros dinámicos
    let gw_port = get_free_port();
    let ctrl_port = get_free_port();
    let auth_token = "e2e-secret-token-12345";

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
intervals = ["01:00-04:00", "06:00-10:00"]
exclude_chinese_public_holidays = true
"#
    );

    let tmp_config_path = std::env::temp_dir().join(format!("peakguard_test_{}.toml", std::process::id()));
    std::fs::write(&tmp_config_path, &tmp_config).expect("No se pudo escribir archivo de prueba temporal");

    // Localizar el ejecutable real compilado
    let bin_path = env!("CARGO_BIN_EXE_peakguard");

    // Iniciar el proceso real 'peakguard run --config <path>' inyectando el token por variable de entorno
    let mut child = tokio::process::Command::new(bin_path)
        .arg("run")
        .arg("--config")
        .arg(&tmp_config_path)
        .env("PEAKGUARD_CONTROL_TOKEN", auth_token)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Fallo al iniciar el proceso 'peakguard run'");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    // 1. Esperar que ambos puertos reales (Gateway y Control API) estén listos
    let mut ready = false;
    for _ in 0..40 {
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
    assert!(ready, "Los servidores del Gateway y Control API no estuvieron listos en el tiempo esperado");

    // 2. Probar Control API: GET /api/v1/status (Honesta verificación de cobertura)
    let status_resp = client
        .get(format!("http://127.0.0.1:{}/api/v1/status", ctrl_port))
        .send()
        .await
        .unwrap();
    assert_eq!(status_resp.status(), reqwest::StatusCode::OK);
    let status_json: serde_json::Value = status_resp.json().await.unwrap();
    assert_eq!(status_json["mode"], "restricted_stop");
    // T18 Paso 3: Cobertura honesta. Antes de tráfico observado o sesión activa, es PARTIAL
    assert_eq!(status_json["coverage"], "PARTIAL");

    // 3. T17.1: Verificar que peticiones administrativas sin autenticación SON RECHAZADAS (401 Unauthorized)
    let unauth_mode = client
        .put(format!("http://127.0.0.1:{}/api/v1/config/mode", ctrl_port))
        .json(&serde_json::json!({ "mode": "notify_only" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_mode.status(), reqwest::StatusCode::UNAUTHORIZED);

    let unauth_ack = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .json(&serde_json::json!({
            "session_id": "s1",
            "transition_utc": "2026-10-15T01:00:00Z",
            "success": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_ack.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 4. T17.1: Probar cambio dinámico de modo AUTORIZADO con Bearer Token
    let mode_put_resp = client
        .put(format!("http://127.0.0.1:{}/api/v1/config/mode", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({ "mode": "notify_only" }))
        .send()
        .await
        .unwrap();
    assert_eq!(mode_put_resp.status(), reqwest::StatusCode::OK);

    // Verificar que el estado refleja el nuevo modo
    let status_resp2 = client
        .get(format!("http://127.0.0.1:{}/api/v1/status", ctrl_port))
        .send()
        .await
        .unwrap();
    let status_json2: serde_json::Value = status_resp2.json().await.unwrap();
    assert_eq!(status_json2["mode"], "notify_only");

    // 5. T17.2: Probar registro de sesión activa vía Control API
    let reg_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/register", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "session_id": "test-session-e2e-01",
            "client_type": "opencode",
            "turn_id": "turn-001",
            "model": "deepseek-chat",
            "provider": "deepseek_direct",
            "schedule_id": "deepseek_test_sched",
            "is_active": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg_resp.status(), reqwest::StatusCode::OK);

    // Heartbeat de sesión
    let hb_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/sessions/heartbeat", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "session_id": "test-session-e2e-01",
            "turn_id": "turn-001",
            "is_active": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(hb_resp.status(), reqwest::StatusCode::OK);

    // 6. T17.1: Validación estricta de acuses (Rechazo de solicitud sin 'success' explícito -> 422)
    let ack_no_success = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "session_id": "test-session-ack-invalid",
            "transition_utc": "2026-10-15T01:00:00Z"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack_no_success.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    // Stop ACK exitoso autorizado
    let ack_ok_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "client_instance_id": "inst_daemon_test",
            "session_id": "test-session-ack-ok",
            "transition_utc": "2026-10-15T01:00:00Z",
            "success": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack_ok_resp.status(), reqwest::StatusCode::OK);

    // Stop ACK con fallo autorizado
    let ack_fail_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({
            "client_instance_id": "inst_daemon_test",
            "session_id": "test-session-ack-fail",
            "transition_utc": "2026-10-15T01:00:00Z",
            "success": false,
            "error": "Error simulado en editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack_fail_resp.status(), reqwest::StatusCode::ACCEPTED);

    // 7. Enrutamiento por Gateway hacia Mock Upstream
    let comp_resp = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "ping daemon" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(comp_resp.status(), reqwest::StatusCode::OK);
    assert_eq!(call_counter.load(Ordering::SeqCst), 1);

    // T18: Tras tráfico observado por el gateway, la cobertura se actualiza a PROTECTED
    let status_after_resp = client
        .get(format!("http://127.0.0.1:{}/api/v1/status", ctrl_port))
        .send()
        .await
        .unwrap();
    let status_after_json: serde_json::Value = status_after_resp.json().await.unwrap();
    assert_eq!(status_after_json["coverage"], "PROTECTED");

    // 8. T17.8: Apagado ordenado limpio mediante Control API (sin child.kill())
    let shutdown_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/control/shutdown", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .unwrap();
    assert_eq!(shutdown_resp.status(), reqwest::StatusCode::OK);

    // Esperar a que el proceso termine ordenadamente por su cuenta
    let exit_status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("El proceso no se apagó ordenadamente dentro del tiempo límite")
        .expect("Fallo al esperar terminación del proceso");

    assert!(exit_status.success(), "El proceso daemon terminó con código de error");
    let _ = std::fs::remove_file(&tmp_config_path);
}

#[tokio::test]
async fn test_real_daemon_simulated_time_peak_transition() {
    let call_counter = Arc::new(AtomicUsize::new(0));
    let mock_upstream = run_mock_upstream_daemon(call_counter.clone()).await;

    let gw_port = get_free_port();
    let ctrl_port = get_free_port();
    let auth_token = "e2e-warp-token-67890";

    // Configuración en modo strict_block con intervalo pico 01:00-04:00 UTC
    let tmp_config = format!(
        r#"config_version = 1

[general]
enabled = true
mode = "strict_block"
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
schedule_id = "opencode_offpeak_sched"

[[schedules]]
id = "deepseek_test_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["01:00-04:00"]
exclude_chinese_public_holidays = true

[[schedules]]
id = "opencode_offpeak_sched"
timezone = "UTC"
weekdays = ["mon", "tue", "wed", "thu", "fri"]
intervals = ["12:00-16:00"]
exclude_chinese_public_holidays = true
"#
    );

    let tmp_config_path = std::env::temp_dir().join(format!("peakguard_test_warp_{}.toml", std::process::id()));
    std::fs::write(&tmp_config_path, &tmp_config).expect("No se pudo escribir archivo de prueba temporal");

    let bin_path = env!("CARGO_BIN_EXE_peakguard");

    // Iniciar el daemon con hora simulada en pleno horario pico (Jueves 2026-10-15 02:30:00 UTC)
    let mut child = tokio::process::Command::new(bin_path)
        .arg("run")
        .arg("--config")
        .arg(&tmp_config_path)
        .env("PEAKGUARD_CONTROL_TOKEN", auth_token)
        .env("PEAKGUARD_SIMULATED_TIME", "2026-10-15T02:30:00Z")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Fallo al iniciar el proceso 'peakguard run'");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    let mut ready = false;
    for _ in 0..40 {
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
    assert!(ready, "Daemon con reloj simulado no estuvo listo");

    // 1. Petición con modelo protegido 'deepseek-chat' en pleno horario pico -> DEBE SER BLOQUEADO (429)
    let blocked_resp = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "peticion bloqueada" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(blocked_resp.status(), reqwest::StatusCode::FORBIDDEN);
    let blocked_json: serde_json::Value = blocked_resp.json().await.unwrap();
    assert!(blocked_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("horario peak"));

    // 2. Petición con modelo 'deepseek-v4.1-flash' (perfil independiente en horario off-peak) -> NUNCA DEBE SER BLOQUEADO (200 OK)
    let allowed_resp = client
        .post(format!("http://127.0.0.1:{}/v1/chat/completions", gw_port))
        .json(&serde_json::json!({
            "model": "deepseek-v4.1-flash",
            "messages": [{ "role": "user", "content": "peticion permitida" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(allowed_resp.status(), reqwest::StatusCode::OK);
    let allowed_json: serde_json::Value = allowed_resp.json().await.unwrap();
    assert_eq!(allowed_json["model"], "deepseek-v4.1-flash");

    // 3. Apagado ordenado mediante Control API
    let shutdown_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/control/shutdown", ctrl_port))
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .unwrap();
    assert_eq!(shutdown_resp.status(), reqwest::StatusCode::OK);

    let exit_status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("El proceso no se apagó ordenadamente")
        .expect("Fallo al esperar proceso");

    assert!(exit_status.success());
    let _ = std::fs::remove_file(&tmp_config_path);
}
