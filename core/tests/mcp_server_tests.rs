use chrono::{TimeZone, Utc};
use peakguard_core::clock::TestClock;
use peakguard_core::config::Config;
use peakguard_core::mcp::McpServer;
use serde_json::{json, Value};
use std::sync::Arc;

fn create_test_server(clock: Arc<TestClock>) -> McpServer {
    let mut config = Config::default();
    config.general.mode = "restricted_stop".to_string();
    McpServer::with_clock(config, clock)
}

#[test]
fn test_mcp_initialize_and_ping() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock);

    // 1. Initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "opencode", "version": "1.18.35" }
        }
    });
    let resp_str = server.handle_json_rpc(&init_req.to_string()).expect("Respuesta esperada");
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp["result"]["serverInfo"]["name"], "peakguard");
    assert_eq!(resp["result"]["serverInfo"]["version"], "0.1.0");

    // 2. notifications/initialized (no response required)
    let notif = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let notif_resp = server.handle_json_rpc(&notif.to_string());
    assert!(notif_resp.is_none());

    // 3. Ping
    let ping_req = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "ping"
    });
    let ping_resp_str = server.handle_json_rpc(&ping_req.to_string()).expect("Respuesta a ping");
    let ping_resp: Value = serde_json::from_str(&ping_resp_str).unwrap();
    assert_eq!(ping_resp["id"], 42);
    assert_eq!(ping_resp["result"], json!({}));
}

#[test]
fn test_mcp_tools_list() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock);

    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let resp_str = server.handle_json_rpc(&list_req.to_string()).expect("Respuesta a tools/list");
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    let tools = resp["result"]["tools"].as_array().expect("Array de tools");
    assert_eq!(tools.len(), 4, "Debe exponer exactamente las 4 herramientas requeridas por §9.4");

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"peakguard_status"));
    assert!(tool_names.contains(&"peakguard_next_peak"));
    assert!(tool_names.contains(&"peakguard_models"));
    assert!(tool_names.contains(&"peakguard_coverage"));
}

#[test]
fn test_mcp_status_tool_transitions() {
    // Miércoles 00:00:00 UTC -> Off-Peak
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock.clone());

    let status_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "peakguard_status",
            "arguments": {}
        }
    });

    let resp_str = server.handle_json_rpc(&status_req.to_string()).expect("Respuesta de status");
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    assert_eq!(resp["result"]["isError"], false);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let status_data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(status_data["is_peak"], false);
    assert_eq!(status_data["status"], "OFF_PEAK_SAFE");
    assert_eq!(status_data["mode"], "restricted_stop");
    assert_eq!(status_data["next_transition_type"], "peak_start");
    assert_eq!(status_data["seconds_to_transition"], 3600);

    // Avanzar a Miércoles 02:30:00 UTC -> Horario Peak (01:00-04:00)
    clock.set_time(Utc.with_ymd_and_hms(2026, 10, 14, 2, 30, 0).unwrap());
    let resp_str2 = server.handle_json_rpc(&status_req.to_string()).expect("Respuesta de status 2");
    let resp2: Value = serde_json::from_str(&resp_str2).unwrap();
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    let status_data2: Value = serde_json::from_str(text2).unwrap();
    assert_eq!(status_data2["is_peak"], true);
    assert_eq!(status_data2["status"], "ACTIVE_PEAK");
    assert_eq!(status_data2["next_transition_type"], "off_peak_start");
    // 04:00:00 - 02:30:00 = 90 minutos = 5400 segundos
    assert_eq!(status_data2["seconds_to_transition"], 5400);
}

#[test]
fn test_mcp_next_peak_tool() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 15, 0).unwrap()));
    let server = create_test_server(clock);

    let next_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "peakguard_next_peak",
            "arguments": {}
        }
    });

    let resp_str = server.handle_json_rpc(&next_req.to_string()).expect("Respuesta");
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    assert_eq!(resp["result"]["isError"], false);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let next_data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(next_data["is_peak_currently"], false);
    // 01:00:00 - 00:15:00 = 45 min = 2700 s
    assert_eq!(next_data["seconds_remaining"], 2700);
    assert_eq!(next_data["human_readable_remaining"], "0h 45m 0s");
}

