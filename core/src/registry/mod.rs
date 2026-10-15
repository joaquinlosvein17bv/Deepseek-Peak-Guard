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

#[derive(Debug, Default)]
pub struct TrafficTracker {
    total_observed_requests: std::sync::atomic::AtomicUsize,
    observed_by_provider: RwLock<HashMap<String, usize>>,
    observed_by_model: RwLock<HashMap<String, usize>>,
}

impl TrafficTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(&self, provider_id: &str, model_id: &str) {
        self.total_observed_requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut p_map = self.observed_by_provider.write().unwrap();
        *p_map.entry(provider_id.to_string()).or_insert(0) += 1;

        let mut m_map = self.observed_by_model.write().unwrap();
        *m_map.entry(model_id.to_string()).or_insert(0) += 1;
    }

    pub fn get_total_observed(&self) -> usize {
        self.total_observed_requests.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn has_observed_provider(&self, provider_id: &str) -> bool {
        let p_map = self.observed_by_provider.read().unwrap();
        p_map.get(provider_id).map(|&c| c > 0).unwrap_or(false)
    }

    pub fn get_observed_providers(&self) -> Vec<String> {
        let p_map = self.observed_by_provider.read().unwrap();
        p_map.keys().cloned().collect()
    }
}

pub struct ModelRegistry {
    providers: HashMap<String, ProviderInfo>,
    models: HashMap<String, ModelInfo>,
    alias_to_model: HashMap<String, String>,
    traffic_tracker: std::sync::Arc<TrafficTracker>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            models: HashMap::new(),
            alias_to_model: HashMap::new(),
            traffic_tracker: std::sync::Arc::new(TrafficTracker::new()),
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

    /// Registra tráfico observado para un proveedor y modelo
    pub fn record_observed_traffic(&self, provider_id: &str, model_id: &str) {
        self.traffic_tracker.record_request(provider_id, model_id);
    }

    /// Retorna referencia al tracker de tráfico observado
    pub fn get_traffic_tracker(&self) -> std::sync::Arc<TrafficTracker> {
        self.traffic_tracker.clone()
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub session_id: String,
    #[serde(default = "default_client_type")]
    pub client_type: String, // "opencode", "pi", "hermes"
    #[serde(default = "default_client_instance_id")]
    pub client_instance_id: String, // ID estable y único por proceso/instancia
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
    #[serde(default = "Utc::now")]
    pub registered_at: DateTime<Utc>,
}

fn default_client_type() -> String {
    "unknown".to_string()
}

fn default_client_instance_id() -> String {
    "default_instance".to_string()
}

fn default_active() -> bool {
    true
}

impl ActiveSession {
    /// Identidad compuesta de sesión (T18.1.2)
    pub fn composite_key(&self) -> String {
        format!("{}:{}:{}", self.client_type, self.client_instance_id, self.session_id)
    }

