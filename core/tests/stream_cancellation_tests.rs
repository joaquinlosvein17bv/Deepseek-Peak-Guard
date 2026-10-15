use axum::{body::Body, response::Response, routing::post, Router};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
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
use tokio::sync::broadcast;

async fn run_mock_slow_upstream(active_streams: Arc<AtomicUsize>) -> SocketAddr {
    let app = Router::new().route(
        "/chat/completions",
        post(move || {
            let active = active_streams.clone();
            async move {
                active.fetch_add(1, Ordering::SeqCst);
                let stream = async_stream::stream! {
                    for i in 1..=5 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
                        let chunk = format!(
                            "data: {{\"id\":\"cmpl-{}\",\"choices\":[{{\"delta\":{{\"content\":\"chunk {}\"}}}}]}}\n\n",
                            i, i
                        );
                        yield Ok::<_, std::io::Error>(chunk);
                    }
                    yield Ok("data: [DONE]\n\n".to_string());
                };

                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(stream))
                    .unwrap()
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
async fn test_active_stream_cancellation_on_peak_transition() {
    let counter = Arc::new(AtomicUsize::new(0));
    let upstream_addr = run_mock_slow_upstream(counter.clone()).await;

    let mut registry = ModelRegistry::default();
    registry.register_provider(ProviderInfo {
        id: "deepseek_direct".to_string(),
        upstream_url: format!("http://{}", upstream_addr),
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

    let mut schedules = HashMap::new();
    let sched = Schedule::new(
        "deepseek_direct_verified",
        &["01:00-04:00".to_string()],
        &[
            "mon".to_string(),
            "tue".to_string(),
            "wed".to_string(),
            "thu".to_string(),
            "fri".to_string(),
        ],
        true,
        &[],
    );
    schedules.insert("deepseek_direct_verified".to_string(), sched);

    // 1. Probar en modo StrictBlock: cancelación activa al entrar en peak
    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::StrictBlock,
        reg_arc.clone(),
        schedules.clone(),
    ));

    // Iniciar 1 segundo antes de peak (00:59:59 UTC)
    let pre_peak = DateTime::parse_from_rfc3339("2026-10-19T00:59:59Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(TestClock::new(pre_peak));
    let (event_tx, mut event_rx) = broadcast::channel(10);

    let gateway_state = GatewayState {
        policy_engine: policy_engine.clone(),
        registry: reg_arc.clone(),
        clock: clock.clone(),
        http_client: reqwest::Client::new(),
        event_tx: Some(event_tx),
    };

    let router = create_gateway_router(gateway_state);
    let gw_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_addr = gw_listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(gw_listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{}/v1/chat/completions", gw_addr))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "stream": true,
            "messages": [{ "role": "user", "content": "stream test" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let mut stream = resp.bytes_stream();

    // Recibir primer chunk en off-peak
    let first_item = stream.next().await.unwrap().unwrap();
    let first_chunk = String::from_utf8_lossy(&first_item);
    assert!(first_chunk.contains("chunk 1"));

    // Avanzar reloj dentro de la franja Peak (01:00:05 UTC)
    clock.set_time(
        DateTime::parse_from_rfc3339("2026-10-19T01:00:05Z")
            .unwrap()
            .with_timezone(&Utc),
    );

    // El siguiente chunk debe ser la señal de cancelación y el stream debe terminar
    let second_item = stream.next().await.unwrap().unwrap();
    let second_chunk = String::from_utf8_lossy(&second_item);
    assert!(
        second_chunk.contains("peakguard_stream_cancelled_peak"),
        "El stream debe emitir chunk de error al entrar en horario peak"
    );

    // El stream debe finalizar inmediatamente después
    let third_item = stream.next().await;
    assert!(
        third_item.is_none(),
        "El stream debió cerrarse inmediatamente tras la cancelación"
    );

    // Verificar que se emitió el evento por el canal SSE
    let sse_event = event_rx.try_recv().unwrap();
    assert!(sse_event.contains("stream_cancelled"));
}

#[tokio::test]
async fn test_active_stream_uninterrupted_in_notify_only_mode() {
    let counter = Arc::new(AtomicUsize::new(0));
    let upstream_addr = run_mock_slow_upstream(counter.clone()).await;

    let mut registry = ModelRegistry::default();
    registry.register_provider(ProviderInfo {
        id: "deepseek_direct".to_string(),
        upstream_url: format!("http://{}", upstream_addr),
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

    let mut schedules = HashMap::new();
    let sched = Schedule::new(
        "deepseek_direct_verified",
        &["01:00-04:00".to_string()],
        &[
            "mon".to_string(),
            "tue".to_string(),
            "wed".to_string(),
            "thu".to_string(),
            "fri".to_string(),
        ],
        true,
        &[],
    );
    schedules.insert("deepseek_direct_verified".to_string(), sched);

    // En modo NotifyOnly: el stream no debe cancelarse
    let policy_engine = Arc::new(PolicyEngine::new(
        OperatingMode::NotifyOnly,
        reg_arc.clone(),
        schedules.clone(),
    ));

    let pre_peak = DateTime::parse_from_rfc3339("2026-10-19T00:59:59Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(TestClock::new(pre_peak));

    let gateway_state = GatewayState {
        policy_engine: policy_engine.clone(),
        registry: reg_arc.clone(),
        clock: clock.clone(),
        http_client: reqwest::Client::new(),
        event_tx: None,
    };

    let router = create_gateway_router(gateway_state);
    let gw_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_addr = gw_listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(gw_listener, router).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{}/v1/chat/completions", gw_addr))
        .json(&serde_json::json!({
            "model": "deepseek-chat",
            "stream": true,
            "messages": [{ "role": "user", "content": "stream notify test" }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let mut stream = resp.bytes_stream();

    // Recibir primer chunk en off-peak
    let first_item = stream.next().await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&first_item).contains("chunk 1"));

    // Avanzar reloj a horario peak
    clock.set_time(
        DateTime::parse_from_rfc3339("2026-10-19T01:00:05Z")
            .unwrap()
            .with_timezone(&Utc),
    );

    // En NotifyOnly, todos los chunks restantes deben recibirse sin cancelación
    let mut total_chunks = 1;
    let mut received_done = false;
    while let Some(item) = stream.next().await {
        let bytes = item.unwrap();
        let chunk_text = String::from_utf8_lossy(&bytes);
        if chunk_text.contains("chunk") {
            total_chunks += 1;
        }
        if chunk_text.contains("[DONE]") {
            received_done = true;
        }
    }

    assert_eq!(total_chunks, 5, "Deben recibirse los 5 chunks en notify_only");
    assert!(received_done, "Debe recibirse [DONE] en notify_only");
}
