use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageStatus {
    Protected,
    Partial,
    Unverified,
    Uncovered,
}

impl CoverageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Protected => "PROTECTED",
            Self::Partial => "PARTIAL",
            Self::Unverified => "UNVERIFIED",
            Self::Uncovered => "UNCOVERED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub provider_id: String,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub upstream_url: String,
    pub schedule_id: String,
    pub enabled: bool,
}

pub struct ModelRegistry {
    providers: HashMap<String, ProviderInfo>,
    models: HashMap<String, ModelInfo>,
    alias_to_model: HashMap<String, String>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            models: HashMap::new(),
            alias_to_model: HashMap::new(),
        };

        // Proveedor por defecto 1: DeepSeek Direct API
        registry.register_provider(ProviderInfo {
            id: "deepseek_direct".to_string(),
            upstream_url: "https://api.deepseek.com".to_string(),
            schedule_id: "deepseek_direct_verified".to_string(),
            enabled: true,
        });

        registry.register_model(ModelInfo {
            id: "deepseek-chat".to_string(),
            name: "DeepSeek-V3 Chat".to_string(),
            aliases: vec![
                "deepseek/deepseek-chat".to_string(),
                "deepseek-v3".to_string(),
            ],
            provider_id: "deepseek_direct".to_string(),
        });

        registry.register_model(ModelInfo {
            id: "deepseek-reasoner".to_string(),
            name: "DeepSeek-R1 Reasoner".to_string(),
            aliases: vec![
                "deepseek/deepseek-reasoner".to_string(),
                "deepseek-r1".to_string(),
            ],
            provider_id: "deepseek_direct".to_string(),
        });

        // Proveedor por defecto 2: OpenCode Go (Perfil independiente)
        registry.register_provider(ProviderInfo {
            id: "opencode_go".to_string(),
            upstream_url: "https://opencode.ai/zen/go/v1".to_string(),
            schedule_id: "opencode_go_verified".to_string(),
            enabled: false,
        });

        registry.register_model(ModelInfo {
            id: "deepseek-v4.1-flash".to_string(),
            name: "DeepSeek V4.1 Flash (Go)".to_string(),
            aliases: vec![
                "opencode/deepseek-v4-flash-free".to_string(),
                "opencode-go/deepseek-v4-flash".to_string(),
            ],
            provider_id: "opencode_go".to_string(),
        });

        registry
    }
}

impl ModelRegistry {
    pub fn register_provider(&mut self, provider: ProviderInfo) {
        self.providers.insert(provider.id.clone(), provider);
    }

    pub fn register_model(&mut self, model: ModelInfo) {
        let model_id = model.id.clone();
        for alias in &model.aliases {
            self.alias_to_model
                .insert(alias.to_lowercase(), model_id.clone());
        }
        self.alias_to_model
            .insert(model_id.to_lowercase(), model_id.clone());
        self.models.insert(model_id, model);
    }

    /// Resuelve el modelo canónico a partir del nombre o alias recibido en la petición
    pub fn resolve_model(&self, model_name: &str) -> Option<&ModelInfo> {
        let normalized = model_name.trim().to_lowercase();
        let canonical_id = self.alias_to_model.get(&normalized)?;
        self.models.get(canonical_id)
    }

    /// Obtiene información del proveedor asociado a un modelo
    pub fn get_provider_for_model(&self, model_name: &str) -> Option<&ProviderInfo> {
        let model = self.resolve_model(model_name)?;
        self.providers.get(&model.provider_id)
    }

    /// Clasifica el estado de cobertura del modelo solicitado
    pub fn classify_coverage(&self, model_name: &str, passes_through_gateway: bool) -> CoverageStatus {
        if !passes_through_gateway {
            return CoverageStatus::Uncovered;
        }

        match self.get_provider_for_model(model_name) {
            Some(provider) if provider.enabled => CoverageStatus::Protected,
            Some(provider) if provider.id == "opencode_go" => CoverageStatus::Unverified,
            Some(_) => CoverageStatus::Partial,
            None => CoverageStatus::Uncovered,
        }
    }

    /// Retorna referencia al mapa interno de modelos
    pub fn get_models(&self) -> &HashMap<String, ModelInfo> {
        &self.models
    }

    /// Retorna referencia al mapa interno de proveedores
    pub fn get_providers(&self) -> &HashMap<String, ProviderInfo> {
        &self.providers
    }

    /// Retorna lista ordenada de todos los modelos registrados
    pub fn list_all_models(&self) -> Vec<&ModelInfo> {
        let mut list: Vec<&ModelInfo> = self.models.values().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub session_id: String,
    pub client_type: String, // "opencode", "pi", "hermes"
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(alias = "provider")]
    pub provider_id: String,
    pub model: String,
    #[serde(default)]
    pub schedule_id: Option<String>,
    #[serde(default = "default_active")]
    pub is_active: bool,
    #[serde(default = "Utc::now")]
    pub last_heartbeat: DateTime<Utc>,
}

