use crate::config::Config;
use chrono::Utc;
use std::path::Path;

pub fn execute<P: AsRef<Path>>(config_path: Option<P>) {
    let now = Utc::now();
    println!("=== PeakGuard Estado Actual ===");
    println!("Hora UTC actual: {}", now.to_rfc3339());

    if let Some(path) = config_path {
        match Config::load_from_file(path.as_ref()) {
            Ok(cfg) => {
                println!("Modo configurado: {}", cfg.general.mode);
                println!("Estado del servicio: LISTO (offline/daemon inactivo)");
                println!("Gateway local: {}:{}", cfg.gateway.listen_address, cfg.gateway.listen_port);
                println!("Alertas activas: {:?}", cfg.alerts.minutes_before);
            }
            Err(e) => {
                eprintln!("Advertencia: no se pudo leer configuración: {}", e);
            }
        }
    } else {
        println!("(No se especificó archivo de configuración; use --config para detalles de políticas)");
    }
}