#[test]
fn test_mcp_models_tool() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "peakguard_models",
            "arguments": {}
        }
    });

    let resp_str = server.handle_json_rpc(&req.to_string()).expect("Respuesta");
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let models_data: Value = serde_json::from_str(text).unwrap();
    let list = models_data["registered_models"].as_array().expect("Array");
    assert!(list.len() >= 3);
    let ids: Vec<&str> = list.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"deepseek-chat"));
    assert!(ids.contains(&"deepseek-reasoner"));
    assert!(ids.contains(&"deepseek-v4.1-flash"));
}

#[test]
fn test_mcp_coverage_tool() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock);

    // 1. DeepSeek Chat
    let req1 = json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "peakguard_coverage",
            "arguments": {
                "model_id": "deepseek-chat"
            }
        }
    });
    let resp_str1 = server.handle_json_rpc(&req1.to_string()).unwrap();
    let resp1: Value = serde_json::from_str(&resp_str1).unwrap();
    let text1 = resp1["result"]["content"][0]["text"].as_str().unwrap();
    let cov1: Value = serde_json::from_str(text1).unwrap();
    assert_eq!(cov1["is_deepseek_family"], true);
    assert_eq!(cov1["coverage_through_gateway"], "Protected");
    assert_eq!(cov1["coverage_direct_bypass"], "Uncovered");

    // 2. Modelo de terceros (Claude) -> Nunca bloqueado (P2)
    let req2 = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "peakguard_coverage",
            "arguments": {
                "model_id": "claude-3-5-sonnet-20241022"
            }
        }
    });
    let resp_str2 = server.handle_json_rpc(&req2.to_string()).unwrap();
    let resp2: Value = serde_json::from_str(&resp_str2).unwrap();
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    let cov2: Value = serde_json::from_str(text2).unwrap();
    assert_eq!(cov2["is_deepseek_family"], false);
    assert_eq!(cov2["policy_action_during_peak"], "Permitido (sin bloqueo)");

    // 3. Sin argumento model_id
    let req3 = json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": {
            "name": "peakguard_coverage",
            "arguments": {}
        }
    });
    let resp_str3 = server.handle_json_rpc(&req3.to_string()).unwrap();
    let resp3: Value = serde_json::from_str(&resp_str3).unwrap();
    assert_eq!(resp3["result"]["isError"], true);
}

#[test]
fn test_mcp_security_guard_disallows_mutation() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock);

    // Intento de llamar a una tool de cambio de modo (set_mode)
    let req = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": {
            "name": "set_mode",
            "arguments": { "mode": "notify_only" }
        }
    });

    let resp_str = server.handle_json_rpc(&req.to_string()).expect("Respuesta");
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    assert_eq!(resp["result"]["isError"], true, "Debe ser marcado con isError: true");
    let error_text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        error_text.contains("§9.4") && error_text.contains("deshabilitadas"),
        "El mensaje debe clarificar la salvaguarda de seguridad §9.4 contra auto-desbloqueo"
    );
}

#[test]
fn test_mcp_invalid_json_and_unknown_methods() {
    let clock = Arc::new(TestClock::new(Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0).unwrap()));
    let server = create_test_server(clock);

    // 1. JSON inválido
    let resp_str = server.handle_json_rpc("not valid json {").unwrap();
    let resp: Value = serde_json::from_str(&resp_str).unwrap();
    assert_eq!(resp["error"]["code"], -32700);

    // 2. Método inexistente
    let req = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "non_existent_method"
    });
    let resp_str2 = server.handle_json_rpc(&req.to_string()).unwrap();
    let resp2: Value = serde_json::from_str(&resp_str2).unwrap();
    assert_eq!(resp2["error"]["code"], -32601);
}