fn default_active() -> bool {
    true
}

#[derive(Default)]
pub struct SessionRegistry {
    sessions: RwLock<HashMap<String, ActiveSession>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, session: ActiveSession) {
        let mut map = self.sessions.write().unwrap();
        map.insert(session.session_id.clone(), session);
    }

    pub fn heartbeat(&self, session_id: &str, turn_id: Option<String>, is_active: Option<bool>) -> bool {
        let mut map = self.sessions.write().unwrap();
        if let Some(sess) = map.get_mut(session_id) {
            sess.last_heartbeat = Utc::now();
            if let Some(t) = turn_id {
                sess.turn_id = Some(t);
            }
            if let Some(active) = is_active {
                sess.is_active = active;
            }
            true
        } else {
            false
        }
    }

    pub fn unregister(&self, session_id: &str) {
        let mut map = self.sessions.write().unwrap();
        map.remove(session_id);
    }

    pub fn get_active_sessions_for_schedule(
        &self,
        schedule_id: &str,
        registry: &ModelRegistry,
    ) -> Vec<ActiveSession> {
        let map = self.sessions.read().unwrap();
        map.values()
            .filter(|s| s.is_active)
            .filter(|s| {
                if let Some(provider) = registry.get_provider_for_model(&s.model) {
                    provider.schedule_id == schedule_id
                } else {
                    false
                }
            })
            .cloned()
            .collect()
    }

    pub fn count_active(&self) -> usize {
        let map = self.sessions.read().unwrap();
        map.values().filter(|s| s.is_active).count()
    }

    pub fn prune_stale(&self, timeout: chrono::Duration) {
        let cutoff = Utc::now() - timeout;
        let mut map = self.sessions.write().unwrap();
        map.retain(|_, s| s.last_heartbeat >= cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_registry_lifecycle() {
        let registry = SessionRegistry::new();
        assert_eq!(registry.count_active(), 0);

        // Registrar sesión activa
        registry.register(ActiveSession {
            session_id: "s1".to_string(),
            client_type: "opencode".to_string(),
            turn_id: Some("t1".to_string()),
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-chat".to_string(),
            schedule_id: Some("deepseek_direct_verified".to_string()),
            is_active: true,
            last_heartbeat: Utc::now(),
        });
        assert_eq!(registry.count_active(), 1);

        // Heartbeat
        let updated = registry.heartbeat("s1", Some("t2".to_string()), Some(true));
        assert!(updated);

        // Heartbeat para sesión inexistente
        assert!(!registry.heartbeat("nonexistent", None, None));

        // Desregistrar sesión
        registry.unregister("s1");
        assert_eq!(registry.count_active(), 0);
    }

    #[test]
    fn test_session_registry_filter_by_schedule() {
        let session_reg = SessionRegistry::new();
        let model_reg = ModelRegistry::default();

        session_reg.register(ActiveSession {
            session_id: "s-deepseek".to_string(),
            client_type: "opencode".to_string(),
            turn_id: Some("t1".to_string()),
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-chat".to_string(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: Utc::now(),
        });

        session_reg.register(ActiveSession {
            session_id: "s-go".to_string(),
            client_type: "opencode".to_string(),
            turn_id: Some("t2".to_string()),
            provider_id: "opencode_go".to_string(),
            model: "deepseek-v4.1-flash".to_string(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: Utc::now(),
        });

        // Filtrar sesiones para deepseek_direct_verified
        let ds_sessions = session_reg.get_active_sessions_for_schedule("deepseek_direct_verified", &model_reg);
        assert_eq!(ds_sessions.len(), 1);
        assert_eq!(ds_sessions[0].session_id, "s-deepseek");

        // Filtrar sesiones para opencode_go_verified
        let go_sessions = session_reg.get_active_sessions_for_schedule("opencode_go_verified", &model_reg);
        assert_eq!(go_sessions.len(), 1);
        assert_eq!(go_sessions[0].session_id, "s-go");

        // Filtrar sesiones para horario inexistente
        let none_sessions = session_reg.get_active_sessions_for_schedule("nonexistent_sched", &model_reg);
        assert_eq!(none_sessions.len(), 0);
    }

    #[test]
    fn test_session_registry_prune_stale() {
        let registry = SessionRegistry::new();
        let old_time = Utc::now() - chrono::Duration::seconds(100);

        registry.register(ActiveSession {
            session_id: "stale-session".to_string(),
            client_type: "pi".to_string(),
            turn_id: None,
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-chat".to_string(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: old_time,
        });

        // Al podar sesiones más viejas de 30s
        registry.prune_stale(chrono::Duration::seconds(30));
        assert_eq!(registry.count_active(), 0);
    }
}

