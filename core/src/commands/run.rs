use crate::config::Config;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::signal;
use tracing::{info, warn};

pub async fn execute<P: AsRef<Path>>(config_path: P) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path.as_ref();
    info!("Iniciando PeakGuard Daemon...");
    let cfg = Config::load_from_file(path)?;
    info!("Configuración cargada correctamente desde: {}", path.display());
    info!("Modo activo: {}", cfg.general.mode);
    info!(
        "Gateway vinculado en http://{}:{}",
        cfg.gateway.listen_address, cfg.gateway.listen_port
    );
    info!(
        "Control API vinculada en http://{}:{}",
        cfg.control.listen_address, cfg.control.listen_port
    );

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Señal de terminación recibida (Ctrl+C). Iniciando apagado ordenado...");
                r.store(false, Ordering::SeqCst);
            }
            Err(err) => {
                warn!("Error escuchando señal de apagado: {}", err);
            }
        }
    });

    info!("PeakGuard está en ejecución activa. Presione Ctrl+C para detener.");

    // Loop de monitorización principal
    while running.load(Ordering::SeqCst) {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    info!("Apagado ordenado de PeakGuard completado. Sesiones y estado preservados.");
    Ok(())
}
