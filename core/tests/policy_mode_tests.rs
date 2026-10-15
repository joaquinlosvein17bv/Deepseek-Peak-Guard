use chrono::{DateTime, Utc};
use peakguard_core::policy::{OperatingMode, PolicyDecision, PolicyEngine, StopKey};
use peakguard_core::registry::ModelRegistry;
use peakguard_core::schedule::Schedule;
use std::collections::HashMap;
use std::sync::Arc;

fn setup_test_engine(mode: OperatingMode) -> PolicyEngine {
    let registry = Arc::new(ModelRegistry::default());
    let mut schedules = HashMap::new();

    let deepseek_sched = Schedule::new(
        "deepseek_direct_verified",
        &["01:00-04:00".to_string(), "06:00-10:00".to_string()],
        &["mon".to_string(), "tue".to_string(), "wed".to_string(), "thu".to_string(), "fri".to_string()],
        true,
        &["2026-10-01".to_string()],
    );

    schedules.insert("deepseek_direct_verified".to_string(), deepseek_sched);

    PolicyEngine::new(mode, registry, schedules)
}

#[test]
fn test_off_peak_allows_all() {
    let engine = setup_test_engine(OperatingMode::StrictBlock);
    // 00:30 UTC es off-peak
    let t = DateTime::parse_from_rfc3339("2026-10-19T00:30:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let decision = engine.evaluate_request("deepseek-chat", Some("s1"), None, t, true);
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn test_third_party_models_never_blocked() {
    let engine = setup_test_engine(OperatingMode::StrictBlock);
    // 02:00 UTC es horario peak para DeepSeek
    let t = DateTime::parse_from_rfc3339("2026-10-19T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    // GPT-4o o Claude no deben ser afectados por la regla de DeepSeek (Principio P2)
    let d1 = engine.evaluate_request("gpt-4o", Some("s1"), None, t, true);
    assert_eq!(d1, PolicyDecision::Allow);

    let d2 = engine.evaluate_request("claude-3-7-sonnet", Some("s2"), None, t, true);
    assert_eq!(d2, PolicyDecision::Allow);
}

#[test]
fn test_strict_block_mode() {
    let engine = setup_test_engine(OperatingMode::StrictBlock);
    // 02:00 UTC es horario peak
    let t = DateTime::parse_from_rfc3339("2026-10-19T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let decision = engine.evaluate_request("deepseek-chat", Some("s1"), None, t, true);
    match decision {
        PolicyDecision::Block { next_off_peak_utc, .. } => {
            assert!(next_off_peak_utc.is_some());
            assert_eq!(
                next_off_peak_utc.unwrap(),
                DateTime::parse_from_rfc3339("2026-10-19T04:00:00Z").unwrap().with_timezone(&Utc)
            );
        }
        other => panic!("Esperado Block, obtenido: {:?}", other),
    }
}

#[test]
fn test_restricted_stop_mode_idempotency() {
    let engine = setup_test_engine(OperatingMode::RestrictedStop);
    let t = DateTime::parse_from_rfc3339("2026-10-19T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    // Primera llamada para sesión activa: emite StopRequired
    let d1 = engine.evaluate_request("deepseek-chat", Some("session-abc"), Some("turn-1"), t, true);
    match d1 {
        PolicyDecision::StopRequired { session_id, turn_id, .. } => {
            assert_eq!(session_id, "session-abc");
            assert_eq!(turn_id, Some("turn-1".to_string()));
        }
        other => panic!("Esperado StopRequired en primera llamada, obtenido: {:?}", other),
    }

    // Segunda llamada para la misma sesión/turno: idempotente, ya no reenvía Stop, bloquea en gateway
    let d2 = engine.evaluate_request("deepseek-chat", Some("session-abc"), Some("turn-1"), t, true);
    match d2 {
        PolicyDecision::Block { .. } => (),
        other => panic!("Esperado Block en segunda llamada idempotente, obtenido: {:?}", other),
    }

    // Acuse de recibo (ACK)
    let key = StopKey {
        session_id: "session-abc".to_string(),
        turn_id: "turn-1".to_string(),
        transition_utc: t.format("%Y-%m-%dT%H:%M").to_string(),
    };
    assert!(!engine.is_stop_acknowledged(&key));
    engine.acknowledge_stop(key.clone());
    assert!(engine.is_stop_acknowledged(&key));
}

#[test]
fn test_notify_only_mode() {
    let engine = setup_test_engine(OperatingMode::NotifyOnly);
    let t = DateTime::parse_from_rfc3339("2026-10-19T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let decision = engine.evaluate_request("deepseek-chat", Some("s1"), None, t, true);
    match decision {
        PolicyDecision::NotifyOnly { .. } => (),
        other => panic!("Esperado NotifyOnly, obtenido: {:?}", other),
    }
}

#[test]
fn test_runtime_mode_switch() {
    let engine = setup_test_engine(OperatingMode::NotifyOnly);
    let t = DateTime::parse_from_rfc3339("2026-10-19T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    // Inicialmente NotifyOnly
    assert_eq!(engine.get_mode(), OperatingMode::NotifyOnly);
    let d1 = engine.evaluate_request("deepseek-chat", None, None, t, true);
    assert!(matches!(d1, PolicyDecision::NotifyOnly { .. }));

    // Cambiar a StrictBlock
    engine.set_mode(OperatingMode::StrictBlock);
    assert_eq!(engine.get_mode(), OperatingMode::StrictBlock);
    let d2 = engine.evaluate_request("deepseek-chat", None, None, t, true);
    assert!(matches!(d2, PolicyDecision::Block { .. }));
}
