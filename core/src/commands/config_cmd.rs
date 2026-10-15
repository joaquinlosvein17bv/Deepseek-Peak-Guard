use crate::config::Config;
use std::path::Path;

pub fn validate<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn std::error::Error>> {
    let p = path.as_ref();
    println!("Validando archivo de configuración: {}", p.display());
    match Config::load_from_file(p) {
        Ok(cfg) => {
            println!("✅ Configuración válida (versión {})", cfg.config_version);
            println!("  - Modo: {}", cfg.general.mode);
            println!("  - Zona de presentación: {}", cfg.general.show_timezone);
            println!(
                "  - Gateway: {}:{}",
                cfg.gateway.listen_address, cfg.gateway.listen_port
            );
            println!(
                "  - Control API: {}:{}",
                cfg.control.listen_address, cfg.control.listen_port
            );
            println!("  - Proveedores configurados: {}", cfg.providers.len());
            for prov in &cfg.providers {
                println!(
                    "    * {} (habilitado: {}, horario: {})",
                    prov.id, prov.enabled, prov.schedule_id
                );
            }
            println!("  - Horarios definidos: {}", cfg.schedules.len());
            for s in &cfg.schedules {
                println!(
                    "    * {} [{}]: {:?}",
                    s.id, s.timezone, s.intervals
                );
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Error de validación en '{}': {}", p.display(), e);
            Err(Box::new(e))
        }
    }
}
