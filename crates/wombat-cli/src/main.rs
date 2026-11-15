//! `wombat`: el productor oficial del bundle firmado.
//!
//! R14 dice que **solo Rust firma**. El fixture `piura-demo/v0` lo generó
//! `contracts/v1/tools/gen_fixture_bundle.py` como bootstrap, para destrabar al
//! dashboard antes de que existiera este binario; este CLI es su reemplazo, y el
//! test de extremo a extremo comprueba que produce los mismos payloads.
//!
//! Tres comandos y un flujo:
//!
//! ```text
//! wombat keygen --out ~/.wombat-keys/demo.key     # una vez, fuera del repo (R19)
//! wombat build  --in <insumos> --aoi <id> --version vN --key <clave> --out <dir>
//! wombat verify <dir>
//! ```
//!
//! `publish` (libp2p) es P1 y no está acá a propósito.
//!
//! DECISIÓN: los subcomandos son los del enunciado de esta rama
//! (`keygen`/`build`/`verify`) y no los de §8.4 del plan maestro
//! (`ingest`/`export-static`/`publish`/`verify`). `build` hace en un paso lo que
//! el plan partía en `ingest` + `export-static`: no hay consumidor del estado
//! intermedio, y un paso menos es un paso que no puede desincronizarse.

mod build;
mod bundle;
mod error;
mod gitguard;
mod inputs;
mod keys;
mod verify;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use bundle::BundlePurpose;
use error::CliError;

/// Directorio de configuración por defecto, relativo al directorio de trabajo.
///
/// DECISIÓN: es un flag (`--config`) y no una ruta fija. El flujo documentado
/// usa el `config/` del repo, pero los tests necesitan su propia lista de
/// emisores de confianza, y la alternativa —que un test mute `config/` del
/// repositorio— es peor que un flag.
const DEFAULT_CONFIG_DIR: &str = "config";

/// La lista de confianza con la que `verify` mide las firmas.
///
/// DECISIÓN: es la del repositorio y **no** la que viene dentro del bundle. Un
/// bundle que trae su propia lista de emisores se autoavala: cambiarla y volver
/// a firmar da un "OK" que no significa nada. Ver el doc de módulo de
/// `verify.rs`.
const DEFAULT_TRUSTED: &str = "config/trusted-issuers.json";

#[derive(Parser)]
#[command(
    name = "wombat",
    about = "Construye y verifica bundles WONB 0.2 firmados",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Genera una clave Ed25519 nueva fuera del repositorio (R19).
    Keygen {
        /// Dónde escribir el secreto. No puede estar dentro de un repo git.
        #[arg(long)]
        out: PathBuf,

        /// Nombre del emisor en la línea lista para pegar.
        #[arg(long, default_value = "wombat-local")]
        name: String,
    },

    /// Construye un bundle firmado a partir de los insumos sin firmar.
    Build {
        /// Directorio con manifest.json, cells.geojson, roads.geojson y overlays/.
        #[arg(long = "in")]
        input: PathBuf,

        /// Identificador del AOI, p. ej. `piura-demo`.
        #[arg(long)]
        aoi: String,

        /// Versión del bundle, p. ej. `v1`. Inmutable una vez publicada (R28).
        #[arg(long)]
        version: String,

        /// Clave de firma generada con `wombat keygen`.
        #[arg(long)]
        key: PathBuf,

        /// Instante de emisión RFC 3339. Por defecto, la hora actual.
        #[arg(long = "issued-at")]
        issued_at: Option<String>,

        /// Directorio de salida. Debe no existir o estar vacío (R28).
        #[arg(long)]
        out: PathBuf,

        /// Clase de salida. Baseline/preparation no emiten Risk ni Action.
        #[arg(long, value_enum, default_value = "scientific")]
        purpose: BundlePurpose,

        /// Directorio con risk-config.json y trusted-issuers.json.
        #[arg(long, default_value = DEFAULT_CONFIG_DIR)]
        config: PathBuf,
    },

    /// Verifica un bundle completo: sha256, ids, firmas, emisores, TTL y cadena.
    Verify {
        /// Directorio del bundle.
        bundle: PathBuf,

        /// Instante contra el que se mide el TTL. Por defecto,
        /// `index.generated_at`: un bundle estático es una foto histórica.
        #[arg(long = "as-of")]
        as_of: Option<String>,

        /// Lista de emisores de confianza. Nunca se lee la del bundle.
        #[arg(long, default_value = DEFAULT_TRUSTED)]
        trusted: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            let mut source = std::error::Error::source(&err);
            while let Some(cause) = source {
                eprintln!("  causa: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), CliError> {
    match Cli::parse().command {
        Command::Keygen { out, name } => keygen(&out, &name),
        Command::Build {
            input,
            aoi,
            version,
            key,
            issued_at,
            out,
            purpose,
            config,
        } => {
            let request = build::Request {
                input_dir: input,
                aoi_id: aoi,
                bundle_version: version,
                key_path: key,
                issued_at,
                out_dir: out,
                purpose,
                config_dir: config,
            };
            let report = build::run(&request)?;
            println!(
                "OK   {}  |  {} bloques | issued_at {} | issuer {}…",
                report.out_dir.display(),
                report.blocks,
                report.issued_at,
                &report.issuer[..20.min(report.issuer.len())]
            );
            Ok(())
        }
        Command::Verify {
            bundle,
            as_of,
            trusted,
        } => verify::run(&bundle, as_of.as_deref(), &trusted).map(|_| ()),
    }
}

/// Genera la clave e imprime lo único que se puede compartir: la pública.
fn keygen(out: &std::path::Path, name: &str) -> Result<(), CliError> {
    let verifying = keys::generate_to_file(out)?;
    let issuer = wombat_core::v02::issuer_string(&verifying);

    println!("clave privada escrita en {}", out.display());
    println!("  {}", keys::permissions_note());
    println!();
    println!("clave pública:");
    println!("  {issuer}");
    println!();
    println!("línea para config/trusted-issuers.json:");
    println!(
        "  {{ \"issuer\": \"{issuer}\", \"name\": \"{name}\", \
         \"roles\": [\"change\", \"context\", \"risk\", \"action\"] }}"
    );
    Ok(())
}
