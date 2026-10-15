use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::debug;

use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::registry::ModelRegistry;
use crate::schedule::Schedule;

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub struct McpServer {
    pub config: Arc<Config>,
    pub schedule: Arc<Schedule>,
    pub registry: Arc<ModelRegistry>,
    pub clock: Arc<dyn Clock>,
}

impl McpServer {
    pub fn new(config: Config) -> Self {
        Self::with_clock(config, Arc::new(SystemClock))
    }

    pub fn with_clock(config: Config, clock: Arc<dyn Clock>) -> Self {
        let schedule = if let Some(first_sched) = config.schedules.first() {
            let holidays = if first_sched.exclude_chinese_public_holidays {
                crate::schedule::chinese_public_holidays_2026()
            } else {
                vec![]
            };
            Schedule::new(
                &first_sched.id,
                &first_sched.intervals,
                &first_sched.weekdays,
                first_sched.exclude_chinese_public_holidays,
                &holidays,
            )
        } else {
            Schedule::default_deepseek()
        };

        Self {
            config: Arc::new(config),
            schedule: Arc::new(schedule),
            registry: Arc::new(ModelRegistry::default()),
            clock,
        }
    }

    pub fn handle_json_rpc(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: Some(Value::Null),
                    result: None,
                    error: Some(JsonRpcError {
                        code: PARSE_ERROR,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                return serde_json::to_string(&resp).ok();
            }
        };

        if request.jsonrpc != "2.0" {
            let resp = JsonRpcResponse {
                jsonrpc: "2.0",
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: INVALID_REQUEST,
                    message: "Invalid Request: 'jsonrpc' must be '2.0'".to_string(),
                    data: None,
                }),
            };
            return serde_json::to_string(&resp).ok();
        }

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "notifications/initialized" => {
                debug!("MCP: Notificación 'initialized' recibida");
                return None;
            }
            "ping" => JsonRpcResponse {
                jsonrpc: "2.0",
                id: request.id,
                result: Some(json!({})),
                error: None,
            },
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params),
            other => JsonRpcResponse {
                jsonrpc: "2.0",
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: METHOD_NOT_FOUND,
                    message: format!("Method not found: '{}'", other),
                    data: None,
                }),
            },
        };

        serde_json::to_string(&response).ok()
    }

    fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "peakguard",
                    "version": "0.1.0"
                }
            })),
            error: None,
        }
    }

    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "tools": [
                    {
                        "name": "peakguard_status",
                        "description": "Obtiene el estado actual de PeakGuard: hora UTC, modo de operación, estado peak/off-peak, y próxima transición.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "peakguard_next_peak",
                        "description": "Devuelve detalles del próximo horario peak de DeepSeek: timestamp UTC de inicio, fin estimado y segundos restantes.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "peakguard_models",
                        "description": "Lista el catálogo de modelos registrados y su nivel de protección y cobertura ante horarios peak.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {},
                            "additionalProperties": false
                        }
                    },
                    {
                        "name": "peakguard_coverage",
                        "description": "Verifica si un modelo específico (ej: deepseek-chat, claude-3-5-sonnet) está cubierto por PeakGuard o si transita sin protección.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "model_id": {
                                    "type": "string",
                                    "description": "Identificador o alias del modelo a consultar (ej: deepseek-chat, deepseek-reasoner, claude-3-5-sonnet, gpt-4o)"
                                }
                            },
                            "required": ["model_id"],
                            "additionalProperties": false
                        }
                    }
                ]
            })),
            error: None,
        }
    }

    fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params_val = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: INVALID_PARAMS,
                        message: "Faltan parámetros 'params' en la llamada tools/call".to_string(),
                        data: None,
                    }),
                }
            }
        };

        let tool_name = params_val.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = params_val.get("arguments").cloned().unwrap_or(json!({}));

        match tool_name {
            "peakguard_status" => self.execute_status_tool(id),
            "peakguard_next_peak" => self.execute_next_peak_tool(id),
            "peakguard_models" => self.execute_models_tool(id),
            "peakguard_coverage" => self.execute_coverage_tool(id, arguments),
            "set_mode" | "peakguard_set_mode" | "config" => {
                let error_msg = "Acción denegada por política de seguridad (§9.4): Las operaciones de modificación o cambio de modo están deshabilitadas en el servidor MCP para prevenir que un modelo de IA se auto-desbloquee durante horas peak. Los cambios de modo deben ser configurados interactivamente por el usuario a través de la CLI ('peakguard config') o el panel TUI de su editor.";
                JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": error_msg
                            }
                        ],
                        "isError": true
                    })),
                    error: None,
                }
            }
            unknown => {
                let error_msg = format!(
                    "Herramienta no encontrada: '{}'. Herramientas disponibles: peakguard_status, peakguard_next_peak, peakguard_models, peakguard_coverage.",
                    unknown
                );
                JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": error_msg
                            }
                        ],
                        "isError": true
                    })),
                    error: None,
                }
            }
        }
    }

    fn execute_status_tool(&self, id: Option<Value>) -> JsonRpcResponse {
        let now = self.clock.now();
        let is_peak = self.schedule.is_peak(now);
        let (next_transition_type, next_transition_utc, seconds_remaining) = if is_peak {
            if let Some((next_off, secs)) = self.schedule.next_off_peak(now) {
                ("off_peak_start", Some(next_off.to_rfc3339()), Some(secs))
            } else {
                ("off_peak_start", None, None)
            }
        } else if let Some((next_p, secs)) = self.schedule.next_peak(now) {
            ("peak_start", Some(next_p.to_rfc3339()), Some(secs))
        } else {
            ("peak_start", None, None)
        };

        let result_obj = json!({
            "utc_now": now.to_rfc3339(),
            "mode": self.config.general.mode,
            "is_peak": is_peak,
            "schedule_id": self.schedule.id,
            "next_transition_type": next_transition_type,
            "next_transition_utc": next_transition_utc,
            "seconds_to_transition": seconds_remaining,
            "gateway_endpoint": format!("http://{}:{}", self.config.gateway.listen_address, self.config.gateway.listen_port),
            "control_api_endpoint": format!("http://{}:{}", self.config.control.listen_address, self.config.control.listen_port),
            "status": if is_peak { "ACTIVE_PEAK" } else { "OFF_PEAK_SAFE" }
        });

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result_obj).unwrap_or_default()
                    }
                ],
                "isError": false
            })),
            error: None,
        }
    }

    fn execute_next_peak_tool(&self, id: Option<Value>) -> JsonRpcResponse {
        let now = self.clock.now();
        let is_peak = self.schedule.is_peak(now);
        let next_peak_info = self.schedule.next_peak(now);
        let intervals: Vec<String> = self
            .schedule
            .intervals
            .iter()
            .map(|i| format!("{}-{}", i.start, i.end))
            .collect();

        let human_remaining = next_peak_info
            .map(|(_, s)| {
                let hours = s / 3600;
                let mins = (s % 3600) / 60;
                let secs = s % 60;
                format!("{}h {}m {}s", hours, mins, secs)
            })
            .unwrap_or_else(|| {
                if is_peak {
                    "En curso (horario peak activo)".to_string()
                } else {
                    "No programado en el horizonte".to_string()
                }
            });

        let result_obj = json!({
            "utc_now": now.to_rfc3339(),
            "is_peak_currently": is_peak,
            "schedule_id": self.schedule.id,
            "intervals_utc": intervals,
            "next_peak_start_utc": next_peak_info.as_ref().map(|(dt, _)| dt.to_rfc3339()),
            "seconds_remaining": next_peak_info.as_ref().map(|(_, s)| *s),
            "human_readable_remaining": human_remaining
        });

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result_obj).unwrap_or_default()
                    }
                ],
                "isError": false
            })),
            error: None,
        }
    }

    fn execute_models_tool(&self, id: Option<Value>) -> JsonRpcResponse {
        let models = self.registry.list_all_models();
        let model_entries: Vec<Value> = models
            .iter()
            .map(|m| {
                let is_direct_deepseek = m.provider_id == "deepseek_direct";
                json!({
                    "id": m.id,
                    "name": m.name,
                    "aliases": m.aliases,
                    "provider_id": m.provider_id,
                    "coverage_when_proxied": if is_direct_deepseek { "Protected" } else { "Partial" },
                    "policy_action": if is_direct_deepseek { "403 o abort según modo en peak" } else { "Sujeto a disponibilidad del upstream" }
                })
            })
            .collect();

        let result_obj = json!({
            "registered_models": model_entries,
            "third_party_policy": "Principio P2: Todo modelo no registrado de terceros (ej: claude-3-5-sonnet, gpt-4o, gemini-2.0-flash) transita libremente sin ningún bloqueo ni interrupción."
        });

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result_obj).unwrap_or_default()
                    }
                ],
                "isError": false
            })),
            error: None,
        }
    }

    fn execute_coverage_tool(&self, id: Option<Value>, arguments: Value) -> JsonRpcResponse {
        let model_id = match arguments.get("model_id").and_then(|v| v.as_str()) {
            Some(m) if !m.trim().is_empty() => m.trim(),
            _ => {
                let error_msg = "Parámetro requerido 'model_id' ausente o vacío.";
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": error_msg
                            }
                        ],
                        "isError": true
                    })),
                    error: None,
                };
            }
        };

        let resolved = self.registry.resolve_model(model_id);
        let (is_deepseek, provider, coverage_proxied) = if let Some(m) = resolved {
            (
                true,
                Some(m.provider_id.as_str()),
                self.registry.classify_coverage(model_id, true),
            )
        } else {
            let lower = model_id.to_lowercase();
            let guess_deepseek = lower.contains("deepseek");
            (
                guess_deepseek,
                None,
                self.registry.classify_coverage(model_id, true),
            )
        };

        let explanation = if is_deepseek {
            "Este modelo pertenece a la familia DeepSeek. Si transita a través del Gateway local (127.0.0.1:8765), estará estrictamente protegido según las ventanas de horario peak. Si se realiza una petición directa que no pasa por el gateway local, el tráfico estará desprotegido (UNCOVERED)."
        } else {
            "Este modelo NO pertenece a DeepSeek (es un modelo de terceros, e.g. Anthropic, OpenAI, Google). Por el Principio P2 de PeakGuard, nunca será bloqueado, detenido ni afectado por ninguna regla de horario peak."
        };

        let result_obj = json!({
            "query_model": model_id,
            "resolved_canonical_id": resolved.map(|m| &m.id),
            "provider_id": provider,
            "is_deepseek_family": is_deepseek,
            "coverage_through_gateway": format!("{:?}", coverage_proxied),
            "coverage_direct_bypass": "Uncovered",
            "policy_action_during_peak": if is_deepseek { "Bloqueo 403 o abort de turno" } else { "Permitido (sin bloqueo)" },
            "explanation": explanation
        });

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [
                    {
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result_obj).unwrap_or_default()
                    }
                ],
                "isError": false
            })),
            error: None,
        }
    }

    pub async fn run_stdio(&self) -> Result<(), Box<dyn std::error::Error>> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(resp_json) = self.handle_json_rpc(trimmed) {
                stdout.write_all(resp_json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }

        Ok(())
    }
}
