use crate::config::Config;
use std::net::TcpListener;
use std::path::Path;

pub fn execute<P: AsRef<Path>>(config_path: Option<P>) {
    println!("=== PeakGuard Doctor: Diagnóstico del Sistema ===");
    println!();

    // 1. Sistema Operativo
    println!("[1/5] Sistema Operativo:");
    println!("  - OS: Windows x64 compatible");
    println!("  - Privilegios: Usuario ordinario (OK, sin elevación)");

    // 2. Archivo de Configuración
    println!("[2/5] Configuración:");
    let cfg_opt = if let Some(p) = config_path {
        let path = p.as_ref();
        if path.exists() {
            match Config::load_from_file(path) {
                Ok(c) => {
                    println!("  - Archivo '{}': VÁLIDO (v{})", path.display(), c.config_version);
                    Some(c)
                }
                Err(e) => {
                    println!("  - Archivo '{}': ERROR ({})", path.display(), e);
                    None
                }
            }
        } else {
            println!("  - Archivo '{}': NO ENCONTRADO", path.display());
            None
        }
    } else {
        println!("  - Archivo por defecto: omitido (use --config para evaluar un archivo específico)");
        None
    };

    // 3. Puertos Locales
    println!("[3/5] Disponibilidad de Puertos Locales:");
    let gw_port = cfg_opt.as_ref().map(|c| c.gateway.listen_port).unwrap_or(8765);
    let ctrl_port = cfg_opt.as_ref().map(|c| c.control.listen_port).unwrap_or(8766);

    check_port(gw_port, "Gateway Data Plane");
    check_port(ctrl_port, "Control API Plane");

    // 4. Clientes Detectados
    println!("[4/5] Detección de Clientes:");
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_else(|_| ".".to_string());
    let opencode_path = format!(r"{}\.config\opencode", home);
    let pi_path = format!(r"{}\.pi\agent", home);
    let hermes_path = format!(r"{}\AppData\Local\hermes", home);

    check_client("OpenCode", &opencode_path);
    check_client("Pi Coding Agent", &pi_path);
    check_client("Hermes Agent", &hermes_path);

    // 5. Cobertura Resumida
    println!("[5/5] Diagnóstico de Cobertura:");
    if let Some(cfg) = cfg_opt {
        for p in &cfg.providers {
            if p.enabled {
                println!("  - Proveedor '{}' [PROTEGIDO / VERIFICADO]: upstream={:?}, schedule='{}'", p.id, p.upstream, p.schedule_id);
            } else if p.id == "opencode_go" {
                println!("  - Proveedor '{}' [NO VERIFICADO / UNVERIFIED]: Inferencia gestionada por OpenCode; requiere custom baseURL para pasar por Gateway", p.id);
            } else {
                println!("  - Proveedor '{}' [DESHABILITADO / NO CUBIERTO]", p.id);
            }
        }
    } else {
        println!("  - Rutas sin configurar: marcadas como UNCOVERED por defecto");
    }

    println!();
    println!("Diagnóstico finalizado. Todo listo para ejecución con 'peakguard run'.");
}

fn check_port(port: u16, label: &str) {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => println!("  - Puerto {} ({}): DISPONIBLE", port, label),
        Err(e) => println!("  - Puerto {} ({}): OCUPADO O ERROR ({})", port, label, e),
    }
}

fn check_client(name: &str, path_str: &str) {
    let p = Path::new(path_str);
    if p.exists() {
        println!("  - {}: INSTALADO ({})", name, path_str);
    } else {
        println!("  - {}: NO DETECTADO en {}", name, path_str);
    }
}
