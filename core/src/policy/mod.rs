use crate::registry::{CoverageStatus, ModelRegistry};
use crate::schedule::Schedule;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingMode {
    StrictBlock,
    RestrictedStop,
    NotifyOnly,
}

impl OperatingMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "strict_block" => Some(Self::StrictBlock),
            "restricted_stop" => Some(Self::RestrictedStop),
            "notify_only" => Some(Self::NotifyOnly),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StrictBlock => "strict_block",
            Self::RestrictedStop => "restricted_stop",
            Self::NotifyOnly => "notify_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Block {
        reason: String,
        next_off_peak_utc: Option<DateTime<Utc>>,
    },
    StopRequired {
        session_id: String,
        turn_id: Option<String>,
        reason: String,
    },
    NotifyOnly {
        message: String,
    },
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StopKey {
    pub session_id: String,
    pub turn_id: String,
    pub transition_utc: String,
}

pub struct PolicyEngine {
    mode: RwLock<OperatingMode>,
    registry: Arc<ModelRegistry>,
    schedules: HashMap<String, Schedule>,
    stop_emitted: RwLock<HashSet<StopKey>>,
    stop_acked: RwLock<HashSet<StopKey>>,
}

impl PolicyEngine {
    pub fn new(
        initial_mode: OperatingMode,
        registry: Arc<ModelRegistry>,
        schedules: HashMap<String, Schedule>,
    ) -> Self {
        Self {
            mode: RwLock::new(initial_mode),
            registry,
            schedules,
            stop_emitted: RwLock::new(HashSet::new()),
            stop_acked: RwLock::new(HashSet::new()),
        }
    }

    pub fn get_mode(&self) -> OperatingMode {
        *self.mode.read().unwrap()
    }

    pub fn set_mode(&self, new_mode: OperatingMode) {
        let mut w = self.mode.write().unwrap();
        *w = new_mode;
    }

    /// Evalúa la política de acceso para una solicitud de inferencia
    pub fn evaluate_request(
        &self,
        model_name: &str,
        session_id: Option<&str>,
        turn_id: Option<&str>,
        time: DateTime<Utc>,
        passes_through_gateway: bool,
    ) -> PolicyDecision {
        // Principio P2: No apagar otros modelos. Si no es un modelo protegido o registrado, ALLOW.
        let coverage = self.registry.classify_coverage(model_name, passes_through_gateway);
        if coverage != CoverageStatus::Protected {
            return PolicyDecision::Allow;
        }

        // Obtener proveedor y su horario
        let provider = match self.registry.get_provider_for_model(model_name) {
            Some(p) => p,
            None => return PolicyDecision::Allow,
        };

        let schedule = match self.schedules.get(&provider.schedule_id) {
            Some(s) => s,
            None => return PolicyDecision::Allow,
        };

        let in_peak = schedule.is_peak(time);
        if !in_peak {
            return PolicyDecision::Allow;
        }

        // En horario Peak: actuar según el modo configurado
        let mode = self.get_mode();
        match mode {
            OperatingMode::NotifyOnly => PolicyDecision::NotifyOnly {
                message: format!("Aviso: {} está en horario peak pero el modo activo es notify_only (sin bloqueo).", model_name),
            },
            OperatingMode::StrictBlock => {
                let next_off = schedule.next_off_peak(time).map(|(dt, _)| dt);
                PolicyDecision::Block {
                    reason: "Petición bloqueada por horario peak (modo: strict_block).".to_string(),
                    next_off_peak_utc: next_off,
                }
            }
            OperatingMode::RestrictedStop => {
                // Si la sesión está identificada y no se ha emitido Stop para esta transición:
                if let Some(sess) = session_id {
                    let transition_str = time.format("%Y-%m-%dT%H:%M").to_string();
                    let key = StopKey {
                        session_id: sess.to_string(),
                        turn_id: turn_id.unwrap_or("default").to_string(),
                        transition_utc: transition_str,
                    };

                    let mut emitted = self.stop_emitted.write().unwrap();
                    if !emitted.contains(&key) {
                        emitted.insert(key);
                        return PolicyDecision::StopRequired {
                            session_id: sess.to_string(),
                            turn_id: turn_id.map(|s| s.to_string()),
                            reason: "Horario peak iniciado. Cancelando turno activo mediante Stop nativo sin cerrar aplicación.".to_string(),
                        };
                    }
                }

                // Si ya se emitió Stop o es una solicitud nueva durante el peak: bloquear en gateway
                let next_off = schedule.next_off_peak(time).map(|(dt, _)| dt);
                PolicyDecision::Block {
                    reason: "Petición bloqueada durante horario peak (modo: restricted_stop).".to_string(),
                    next_off_peak_utc: next_off,
                }
            }
        }
    }

    /// Registra el acuse de recibo (ACK) de Stop enviado por el adaptador
    pub fn acknowledge_stop(&self, key: StopKey) {
        let mut acked = self.stop_acked.write().unwrap();
        acked.insert(key);
    }

    /// Verifica si un Stop ha sido confirmado por el cliente
    pub fn is_stop_acknowledged(&self, key: &StopKey) -> bool {
        self.stop_acked.read().unwrap().contains(key)
    }
}