    pub fn new(
        session_id: impl Into<String>,
        client_type: impl Into<String>,
        client_instance_id: impl Into<String>,
        provider_id: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            client_type: client_type.into(),
            client_instance_id: client_instance_id.into(),
            turn_id: None,
            provider_id: provider_id.into(),
            model: model.into(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: Utc::now(),
            registered_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopCommandStatus {
    Pending,
    Acknowledged,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopCommand {
    pub command_id: String,
    pub client_type: String,
    pub client_instance_id: String,
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    pub provider_id: String,
    pub model: String,
    pub transition_utc: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub status: StopCommandStatus,
    #[serde(default)]
    pub acked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ack_success: Option<bool>,
    #[serde(default)]
    pub ack_error: Option<String>,
}

#[derive(Default)]
pub struct StopCommandManager {
    commands: RwLock<HashMap<String, StopCommand>>,
    persistence_path: Option<std::path::PathBuf>,
}

impl StopCommandManager {
    pub fn new() -> Self {
        Self {
            commands: RwLock::new(HashMap::new()),
            persistence_path: None,
        }
    }

    pub fn with_persistence(path: std::path::PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut commands = HashMap::new();
        if path.exists() {
            let data = match std::fs::read_to_string(&path) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Error leyendo archivo de comandos persistidos ('{}'): {}. Fail-closed.", path.display(), e);
                    return Err(format!("Error leyendo archivo de comandos persistidos ('{}'): {}", path.display(), e).into());
                }
            };
            let loaded: Vec<StopCommand> = match serde_json::from_str(&data) {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Archivo de comandos persistidos corrupto ('{}'): {}. Fail-closed sin sobrescribir.", path.display(), e);
                    return Err(format!("Archivo de comandos persistidos corrupto ('{}'): {}", path.display(), e).into());
                }
            };

            let now = Utc::now();
            for mut cmd in loaded {
                let age = now.signed_duration_since(cmd.created_at);
                if age.num_minutes() >= 10 && cmd.status == StopCommandStatus::Pending {
                    cmd.status = StopCommandStatus::Expired;
                    cmd.ack_error = Some("Expirado tras reinicio del daemon (>10m)".to_string());
                }
                commands.insert(cmd.command_id.clone(), cmd);
            }
        }

        let mgr = Self {
            commands: RwLock::new(commands),
            persistence_path: Some(path.clone()),
        };

        if path.exists() {
            mgr.persist_to_disk()?;
        }
        Ok(mgr)
    }

    pub fn persist_to_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref path) = self.persistence_path {
            let map = self.commands.read().map_err(|e| format!("Lock poisoned: {}", e))?;
            let list: Vec<StopCommand> = map.values().cloned().collect();
            drop(map);

            let json = serde_json::to_string_pretty(&list)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp_path = path.with_extension(format!("tmp.{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)));
            {
                use std::io::Write;
                let mut f = std::fs::File::create(&tmp_path)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(perms) = f.metadata().map(|m| m.permissions()) {
                        let mut p = perms;
                        p.set_mode(0o600);
                        let _ = f.set_permissions(p);
                    }
                }
                f.write_all(json.as_bytes())?;
                f.sync_all()?;
            }

            // Sustitución atómica y segura compatible con Windows
            let mut renamed = false;
            let mut last_err = None;
            for _ in 0..5 {
                match std::fs::rename(&tmp_path, path) {
                    Ok(_) => {
                        renamed = true;
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
            if !renamed {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(Box::new(last_err.unwrap()));
            }
        }
        Ok(())
    }

    pub fn get_command(&self, command_id: &str) -> Option<StopCommand> {
        let map = self.commands.read().unwrap();
        map.get(command_id).cloned()
    }

    pub fn expire_stale_commands(&self, max_age: chrono::Duration) {
        let mut map = self.commands.write().unwrap();
        let now = Utc::now();
        let mut changed = false;
        for cmd in map.values_mut() {
            if cmd.status == StopCommandStatus::Pending && (now - cmd.created_at) >= max_age {
                cmd.status = StopCommandStatus::Expired;
                cmd.ack_error = Some("Expirado por timeout de orden".to_string());
                changed = true;
            }
        }
        drop(map);
        if changed {
            let _ = self.persist_to_disk();
        }
    }

    pub fn create_command(
        &self,
        client_type: &str,
        client_instance_id: &str,
        session_id: &str,
        turn_id: Option<String>,
        provider_id: &str,
        model: &str,
        transition_utc: &str,
        reason: &str,
    ) -> Result<StopCommand, String> {
        let command_id = format!(
            "cmd_{}_{}_{}",
            Utc::now().timestamp_millis(),
            client_instance_id,
            session_id
        );
        let cmd = StopCommand {
            command_id: command_id.clone(),
            client_type: client_type.to_string(),
            client_instance_id: client_instance_id.to_string(),
            session_id: session_id.to_string(),
            turn_id,
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            transition_utc: transition_utc.to_string(),
            reason: reason.to_string(),
            created_at: Utc::now(),
            status: StopCommandStatus::Pending,
            acked_at: None,
            ack_success: None,
            ack_error: None,
        };
        let mut map = self.commands.write().unwrap();
        map.insert(command_id, cmd.clone());
        drop(map);
        if let Err(e) = self.persist_to_disk() {
            return Err(format!("Fallo persistiendo nueva orden Stop: {}", e));
        }
        Ok(cmd)
    }

    pub fn get_pending_commands_for_instance(&self, client_instance_id: &str) -> Vec<StopCommand> {
        let map = self.commands.read().unwrap();
        map.values()
            .filter(|c| c.client_instance_id == client_instance_id && c.status == StopCommandStatus::Pending)
            .cloned()
            .collect()
    }

    pub fn record_ack(
        &self,
        command_id: &str,
        client_instance_id: &str,
        session_id: &str,
        turn_id: Option<&str>,
        success: bool,
        error: Option<String>,
    ) -> Result<StopCommand, String> {
        let mut map = self.commands.write().unwrap();

        // 1. T18.3.2: Exigir command_id obligatorio y correspondencia exacta (sin fallbacks)
        let cmd = match map.get_mut(command_id) {
            Some(c) => c,
            None => {
                return Err(format!("Comando con command_id '{}' no encontrado en el registro", command_id));
            }
        };

        if !cmd.client_instance_id.is_empty()
            && cmd.client_instance_id != "default_instance"
            && cmd.client_instance_id != client_instance_id
        {
            return Err(format!(
                "Instancia no autorizada: command_id '{}' pertenece a '{}', pero fue respondido por '{}'",
                command_id, cmd.client_instance_id, client_instance_id
            ));
        }

        if cmd.session_id != session_id {
            return Err(format!(
                "Sesión discordante: command_id '{}' pertenece a sesión '{}', pero fue respondido para '{}'",
                command_id, cmd.session_id, session_id
            ));
        }

        // T18.3.2: Si la orden original tenía turn_id, el ACK debe coincidir exactamente
        if let Some(ref cmd_turn) = cmd.turn_id {
            match turn_id {
                Some(ack_turn) if ack_turn == cmd_turn => {}
                Some(ack_turn) => {
                    return Err(format!(
                        "Turno discordante: command_id '{}' pertenece a turno '{}', pero fue respondido para turno '{}'",
                        command_id, cmd_turn, ack_turn
                    ));
                }
                None => {
                    return Err(format!(
                        "Turno ausente: command_id '{}' requiere turno '{}', pero el ACK no incluyó turn_id",
                        command_id, cmd_turn
                    ));
                }
            }
        }

        // Idempotencia: si ya está en estado terminal, retornar estado sin duplicar acciones ni mutar
        if cmd.status == StopCommandStatus::Acknowledged
            || cmd.status == StopCommandStatus::Failed
            || cmd.status == StopCommandStatus::Expired
        {
            return Ok(cmd.clone());
        }

        cmd.status = if success {
            StopCommandStatus::Acknowledged
        } else {
            StopCommandStatus::Failed
        };
        cmd.acked_at = Some(Utc::now());
        cmd.ack_success = Some(success);
        cmd.ack_error = error;
        let res = cmd.clone();
        drop(map);

        if let Err(e) = self.persist_to_disk() {
            return Err(format!("Fallo persistiendo acuse en disco: {}", e));
        }

        Ok(res)
    }
}

pub struct SessionRegistry {
    sessions: RwLock<HashMap<String, ActiveSession>>,
    command_manager: std::sync::Arc<StopCommandManager>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            command_manager: std::sync::Arc::new(StopCommandManager::new()),
        }
    }
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_persistence_path(path: std::path::PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cmd_mgr = StopCommandManager::with_persistence(path)?;
        Ok(Self {
            sessions: RwLock::new(HashMap::new()),
            command_manager: std::sync::Arc::new(cmd_mgr),
        })
    }

    pub fn new_with_persistence() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let commands_path = if let Ok(p) = std::env::var("PEAKGUARD_COMMANDS_PATH") {
            std::path::PathBuf::from(p)
        } else {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string());
            std::path::PathBuf::from(home).join(".peakguard").join("pending_commands.json")
        };
        Self::with_persistence_path(commands_path)
    }

