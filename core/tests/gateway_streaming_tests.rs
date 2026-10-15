use axum::{
    body::Body,
    response::Response,
    routing::post,
    Router,
};
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

// Mock Upstream que emite stream SSE con 3 chunks
async fn run_mock_streaming_upstream(active_streams: Arc<AtomicUsize>) -> SocketAddr {
    let app = Router::new().route(
        "/chat/completions",
        post(move || {
            let active = active_streams.clone();
            async move {
                active.fetch_add(1, Ordering::SeqCst);
                let stream = async_stream::stream! {
                    for i in 1..=3 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        let chunk = format!(
                            "data: {{\"id\":\"cmpl-1\",\"choices\":[{{\"delta\":{{\"content\":\"token {}\"}}}}]}}\n\n",
                            i
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
async fn test_streaming_sse_and_concurrency_20_streams() {
    let active_counter = Arc::new(AtomicUsize::new(0));
    let mock_addr = run_mock_streaming_upstream(active_counter.clone()).await;

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

    // OFF-PEAK inicial (00:30 UTC)
    let off_peak = DateTime::parse_from_rfc3339("2026-10-19T00:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(TestClock::new(off_peak));

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

    let gw_url = format!("http://{}/v1/chat/completions", gw_addr);

    // Ejecutar 20 streams concurrentes simulados
    let mut tasks = Vec::new();
    for i in 0..20 {
        let url = gw_url.clone();
        let session_id = format!("session-test-{}", i);
        let task = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let resp = client
                .post(&url)
                .header("x-session-id", &session_id)
                .json(&serde_json::json!({
                    "model": "deepseek-chat",
                    "stream": true,
                    "messages": [{ "role": "user", "content": format!("Hello from stream {}", i) }]
                }))
                .send()
                .await
                .unwrap();

            assert_eq!(resp.status(), reqwest::StatusCode::OK);
            assert!(resp
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("text/event-stream"));

            let mut stream = resp.bytes_stream();
            let mut chunk_count = 0;
            while let Some(item) = stream.next().await {
                let bytes = item.unwrap();
                let text = String::from_utf8_lossy(&bytes);
                if text.contains("data:") {
                    chunk_count += 1;
                }
            }
            // Debe haber recibido los chunks emitidos
            assert!(chunk_count >= 1);
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await.unwrap();
    }

    // Comprobar que los 20 streams se completaron en el upstream mock
    assert_eq!(active_counter.load(Ordering::SeqCst), 20);
}
