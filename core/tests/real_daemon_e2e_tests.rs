use axum::{response::IntoResponse, routing::post, Json, Router};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

async fn run_mock_upstream_daemon(call_counter: Arc<AtomicUsize>) -> SocketAddr {
    let app = Router::new().route(
        "/chat/completions",
        post(move || {
            let counter = call_counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!({
                        "id": "cmpl-mock-daemon",
                        "object": "chat.completion",
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

#[tokio::test]
async fn test_real_daemon_binary_lifecycle_and_ports() {
    let call_counter = Arc::new(AtomicUsize::new(0));
    let mock_upstream = run_mock_upstream_daemon(call_counter.clone()).await;

    // Usar puertos libres dedicados para la prueba del binario real
    let gw_port = 18765;
    let ctrl_port = 18766;

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

    // Iniciar el proceso real 'peakguard run --config <path>'
    let mut child = tokio::process::Command::new(bin_path)
        .arg("run")
        .arg("--config")
        .arg(&tmp_config_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Fallo al iniciar el proceso 'peakguard run'");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();

    // 1. Esperar que ambos puertos reales (Gateway 18765 y Control API 18766) estén escuchando y respondan OK
    let mut ready = false;
    for _ in 0..30 {
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

    // 2. Probar Control API: GET /api/v1/status
    let status_resp = client
        .get(format!("http://127.0.0.1:{}/api/v1/status", ctrl_port))
        .send()
        .await
        .unwrap();
    assert_eq!(status_resp.status(), reqwest::StatusCode::OK);
    let status_json: serde_json::Value = status_resp.json().await.unwrap();
    assert_eq!(status_json["mode"], "restricted_stop");
    assert_eq!(status_json["coverage"], "PROTEGIDO");

    // 3. Probar cambio dinámico de modo vía PUT /api/v1/config/mode
    let mode_put_resp = client
        .put(format!("http://127.0.0.1:{}/api/v1/config/mode", ctrl_port))
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

    // 4. Probar Stop ACK exitoso (sin falsos positivos)
    let ack_ok_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .json(&serde_json::json!({
            "session_id": "test-session-ack-ok",
            "transition_utc": "2026-10-15T01:00:00Z",
            "success": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack_ok_resp.status(), reqwest::StatusCode::OK);
    let ack_ok_json: serde_json::Value = ack_ok_resp.json().await.unwrap();
    assert_eq!(ack_ok_json["status"], "acknowledged");
    assert_eq!(ack_ok_json["success"], true);

    // 5. Probar Stop ACK con fallo: debe registrar el fallo y NO reportar éxito
    let ack_fail_resp = client
        .post(format!("http://127.0.0.1:{}/api/v1/adapters/stop-ack", ctrl_port))
        .json(&serde_json::json!({
            "session_id": "test-session-ack-fail",
            "transition_utc": "2026-10-15T01:00:00Z",
            "success": false,
            "error": "Error interno abortando turno en editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack_fail_resp.status(), reqwest::StatusCode::ACCEPTED);
    let ack_fail_json: serde_json::Value = ack_fail_resp.json().await.unwrap();
    assert_eq!(ack_fail_json["status"], "failed");
    assert_eq!(ack_fail_json["success"], false);
    assert_eq!(ack_fail_json["fallback"], "gateway_cancel");

    // 6. Probar enrutamiento por el Gateway HTTP hacia el mock upstream
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
    let comp_json: serde_json::Value = comp_resp.json().await.unwrap();
    assert_eq!(
        comp_json["choices"][0]["message"]["content"],
        "Mock upstream live response"
    );
    assert_eq!(call_counter.load(Ordering::SeqCst), 1);

    // 7. Apagado del proceso hijo y limpieza
    child.kill().await.expect("Fallo al terminar el proceso de prueba");
    let _ = std::fs::remove_file(&tmp_config_path);
}