    pub fn get_command_manager(&self) -> std::sync::Arc<StopCommandManager> {
        self.command_manager.clone()
    }

    pub fn register(&self, session: ActiveSession) {
        let mut map = self.sessions.write().unwrap();
        let key = session.composite_key();
        map.insert(key, session);
    }

    pub fn get_active_sessions(&self) -> Vec<ActiveSession> {
        let map = self.sessions.read().unwrap();
        map.values().cloned().collect()
    }

    pub fn heartbeat(
        &self,
        client_instance_id: &str,
        session_id: &str,
        turn_id: Option<String>,
        is_active: Option<bool>,
    ) -> bool {
        if client_instance_id.trim().is_empty() {
            return false;
        }
        let mut map = self.sessions.write().unwrap();
        for s in map.values_mut() {
            if s.session_id == session_id && s.client_instance_id == client_instance_id {
                s.last_heartbeat = Utc::now();
                if let Some(t) = turn_id {
                    s.turn_id = Some(t);
                }
                if let Some(active) = is_active {
                    s.is_active = active;
                }
                return true;
            }
        }
        false
    }

    pub fn unregister(&self, client_instance_id: &str, session_id: &str) -> bool {
        if client_instance_id.trim().is_empty() {
            return false;
        }
        let mut map = self.sessions.write().unwrap();
        let prev_len = map.len();
        map.retain(|_, s| !(s.session_id == session_id && s.client_instance_id == client_instance_id));
        map.len() < prev_len
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
                } else if !s.provider_id.is_empty() {
                    if let Some(p) = registry.get_providers().get(&s.provider_id) {
                        p.schedule_id == schedule_id
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .cloned()
            .collect()
    }

    /// Retorna sesiones activas filtradas específicamente por proveedor (T18.1.1)
    pub fn get_active_sessions_for_provider(
        &self,
        provider_id: &str,
        registry: &ModelRegistry,
    ) -> Vec<ActiveSession> {
        let map = self.sessions.read().unwrap();
        map.values()
            .filter(|s| s.is_active)
            .filter(|s| {
                if s.provider_id == provider_id {
                    return true;
                }
                if let Some(p) = registry.get_provider_for_model(&s.model) {
                    return p.id == provider_id;
                }
                false
            })
            .cloned()
            .collect()
    }

    /// Cuenta sesiones activas específicas para un proveedor
    pub fn count_active_for_provider(&self, provider_id: &str, registry: &ModelRegistry) -> usize {
        self.get_active_sessions_for_provider(provider_id, registry).len()
    }

    pub fn count_active(&self) -> usize {
        let map = self.sessions.read().unwrap();
        map.values().filter(|s| s.is_active).count()
    }

    pub fn prune_stale(&self, inactive_timeout: chrono::Duration, active_max_silence: chrono::Duration) {
        let now = Utc::now();
        let mut map = self.sessions.write().unwrap();
        map.retain(|_, s| {
            let elapsed = now - s.last_heartbeat;
            if s.is_active {
                elapsed < active_max_silence
            } else {
                elapsed < inactive_timeout
            }
        });
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
            client_instance_id: "inst_1".to_string(),
            turn_id: Some("t1".to_string()),
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-chat".to_string(),
            schedule_id: Some("deepseek_direct_verified".to_string()),
            is_active: true,
            last_heartbeat: Utc::now(),
            registered_at: Utc::now(),
        });
        assert_eq!(registry.count_active(), 1);

