pub mod clock;
mod commands;
pub mod config;
mod logger;
pub mod registry;
pub mod schedule;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "peakguard",
    about = "Guardián ligero y determinista de horarios Peak/Off-Peak para modelos de IA (DeepSeek)",
    version
)]
struct Cli {
    #[arg(short, long, value_name = "FILE", help = "Ruta al archivo peakguard.toml")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Muestra la versión, plataforma y arquitectura del binario")]
    Version,

    #[command(about = "Muestra el estado actual del reloj UTC, modo y ventanas")]
    Status,

    #[command(about = "Ejecuta diagnóstico de puertos, configuración, clientes y upstreams")]
    Doctor,

    #[command(about = "Comandos de gestión y validación de configuración")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    #[command(about = "Inicia el demonio de PeakGuard (Gateway + Control API + Monitor)")]
    Run,
}

#[derive(Subcommand)]
enum ConfigAction {
    #[command(about = "Valida la sintaxis y semántica de un archivo peakguard.toml")]
    Validate {
        #[arg(value_name = "FILE", help = "Archivo a validar")]
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init_logger();
    let cli = Cli::parse();

    let default_config = PathBuf::from("config/peakguard.example.toml");
    let config_path = cli.config.or_else(|| {
        if default_config.exists() {
            Some(default_config)
        } else if PathBuf::from("../config/peakguard.example.toml").exists() {
            Some(PathBuf::from("../config/peakguard.example.toml"))
        } else {
            None
        }
    });

    match cli.command {
        Some(Commands::Version) => {
            commands::version::execute();
        }
        Some(Commands::Status) => {
            commands::status::execute(config_path.as_deref());
        }
        Some(Commands::Doctor) => {
            commands::doctor::execute(config_path.as_deref());
        }
        Some(Commands::Config { action }) => match action {
            ConfigAction::Validate { file } => {
                commands::config_cmd::validate(file)?;
            }
        },
        Some(Commands::Run) => {
            let p = config_path.ok_or_else(|| {
                "No se encontró archivo de configuración. Especifique uno con --config <path>."
            })?;
            commands::run::execute(p).await?;
        }
        None => {
            // Sin subcomando: muestra estado por defecto
            commands::status::execute(config_path.as_deref());
        }
    }

    Ok(())
}
