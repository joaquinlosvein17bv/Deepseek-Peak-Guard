use axum::{routing::post, Json, Router};
use chrono::{DateTime, Utc};
use peakguard_core::clock::TestClock;
use peakguard_core::gateway::{create_gateway_router, GatewayState};
use peakguard_core::policy::{OperatingMode, PolicyEngine};
use peakguard_core::registry::{ModelInfo, ModelRegistry, ProviderInfo};
use peakguard_core::schedule::Schedule;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;

// Mock Upstream Server que cuenta cuántas peticiones recibe
async fn run_mock_upstream(counter: Arc<AtomicUsize>) -> SocketAddr {
    let app = Router::new().route(
        "/chat/completions",
        post(move || {
            let c = counter.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Json(serde_json::json!({
                    "id": "chatcmpl-mock-123",
                    "object": "chat.completion",
                    "choices": [
                        { "message": { "role": "assistant", "content": "Hola desde upstream mock!" } }
                    ]
                }))
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
async fn test_gateway_allow_and_block_e2e() {
    let mock_counter = Arc::new(AtomicUsize::new(0));
    let mock_addr = run_mock_upstream(mock_counter.clone()).await;

    // Configurar registro apuntando al mock upstream
    let mut registry = ModelRegistry::default();
    registry.register_provider(ProviderInfo {
        id: "deepseek_direct".to_string(),
        upstream_url: format!("http://{}", mock_addr),
        schedule_id: "deepseek_direct_verified".to_string(),
        enabled: true,
    });
    registry.register_model(ModelInfo {
        id: "deepseek-chat".to_string(),
        name: "DeepSeek Chat".to_string(),
        aliases: vec![],
        provider_id: "deepseek_direct".to_string(),
    });
    let reg_arc = Arc::new(registry);

    // Horario: Lun-Vie 01:00-04:00 UTC es peak
    let mut schedules = HashMap::new();
    let sched = Schedule::new(
        "deepseek_direct_verified",
        &["01:00-04:00".to_string()],
        &["mon".to_string(), "tue".to_string(), "wed".to_string(), "thu".to_string(), "fri".to_string()],
        true,
        &[],
    );
    schedules.insert("deepseek_direct_verified".to_string(), sched);

    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::StrictBlock,
        reg_arc.clone(),
        schedules,
    ));

    // Reloj inicial: 00:30 UTC (OFF-PEAK)
    let off_peak_time = DateTime::parse_from_rfc3339("2026-10-19T00:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(TestClock::new(off_peak_time));

    let gateway_state = GatewayState {
        policy_engine: policy_engine.clone(),
        registry: reg_arc.clone(),
        clock: clock.clone(),
        http_client: reqwest::Client::new(),
    };

    let router = create_gateway_router(gateway_state);
    let gw_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_addr = gw_listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(gw_listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let gw_url = format!("http://{}/v1/chat/completions", gw_addr);

    // --- TEST 1: OFF-PEAK -> Debe transmitir al upstream y devolver HTTP 200 ---
    let resp1 = client
        .post(&gw_url)
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "Test off-peak" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp1.status(), reqwest::StatusCode::OK);
    let body1: serde_json::Value = resp1.json().await.unwrap();
    assert_eq!(body1["choices"][0]["message"]["content"], "Hola desde upstream mock!");
    assert_eq!(mock_counter.load(Ordering::SeqCst), 1);

    // --- TEST 2: PEAK (02:00 UTC) en StrictBlock -> Debe bloquear localmente con HTTP 403 y CERO llamadas upstream ---
    let peak_time = DateTime::parse_from_rfc3339("2026-10-19T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    clock.set_time(peak_time);

    let resp2 = client
        .post(&gw_url)
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "Test peak" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp2.status(), reqwest::StatusCode::FORBIDDEN);
    let body2: serde_json::Value = resp2.json().await.unwrap();
    assert_eq!(body2["error"]["code"], "peakguard_policy_blocked");
    assert_eq!(body2["error"]["mode"], "strict_block");
    assert_eq!(body2["error"]["retry_automatically"], false);

    // Verificación crítica: el contador del mock upstream NO se incrementó; cero bytes salieron
    assert_eq!(mock_counter.load(Ordering::SeqCst), 1);
}
