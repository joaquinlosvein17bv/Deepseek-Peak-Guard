use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("No se pudo leer el archivo de configuración: {0}")]
    Io(#[from] std::io::Error),

    #[error("Error de sintaxis en el archivo TOML: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Error de validación: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub config_version: u32,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub alerts: AlertsConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub control: ControlConfig,
    #[serde(default)]
    pub stop: StopConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub schedules: Vec<ScheduleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mode")]
    pub mode: String, // "strict_block" | "restricted_stop" | "notify_only"
    #[serde(default = "default_timezone")]
    pub show_timezone: String,
    #[serde(default)]
    pub start_on_login: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: default_mode(),
            show_timezone: default_timezone(),
            start_on_login: false,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_mode() -> String {
    "restricted_stop".to_string()
}

fn default_timezone() -> String {
    "America/Lima".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsConfig {
    #[serde(default = "default_minutes_before")]
    pub minutes_before: Vec<u32>,
    #[serde(default = "default_countdown")]
    pub final_countdown_seconds: u32,
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default = "default_true")]
    pub tui: bool,
    #[serde(default)]
    pub telegram: bool,
    #[serde(default = "default_true")]
    pub notify_on_peak_end: bool,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            minutes_before: default_minutes_before(),
            final_countdown_seconds: default_countdown(),
            desktop: true,
            tui: true,
            telegram: false,
            notify_on_peak_end: true,
        }
    }
}

fn default_minutes_before() -> Vec<u32> {
    vec![30, 10, 5]
}

fn default_countdown() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_listen_address")]
    pub listen_address: String,
    #[serde(default = "default_gateway_port")]
    pub listen_port: u16,
    #[serde(default)]
    pub allow_remote_connections: bool,
    #[serde(default = "default_true")]
    pub fail_closed_for_protected_routes: bool,
    #[serde(default = "default_stream_action")]
    pub active_stream_on_peak: String, // "cancel"
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            listen_port: default_gateway_port(),
            allow_remote_connections: false,
            fail_closed_for_protected_routes: true,
            active_stream_on_peak: default_stream_action(),
        }
    }
}

fn default_listen_address() -> String {
    "127.0.0.1".to_string()
}

fn default_gateway_port() -> u16 {
    8765
}

fn default_stream_action() -> String {
    "cancel".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlConfig {
    #[serde(default = "default_listen_address")]
    pub listen_address: String,
    #[serde(default = "default_control_port")]
    pub listen_port: u16,
    #[serde(default = "default_token_env")]
    pub token_env: String,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            listen_port: default_control_port(),
            token_env: default_token_env(),
        }
    }
}

fn default_control_port() -> u16 {
    8766
}

fn default_token_env() -> String {
    "PEAKGUARD_CONTROL_TOKEN".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopConfig {
    #[serde(default = "default_ack_timeout")]
    pub ack_timeout_ms: u64,
    #[serde(default)]
    pub auto_resume: bool,
    #[serde(default = "default_true")]
    pub fallback_to_gateway_cancel: bool,
}

impl Default for StopConfig {
    fn default() -> Self {
        Self {
            ack_timeout_ms: default_ack_timeout(),
            auto_resume: false,
            fallback_to_gateway_cancel: true,
        }
    }
}

fn default_ack_timeout() -> u64 {
    2000
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub bot_token_env: Option<String>,
    #[serde(default)]
    pub chat_id_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default)]
    pub upstream: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub schedule_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub id: String,
    #[serde(default = "default_utc")]
    pub timezone: String,
    pub weekdays: Vec<String>,
    pub intervals: Vec<String>,
    #[serde(default)]
    pub exclude_chinese_public_holidays: bool,
    #[serde(default)]
    pub holidays: Vec<String>,
    #[serde(default)]
    pub source_url: Option<String>,
}

