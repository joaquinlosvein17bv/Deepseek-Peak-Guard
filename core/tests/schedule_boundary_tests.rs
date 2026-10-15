use chrono::{DateTime, Utc};
use peakguard_core::clock::{Clock, ClockJumpDetector, TestClock};
use peakguard_core::registry::{CoverageStatus, ModelRegistry};
use peakguard_core::schedule::{AlertThreshold, Schedule};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct ScheduleFixture {
    id: String,
    intervals: Vec<IntervalFixture>,
    weekdays: Vec<String>,
    exclude_chinese_public_holidays: bool,
    #[serde(default)]
    holidays_2026: Vec<String>,
}

#[derive(Deserialize)]
struct IntervalFixture {
    start: String,
    end: String,
}

#[derive(Deserialize)]
struct BoundaryCase {
    description: String,
    timestamp_utc: String,
    expected_is_peak: bool,
    schedule_id: Option<String>,
    #[serde(default)]
    expected_alert_threshold: Option<String>,
    #[serde(default)]
    seconds_to_peak: Option<u64>,
}

fn load_schedule<P: AsRef<Path>>(path: P) -> Schedule {
    let content = fs::read_to_string(path).expect("No se pudo leer fixture de schedule");
    let fix: ScheduleFixture = serde_json::from_str(&content).expect("JSON fixture inválido");
    let intervals: Vec<String> = fix
        .intervals
        .into_iter()
        .map(|i| format!("{}-{}", i.start, i.end))
        .collect();

    Schedule::new(
        &fix.id,
        &intervals,
        &fix.weekdays,
        fix.exclude_chinese_public_holidays,
        &fix.holidays_2026,
    )
}

#[test]
fn test_all_boundary_fixtures() {
    let deepseek_sched = load_schedule("../test-fixtures/schedules/deepseek_direct.json");
    let opencode_sched = load_schedule("../test-fixtures/schedules/opencode_go.json");

    let cases_content = fs::read_to_string("../test-fixtures/schedules/boundary_test_cases.json")
        .expect("No se pudo leer boundary_test_cases.json");
    let cases: Vec<BoundaryCase> =
        serde_json::from_str(&cases_content).expect("Error deserializando boundary cases");

    for case in &cases {
        let dt = DateTime::parse_from_rfc3339(&case.timestamp_utc)
            .expect("Timestamp inválido")
            .with_timezone(&Utc);

        let sched = match case.schedule_id.as_deref() {
            Some("opencode_go_verified") => &opencode_sched,
            _ => &deepseek_sched,
        };

        let is_peak = sched.is_peak(dt);
        assert_eq!(
            is_peak, case.expected_is_peak,
            "Fallo en caso: '{}' a las {}",
            case.description, case.timestamp_utc
        );

        if let Some(expected_seconds) = case.seconds_to_peak {
            let next = sched.next_peak(dt);
            assert!(
                next.is_some(),
                "Se esperaba próximo peak para el caso: '{}'",
                case.description
            );
            let (_, remaining) = next.unwrap();
            assert_eq!(
                remaining, expected_seconds,
                "Segundos restantes incorrectos en caso: '{}'",
                case.description
            );
        }

        if let Some(ref expected_th) = case.expected_alert_threshold {
            let alert = sched.evaluate_alert(dt);
            assert!(
                alert.is_some(),
                "Se esperaba alerta para caso: '{}'",
                case.description
            );
            match (expected_th.as_str(), alert.unwrap()) {
                ("warning_30", AlertThreshold::Warning30) => (),
                ("warning_10", AlertThreshold::Warning10) => (),
                ("warning_5", AlertThreshold::Warning5) => (),
                ("countdown_start", AlertThreshold::Countdown(10)) => (),
                (exp, actual) => panic!(
                    "Discrepancia de alerta en '{}': esperado '{}', obtenido '{:?}'",
                    case.description, exp, actual
                ),
            }
        }
    }
}

#[test]
fn test_model_registry_resolution_and_coverage() {
    let registry = ModelRegistry::default();

    // 1. Resolución de modelos DeepSeek directos
    let m1 = registry.resolve_model("deepseek-chat");
    assert!(m1.is_some());
    assert_eq!(m1.unwrap().id, "deepseek-chat");

    let m2 = registry.resolve_model("deepseek/deepseek-reasoner");
    assert!(m2.is_some());
    assert_eq!(m2.unwrap().id, "deepseek-reasoner");

    // 2. Cobertura: a través de gateway vs directo
    assert_eq!(
        registry.classify_coverage("deepseek-chat", true),
        CoverageStatus::Protected
    );
    assert_eq!(
        registry.classify_coverage("deepseek-chat", false),
        CoverageStatus::Uncovered
    );

    // 3. OpenCode Go está deshabilitado por defecto para routing directo
    assert_eq!(
        registry.classify_coverage("opencode/deepseek-v4-flash-free", true),
        CoverageStatus::Partial
    );

    // 4. Modelo desconocido
    assert_eq!(
        registry.classify_coverage("gpt-4o", true),
        CoverageStatus::Uncovered
    );
}

#[test]
fn test_clock_jump_detector() {
    let mut detector = ClockJumpDetector::new(60);
    let now = Utc::now();

    // Pequeño avance normal de 5 segundos
    assert!(!detector.check_jump(now + chrono::Duration::seconds(5)));

    // Salto brusco de 1 hora (ej. PC despertó de suspensión)
    assert!(detector.check_jump(now + chrono::Duration::seconds(3600)));
}

#[test]
fn test_injectable_test_clock() {
    let initial = DateTime::parse_from_rfc3339("2026-10-19T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = TestClock::new(initial);

    assert_eq!(clock.now(), initial);

    clock.advance(chrono::Duration::minutes(30));
    assert_eq!(
        clock.now(),
        DateTime::parse_from_rfc3339("2026-10-19T00:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    );
}
