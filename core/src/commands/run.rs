use crate::clock::{Clock, SystemClock, TestClock};
use crate::config::Config;
use crate::control_api::{self, ControlApiState};
use crate::gateway::{self, GatewayState};
use crate::notifications::{DeduplicationKey, NotificationEngine};
use crate::policy::{OperatingMode, PolicyEngine};
use crate::registry::{ModelRegistry, SessionRegistry};
use crate::schedule::{chinese_public_holidays_2026, Schedule};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{error, info};

pub async fn execute<P: AsRef<Path>>(config_path: P) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path.as_ref();
    info!("Iniciando PeakGuard Daemon...");

    let cfg = Config::load_from_file(path)?;
    info!("Configuración cargada correctamente desde: {}", path.display());

    let mode = OperatingMode::parse(&cfg.general.mode).unwrap_or(OperatingMode::RestrictedStop);
    info!("Modo activo de operación: {}", mode.as_str());

    // 1. Configurar ModelRegistry
    let mut registry = ModelRegistry::default();
    for p in &cfg.providers {
        if let Some(upstream) = &p.upstream {
            registry.register_provider(crate::registry::ProviderInfo {
                id: p.id.clone(),
                upstream_url: upstream.clone(),
                schedule_id: p.schedule_id.clone(),
                enabled: p.enabled,
            });
        }
    }
    let reg_arc = Arc::new(registry);

    // 2. Configurar Schedules (incorporando festivos configurables de cualquier año)
    let mut schedules = HashMap::new();
    for s in &cfg.schedules {
        let mut holidays = s.holidays.clone();
        if s.exclude_chinese_public_holidays {
            for h in chinese_public_holidays_2026() {
                if !holidays.contains(&h) {
                    holidays.push(h);
                }
            }
        }
        let sched = Schedule::new(
            &s.id,
            &s.intervals,
            &s.weekdays,
            s.exclude_chinese_public_holidays,
            &holidays,
        );
        schedules.insert(s.id.clone(), sched);
    }
    if schedules.is_empty() {
        let default_sched = Schedule::default_deepseek();
        schedules.insert(default_sched.id.clone(), default_sched);
    }

    // 3. Configurar PolicyEngine
    let policy_engine = Arc::new(PolicyEngine::new(
        mode,
        reg_arc.clone(),
        schedules.clone(),
    ));

    // 4. Canal de eventos SSE y registro central de sesiones
    let (event_tx, _) = broadcast::channel::<String>(256);
    let session_registry = Arc::new(SessionRegistry::new());

    // Soporte para reloj simulado determinista en pruebas E2E (vía PEAKGUARD_SIMULATED_TIME)
    let (clock, test_clock_opt): (Arc<dyn Clock>, Option<TestClock>) =
        if let Ok(sim_str) = std::env::var("PEAKGUARD_SIMULATED_TIME") {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&sim_str) {
                let tc = TestClock::new(dt.with_timezone(&chrono::Utc));
                info!("MODO DE PRUEBA ACTIVADO: Usando reloj simulado en {}", sim_str);
                (Arc::new(tc.clone()), Some(tc))
            } else {
                (Arc::new(SystemClock), None)
            }
        } else {
            (Arc::new(SystemClock), None)
        };

    // 5. Vincular listeners de Gateway y Control API
    let gw_addr: SocketAddr = format!(
        "{}:{}",
        cfg.gateway.listen_address, cfg.gateway.listen_port
    )
    .parse()?;

    let gw_listener = match TcpListener::bind(gw_addr).await {
        Ok(l) => {
            info!("Gateway HTTP escuchando en http://{}", gw_addr);
            l
        }
        Err(e) => {
            error!(
                "Error crítico vinculando puerto del Gateway ({}): {}",
                gw_addr, e
            );
            return Err(e.into());
        }
    };

    let ctrl_addr: SocketAddr = format!(
        "{}:{}",
        cfg.control.listen_address, cfg.control.listen_port
    )
    .parse()?;

    let ctrl_listener = match TcpListener::bind(ctrl_addr).await {
        Ok(l) => {
            info!("Control API escuchando en http://{}", ctrl_addr);
            l
        }
        Err(e) => {
            error!(
                "Error crítico vinculando puerto de Control API ({}): {}",
                ctrl_addr, e
            );
            return Err(e.into());
        }
    };

    // 6. Autenticación y Construcción de routers de Axum
    let auth_token = match control_api::resolve_or_create_control_token(&cfg.control.token_env) {
        Ok(t) => Arc::new(t),
        Err(e) => {
            error!(
                "Error crítico: Fallo al generar o asegurar credenciales de Control API: {}",
                e
            );
            return Err(e);
        }
    };
    info!(
        "Control API inicializada con autenticación Bearer Token (token_env='{}')",
        cfg.control.token_env
    );

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    let gateway_state = GatewayState {
        policy_engine: policy_engine.clone(),
        registry: reg_arc.clone(),
        clock: clock.clone(),
        http_client: reqwest::Client::new(),
        event_tx: Some(event_tx.clone()),
    };
    let gateway_router = gateway::create_gateway_router(gateway_state);

    let control_state = ControlApiState {
        policy_engine: policy_engine.clone(),
        registry: reg_arc.clone(),
        schedules: Arc::new(schedules.clone()),
        session_registry: session_registry.clone(),
        event_tx: event_tx.clone(),
        clock: clock.clone(),
        auth_token: auth_token.clone(),
        shutdown_tx: Some(shutdown_tx),
    };
    let control_router = control_api::create_control_router(control_state);

    // 7. Configurar motor de notificaciones y monitor en background
    let bot_token = cfg
        .telegram
        .bot_token_env
        .as_ref()
        .and_then(|var| std::env::var(var).ok());
    let chat_id = cfg
        .telegram
        .chat_id_env
        .as_ref()
        .and_then(|var| std::env::var(var).ok());
    let notifier = Arc::new(NotificationEngine::new(bot_token, chat_id));

    let ticker_clock = clock.clone();
    let ticker_schedules = schedules.clone();
    let ticker_event_tx = event_tx.clone();
    let ticker_notifier = notifier.clone();
    let ticker_session_reg = session_registry.clone();
    let ticker_registry = reg_arc.clone();
    let ticker_policy_engine = policy_engine.clone();
    let telegram_enabled = cfg.alerts.telegram;
    let notify_peak_end = cfg.alerts.notify_on_peak_end;

    let monitor_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut schedule_peak_states: HashMap<String, bool> = HashMap::new();

        loop {
            interval.tick().await;
            if let Some(ref tc) = test_clock_opt {
                tc.advance(chrono::Duration::seconds(1));
            }
            let now = ticker_clock.now();

            // Purgar sesiones inactivas (más de 60s sin heartbeat), preservando turnos activos hasta 30m
            ticker_session_reg.prune_stale(chrono::Duration::seconds(60), chrono::Duration::minutes(30));

            for (sched_id, schedule) in &ticker_schedules {
                let is_peak = schedule.is_peak(now);
                let was_peak = schedule_peak_states.entry(sched_id.clone()).or_insert(false);

                // Detección de transición a Peak
                if is_peak && !*was_peak {
                    *was_peak = true;
                    let transition_str = schedule
                        .current_peak_window_start(now)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| now.to_rfc3339());

                    info!(
                        "TRANSICIÓN: Inicio de horario peak detectado para '{}' a las {}",
                        sched_id, transition_str
                    );
                    let _ = ticker_event_tx.send(format!(
                        "{{\"type\":\"peak_started\",\"schedule_id\":\"{}\",\"transition_utc\":\"{}\"}}",
                        sched_id, transition_str
                    ));

                    // En modo RestrictedStop: Enviar stop_requested a los turnos activos de este horario
                    if ticker_policy_engine.get_mode() == OperatingMode::RestrictedStop {
                        let active_sessions = ticker_session_reg.get_active_sessions_for_schedule(sched_id, &ticker_registry);
                        for sess in active_sessions {
                            info!(
                                "EMITIENDO STOP NATIVO: Sesión activa '{}' (cliente '{}', modelo '{}') detenida por inicio de horario peak.",
                                sess.session_id, sess.client_type, sess.model
                            );
                            let _ = ticker_event_tx.send(format!(
                                "{{\"type\":\"stop_requested\",\"session_id\":\"{}\",\"turn_id\":{},\"provider_id\":\"{}\",\"model\":\"{}\",\"transition_utc\":\"{}\",\"reason\":\"Horario peak iniciado. Cancelando turno activo mediante Stop nativo sin cerrar aplicación.\"}}",
                                sess.session_id,
                                sess.turn_id.as_ref().map(|t| format!("\"{}\"", t)).unwrap_or_else(|| "null".to_string()),
                                sess.provider_id,
                                sess.model,
                                transition_str
                            ));
                        }
                    }

                    if telegram_enabled {
                        let key = DeduplicationKey {
                            schedule_id: sched_id.clone(),
                            window_start_utc: transition_str.clone(),
                            threshold: "peak_active".to_string(),
                            channel: "telegram".to_string(),
                        };
                        ticker_notifier
                            .notify_telegram(
                                &key,
                                &format!("🚨 <b>PeakGuard:</b> Horario Peak ACTIVO ({})", sched_id),
                            )
                            .await;
                    }
                } else if !is_peak && *was_peak {
                    // Detección de fin de Peak
                    *was_peak = false;
                    let transition_str = now.to_rfc3339();
                    info!(
                        "TRANSICIÓN: Fin de horario peak para '{}' a las {}",
                        sched_id, transition_str
                    );
                    let _ = ticker_event_tx.send(format!(
                        "{{\"type\":\"peak_ended\",\"schedule_id\":\"{}\",\"transition_utc\":\"{}\"}}",
                        sched_id, transition_str
                    ));

                    if telegram_enabled && notify_peak_end {
                        let key = DeduplicationKey {
                            schedule_id: sched_id.clone(),
                            window_start_utc: transition_str,
                            threshold: "peak_ended".to_string(),
                            channel: "telegram".to_string(),
                        };
                        ticker_notifier
                            .notify_telegram(
                                &key,
                                &format!("✅ <b>PeakGuard:</b> Horario Peak FINALIZADO ({})", sched_id),
                            )
                            .await;
                    }
                }

                // Evaluación de alertas anticipadas (30m, 10m, 5m y cuenta atrás 10s)
                if let Some((peak_start, seconds_left)) = schedule.next_peak(now) {
                    let window_start = peak_start.to_rfc3339();

                    if seconds_left <= 10 && seconds_left >= 1 {
                        let _ = ticker_event_tx.send(format!(
                            "{{\"type\":\"countdown\",\"schedule_id\":\"{}\",\"seconds\":{}}}",
                            sched_id, seconds_left
                        ));
                    }

                    // T-30 min: rango [1200, 1800] s
                    if seconds_left <= 1800 && seconds_left > 1200 {
                        let key = DeduplicationKey {
                            schedule_id: sched_id.clone(),
                            window_start_utc: window_start.clone(),
                            threshold: "warning_30".to_string(),
                            channel: "internal".to_string(),
                        };
                        if ticker_notifier.should_emit(&key) {
                            info!("Emitiendo alerta T-30 min para '{}' ({}s restantes)", sched_id, seconds_left);
                            let _ = ticker_event_tx.send(format!(
                                "{{\"type\":\"warning_30\",\"schedule_id\":\"{}\",\"seconds_left\":{}}}",
                                sched_id, seconds_left
                            ));
                            if telegram_enabled {
                                let tg_key = DeduplicationKey {
                                    schedule_id: sched_id.clone(),
                                    window_start_utc: window_start.clone(),
                                    threshold: "warning_30".to_string(),
                                    channel: "telegram".to_string(),
                                };
                                ticker_notifier
                                    .notify_telegram(
                                        &tg_key,
                                        &format!("⚠️ <b>PeakGuard:</b> Faltan ~30 minutos para el horario Peak ({})", sched_id),
                                    )
                                    .await;
                            }
                        }
                    } else if seconds_left <= 600 && seconds_left > 350 {
                        // T-10 min: rango [350, 600] s
                        let key = DeduplicationKey {
                            schedule_id: sched_id.clone(),
                            window_start_utc: window_start.clone(),
                            threshold: "warning_10".to_string(),
                            channel: "internal".to_string(),
                        };
                        if ticker_notifier.should_emit(&key) {
                            info!("Emitiendo alerta T-10 min para '{}' ({}s restantes)", sched_id, seconds_left);
                            let _ = ticker_event_tx.send(format!(
                                "{{\"type\":\"warning_10\",\"schedule_id\":\"{}\",\"seconds_left\":{}}}",
                                sched_id, seconds_left
                            ));
                            if telegram_enabled {
                                let tg_key = DeduplicationKey {
                                    schedule_id: sched_id.clone(),
                                    window_start_utc: window_start.clone(),
                                    threshold: "warning_10".to_string(),
                                    channel: "telegram".to_string(),
                                };
                                ticker_notifier
                                    .notify_telegram(
                                        &tg_key,
                                        &format!("⚠️ <b>PeakGuard:</b> Faltan ~10 minutos para el horario Peak ({})", sched_id),
                                    )
                                    .await;
                            }
                        }
                    } else if seconds_left <= 300 && seconds_left > 10 {
                        // T-5 min: rango [10, 300] s
                        let key = DeduplicationKey {
                            schedule_id: sched_id.clone(),
                            window_start_utc: window_start.clone(),
                            threshold: "warning_5".to_string(),
                            channel: "internal".to_string(),
                        };
                        if ticker_notifier.should_emit(&key) {
                            info!("Emitiendo alerta T-5 min para '{}' ({}s restantes)", sched_id, seconds_left);
                            let _ = ticker_event_tx.send(format!(
                                "{{\"type\":\"warning_5\",\"schedule_id\":\"{}\",\"seconds_left\":{}}}",
                                sched_id, seconds_left
                            ));
                            if telegram_enabled {
                                let tg_key = DeduplicationKey {
                                    schedule_id: sched_id.clone(),
                                    window_start_utc: window_start.clone(),
                                    threshold: "warning_5".to_string(),
                                    channel: "telegram".to_string(),
                                };
                                ticker_notifier
                                    .notify_telegram(
                                        &tg_key,
                                        &format!("⚠️ <b>PeakGuard:</b> Faltan ~5 minutos para el horario Peak ({})", sched_id),
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
        }
    });

    info!("PeakGuard está en ejecución activa. Presione Ctrl+C o envíe shutdown para detener.");

    // 8. Servir en paralelo ambos puertos con apagado ordenado mediante Ctrl+C o shutdown HTTP
    tokio::select! {
        res = axum::serve(gw_listener, gateway_router) => {
            if let Err(e) = res {
                error!("Error en servidor Gateway: {}", e);
            }
        }
        res = axum::serve(ctrl_listener, control_router) => {
            if let Err(e) = res {
                error!("Error en servidor Control API: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Señal de terminación recibida (Ctrl+C). Iniciando apagado ordenado...");
        }
        _ = shutdown_rx.recv() => {
            info!("Orden de apagado recibida vía Control API. Iniciando apagado ordenado...");
        }
    }

    monitor_handle.abort();
    info!("Apagado ordenado de PeakGuard completado. Sesiones y estado preservados.");
    Ok(())
}