fn default_utc() -> String {
    "UTC".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: 1,
            general: GeneralConfig::default(),
            alerts: AlertsConfig::default(),
            gateway: GatewayConfig::default(),
            control: ControlConfig::default(),
            stop: StopConfig::default(),
            telegram: TelegramConfig::default(),
            providers: vec![ProviderConfig {
                id: "deepseek_direct".to_string(),
                upstream: Some("https://api.deepseek.com".to_string()),
                enabled: true,
                schedule_id: "deepseek_direct_verified".to_string(),
            }],
            schedules: vec![ScheduleConfig {
                id: "deepseek_direct_verified".to_string(),
                timezone: "UTC".to_string(),
                weekdays: vec![
                    "mon".to_string(),
                    "tue".to_string(),
                    "wed".to_string(),
                    "thu".to_string(),
                    "fri".to_string(),
                ],
                intervals: vec!["01:00-04:00".to_string(), "06:00-10:00".to_string()],
                exclude_chinese_public_holidays: true,
                holidays: vec![],
                source_url: Some("https://api-docs.deepseek.com/quick_start/pricing/".to_string()),
            }],
        }
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validar versión
        if self.config_version != 1 {
            return Err(ConfigError::Validation(format!(
                "config_version no soportada: {}. Se requiere versión 1.",
                self.config_version
            )));
        }

        // Validar modo
        let mode = self.general.mode.as_str();
        if mode != "strict_block" && mode != "restricted_stop" && mode != "notify_only" {
            return Err(ConfigError::Validation(format!(
                "Modo inválido '{}'. Modos permitidos: strict_block, restricted_stop, notify_only.",
                mode
            )));
        }

        // Validar puertos
        if self.gateway.listen_port == 0 {
            return Err(ConfigError::Validation(
                "El puerto del gateway (listen_port) no puede ser 0.".into(),
            ));
        }
        if self.control.listen_port == 0 {
            return Err(ConfigError::Validation(
                "El puerto de control (listen_port) no puede ser 0.".into(),
            ));
        }
        if self.gateway.listen_port == self.control.listen_port {
            return Err(ConfigError::Validation(format!(
                "Conflicto de puertos: gateway y control usan el mismo puerto ({}).",
                self.gateway.listen_port
            )));
        }

        // Validar interfaces de loopback si allow_remote_connections es false
        if !self.gateway.allow_remote_connections && self.gateway.listen_address != "127.0.0.1" && self.gateway.listen_address != "localhost" {
            return Err(ConfigError::Validation(format!(
                "Dirección de gateway insegura '{}'. Se requiere '127.0.0.1' cuando allow_remote_connections es false.",
                self.gateway.listen_address
            )));
        }

        // Validar horarios referenciados por proveedores
        for p in &self.providers {
            if p.enabled {
                let found = self.schedules.iter().any(|s| s.id == p.schedule_id);
                if !found {
                    return Err(ConfigError::Validation(format!(
                        "El proveedor '{}' referencia un horario inexistente: '{}'.",
                        p.id, p.schedule_id
                    )));
                }
                if p.upstream.is_none() {
                    return Err(ConfigError::Validation(format!(
                        "El proveedor habilitado '{}' debe especificar un upstream URL válido.",
                        p.id
                    )));
                }
            }
        }

        // Validar formato de intervalos
        for s in &self.schedules {
            for interval in &s.intervals {
                let parts: Vec<&str> = interval.split('-').collect();
                if parts.len() != 2 {
                    return Err(ConfigError::Validation(format!(
                        "Intervalo horario mal formado en schedule '{}': '{}'. Formato esperado: 'HH:MM-HH:MM'.",
                        s.id, interval
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_default_config() {
        let toml_str = r#"
            config_version = 1

            [general]
            mode = "restricted_stop"

            [gateway]
            listen_port = 8765

            [control]
            listen_port = 8766

            [[providers]]
            id = "deepseek_direct"
            upstream = "https://api.deepseek.com"
            enabled = true
            schedule_id = "test_sched"

            [[schedules]]
            id = "test_sched"
            weekdays = ["mon", "tue"]
            intervals = ["01:00-04:00"]
        "#;

        let cfg: Result<Config, _> = toml::from_str(toml_str);
        assert!(cfg.is_ok());
        let cfg = cfg.unwrap();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_invalid_mode() {
        let toml_str = r#"
            config_version = 1
            [general]
            mode = "invalid_mode_name"
        "#;

        let cfg: Config = toml::from_str(toml_str).unwrap();
        let err = cfg.validate();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Modo inválido"));
    }

    #[test]
    fn test_conflicting_ports() {
        let toml_str = r#"
            config_version = 1
            [gateway]
            listen_port = 8888
            [control]
            listen_port = 8888
        "#;

        let cfg: Config = toml::from_str(toml_str).unwrap();
        let err = cfg.validate();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Conflicto de puertos"));
    }

    #[test]
    fn test_missing_referenced_schedule() {
        let toml_str = r#"
            config_version = 1
            [[providers]]
            id = "deepseek_direct"
            upstream = "https://api.deepseek.com"
            enabled = true
            schedule_id = "non_existent_schedule"
        "#;

        let cfg: Config = toml::from_str(toml_str).unwrap();
        let err = cfg.validate();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("horario inexistente"));
    }
}

