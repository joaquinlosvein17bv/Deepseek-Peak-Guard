use chrono::{DateTime, Duration, Utc};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Default, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Reloj inyectable para pruebas aceleradas y deterministas
#[derive(Clone)]
pub struct TestClock {
    now_millis: Arc<AtomicI64>,
}

impl TestClock {
    pub fn new(initial: DateTime<Utc>) -> Self {
        Self {
            now_millis: Arc::new(AtomicI64::new(initial.timestamp_millis())),
        }
    }

    pub fn set_time(&self, new_time: DateTime<Utc>) {
        self.now_millis
            .store(new_time.timestamp_millis(), Ordering::SeqCst);
    }

    pub fn advance(&self, duration: Duration) {
        self.now_millis
            .fetch_add(duration.num_milliseconds(), Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        let millis = self.now_millis.load(Ordering::SeqCst);
        DateTime::from_timestamp_millis(millis).unwrap_or_else(Utc::now)
    }
}

/// Detector de saltos bruscos en el reloj (suspensión/despertar de Windows o cambio de hora manual)
pub struct ClockJumpDetector {
    last_tick: DateTime<Utc>,
    jump_threshold: Duration,
}

impl ClockJumpDetector {
    pub fn new(jump_threshold_seconds: i64) -> Self {
        Self {
            last_tick: Utc::now(),
            jump_threshold: Duration::seconds(jump_threshold_seconds),
        }
    }

    pub fn check_jump(&mut self, current: DateTime<Utc>) -> bool {
        let elapsed = current.signed_duration_since(self.last_tick);
        self.last_tick = current;
        elapsed.abs() > self.jump_threshold
    }
}
