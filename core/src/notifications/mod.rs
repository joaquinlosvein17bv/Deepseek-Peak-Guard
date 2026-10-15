use serde::Serialize;
use std::collections::HashSet;
use std::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DeduplicationKey {
    pub schedule_id: String,
    pub window_start_utc: String,
    pub threshold: String, // "warning_30", "warning_10", "warning_5", "countdown_start", "peak_active", "peak_ended"
    pub channel: String,   // "telegram", "desktop", "tui"
}

pub struct NotificationEngine {
    seen_alerts: RwLock<HashSet<DeduplicationKey>>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<String>,
    http_client: reqwest::Client,
}

#[derive(Serialize)]
struct TelegramSendMessagePayload<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
}

impl NotificationEngine {
    pub fn new(bot_token: Option<String>, chat_id: Option<String>) -> Self {
        Self {
            seen_alerts: RwLock::new(HashSet::new()),
            telegram_bot_token: bot_token,
            telegram_chat_id: chat_id,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Comprueba si una alerta ya fue emitida sin registrarla
    pub fn is_seen(&self, key: &DeduplicationKey) -> bool {
        self.seen_alerts.read().unwrap().contains(key)
    }

    /// Comprueba y registra si la alerta ya fue emitida para evitar spam o duplicados
    pub fn should_emit(&self, key: &DeduplicationKey) -> bool {
        let mut seen = self.seen_alerts.write().unwrap();
        if seen.contains(key) {
            false
        } else {
            seen.insert(key.clone());
            true
        }
    }

    /// Emite aviso por Telegram si está configurado
    pub async fn notify_telegram(&self, key: &DeduplicationKey, message: &str) -> bool {
        if !self.should_emit(key) {
            return false;
        }

        let token = match &self.telegram_bot_token {
            Some(t) if !t.is_empty() => t,
            _ => return false,
        };

        let chat_id = match &self.telegram_chat_id {
            Some(c) if !c.is_empty() => c,
            _ => return false,
        };

        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let payload = TelegramSendMessagePayload {
            chat_id,
            text: message,
            parse_mode: "Markdown",
        };

        match self.http_client.post(&url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Alerta de Telegram enviada exitosamente para '{}'", key.threshold);
                true
            }
            Ok(resp) => {
                warn!("Telegram API devolvió estado no exitoso: {}", resp.status());
                false
            }
            Err(e) => {
                warn!("Fallo al enviar notificación por Telegram (la política no se ve afectada): {}", e);
                false
            }
        }
    }

    /// Emite aviso local en Windows
    pub fn notify_desktop(&self, key: &DeduplicationKey, title: &str, body: &str) -> bool {
        if !self.should_emit(key) {
            return false;
        }

        info!("[Notificación Escritorio] {}: {}", title, body);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplication_exact_sequence() {
        let engine = NotificationEngine::new(None, None);

        let k1 = DeduplicationKey {
            schedule_id: "deepseek_direct_verified".to_string(),
            window_start_utc: "2026-10-19T01:00:00Z".to_string(),
            threshold: "warning_30".to_string(),
            channel: "telegram".to_string(),
        };

        // Primera vez: debe emitir
        assert!(engine.should_emit(&k1));

        // Segunda vez con la misma clave: debe suprimir
        assert!(!engine.should_emit(&k1));

        // Clave para umbral de 10 min en la misma ventana: debe emitir una vez
        let k2 = DeduplicationKey {
            threshold: "warning_10".to_string(),
            ..k1.clone()
        };
        assert!(engine.should_emit(&k2));
        assert!(!engine.should_emit(&k2));

        // Mismo umbral pero canal diferente (desktop vs telegram): debe emitir
        let k3 = DeduplicationKey {
            channel: "desktop".to_string(),
            ..k1.clone()
        };
        assert!(engine.should_emit(&k3));
        assert!(!engine.should_emit(&k3));
    }
}