        // Heartbeat con client_instance_id
        let updated = registry.heartbeat("inst_1", "s1", Some("t2".to_string()), Some(true));
        assert!(updated);

        // Heartbeat para sesión inexistente
        assert!(!registry.heartbeat("inst_nonexistent", "nonexistent", None, None));

        // Desregistrar sesión
        let unregistered = registry.unregister("inst_1", "s1");
        assert!(unregistered);
        assert_eq!(registry.count_active(), 0);
    }

    #[test]
    fn test_composite_identity_isolation() {
        // T18.1.2: Dos clientes o instancias con el mismo session_id ("session_common")
        // NO deben sobreescribirse entre sí gracias a la clave compuesta.
        let registry = SessionRegistry::new();

        let s_a = ActiveSession::new("session_common", "opencode", "inst_alpha", "deepseek_direct", "deepseek-chat");
        let s_b = ActiveSession::new("session_common", "opencode", "inst_beta", "deepseek_direct", "deepseek-chat");

        registry.register(s_a);
        registry.register(s_b);

        // Ambas deben estar activas y no haber colisionado
        assert_eq!(registry.count_active(), 2);

        // Heartbeat a inst_alpha solo afecta a inst_alpha
        assert!(registry.heartbeat("inst_alpha", "session_common", Some("turn_a1".to_string()), Some(true)));

        // Unregister de inst_alpha no debe borrar inst_beta
        assert!(registry.unregister("inst_alpha", "session_common"));
        assert_eq!(registry.count_active(), 1);

        // inst_beta sigue activa
        assert!(registry.heartbeat("inst_beta", "session_common", None, None));
    }

    #[test]
    fn test_stop_command_manager_lifecycle() {
        // T18.1.3: Registro y recuperación de comandos Stop
        let mgr = StopCommandManager::new();

        // 1. Crear comando Stop
        let cmd = mgr.create_command(
            "pi",
            "inst_pi_99",
            "sess_42",
            Some("turn_9".to_string()),
            "deepseek_direct",
            "deepseek-reasoner",
            &Utc::now().to_rfc3339(),
            "Entrada en pico",
        ).unwrap();

        assert_eq!(cmd.status, StopCommandStatus::Pending);
        assert_eq!(cmd.client_instance_id, "inst_pi_99");

        // 2. Recuperar pendientes para inst_pi_99
        let pending = mgr.get_pending_commands_for_instance("inst_pi_99");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].command_id, cmd.command_id);

        // Para otra instancia no debe haber pendientes
        let pending_other = mgr.get_pending_commands_for_instance("inst_other");
        assert_eq!(pending_other.len(), 0);

        // 3. Intento de ACK por instancia ajena debe fallar (403 / Err)
        let foreign_ack = mgr.record_ack(
            &cmd.command_id,
            "inst_imposter",
            "sess_42",
            Some("turn_9"),
            true,
            None,
        );
        assert!(foreign_ack.is_err(), "ACK de instancia ajena debe ser rechazado");

        // 4. ACK válido por la instancia legítima
        let valid_ack = mgr.record_ack(
            &cmd.command_id,
            "inst_pi_99",
            "sess_42",
            Some("turn_9"),
            true,
            None,
        );
        assert!(valid_ack.is_ok());
        let acked_cmd = valid_ack.unwrap();
        assert_eq!(acked_cmd.status, StopCommandStatus::Acknowledged);

        // Ya no debe aparecer como pendiente
        let pending_after = mgr.get_pending_commands_for_instance("inst_pi_99");
        assert_eq!(pending_after.len(), 0);

        // 5. ACK repetido es idempotente y no falla
        let dup_ack = mgr.record_ack(
            &cmd.command_id,
            "inst_pi_99",
            "sess_42",
            Some("turn_9"),
            true,
            None,
        );
        assert!(dup_ack.is_ok());
    }

    #[test]
    fn test_session_registry_filter_by_schedule() {
        let session_reg = SessionRegistry::new();
        let model_reg = ModelRegistry::default();

        session_reg.register(ActiveSession {
            session_id: "s-deepseek".to_string(),
            client_type: "opencode".to_string(),
            client_instance_id: "inst_1".to_string(),
            turn_id: Some("t1".to_string()),
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-chat".to_string(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: Utc::now(),
            registered_at: Utc::now(),
        });

        session_reg.register(ActiveSession {
            session_id: "s-go".to_string(),
            client_type: "opencode".to_string(),
            client_instance_id: "inst_1".to_string(),
            turn_id: Some("t2".to_string()),
            provider_id: "opencode_go".to_string(),
            model: "deepseek-v4.1-flash".to_string(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: Utc::now(),
            registered_at: Utc::now(),
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

        // Sesión activa (razonamiento largo)
        registry.register(ActiveSession {
            session_id: "active-reasoning-session".to_string(),
            client_type: "pi".to_string(),
            client_instance_id: "inst_pi_1".to_string(),
            turn_id: Some("turn-r1".to_string()),
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-reasoner".to_string(),
            schedule_id: None,
            is_active: true,
            last_heartbeat: old_time,
            registered_at: old_time,
        });

        // Sesión inactiva (finalizada)
        registry.register(ActiveSession {
            session_id: "inactive-stale-session".to_string(),
            client_type: "opencode".to_string(),
            client_instance_id: "inst_oc_1".to_string(),
            turn_id: None,
            provider_id: "deepseek_direct".to_string(),
            model: "deepseek-chat".to_string(),
            schedule_id: None,
            is_active: false,
            last_heartbeat: old_time,
            registered_at: old_time,
        });

        assert_eq!(registry.count_active(), 1);

        // Podar con inactive_timeout = 30s y active_max_silence = 30 min
        registry.prune_stale(chrono::Duration::seconds(30), chrono::Duration::minutes(30));

        // La sesión activa DEBE preservarse durante el razonamiento silencioso
        assert_eq!(registry.count_active(), 1);

        // Si la sesión activa supera el límite máximo de silencio (ej. 30 min), recién entonces se poda
        registry.prune_stale(chrono::Duration::seconds(30), chrono::Duration::seconds(50));
        assert_eq!(registry.count_active(), 0);
    }
}

