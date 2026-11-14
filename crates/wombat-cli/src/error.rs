//! Errores del CLI.
//!
//! Son los errores de *operación*: leer un insumo que no está, una clave que no
//! figura entre los emisores de confianza, un directorio de salida que ya tiene
//! una versión publicada. Los errores de *contrato* siguen siendo
//! [`wombat_core::v02::WonbError`] y [`wombat_risk::RiskError`], y viajan
//! envueltos sin perder su mensaje: quien lee la salida necesita saber si lo que
//! falló fue el bundle o la máquina.

use thiserror::Error;

/// Razón por la que un comando del CLI no pudo completarse.
#[derive(Error, Debug)]
pub enum CliError {
    #[error("no se pudo leer {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("no se pudo escribir {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// R19: las claves privadas no entran al repo. Ni al repo, ni a ningún
    /// directorio que esté bajo control de versiones.
    #[error(
        "R19: {path} está dentro del repositorio git en {repo}. \
         Una clave privada no entra a un árbol versionado: elegí una ruta fuera, \
         por ejemplo ~/.wombat-keys/demo.key"
    )]
    KeyInsideGitRepo { path: String, repo: String },

    /// R28: un bundle publicado es inmutable por versión.
    #[error(
        "R28: {path} ya contiene un bundle (bundle_version {existing}). \
         Un bundle publicado no se sobrescribe: publicá una versión nueva en otro directorio"
    )]
    BundleExists { path: String, existing: String },

    /// R1: dato no comparable solo se admite si se declara simulado.
    #[error(
        "R1: manifest.nisar.comparable es false y data_origin es \"{data_origin}\". \
         Un par no comparable solo puede publicarse como data_origin \"simulated\""
    )]
    NotComparable { data_origin: String },

    #[error(
        "la clave pública {issuer} no está en {path}. \
         Agregá el emisor a trusted-issuers.json antes de construir, o firmá con una clave que ya esté"
    )]
    IssuerNotTrusted { issuer: String, path: String },

    #[error("clave inválida en {path}: {reason}")]
    BadKeyFile { path: String, reason: String },

    #[error("insumo inválido en {path}: {reason}")]
    BadInput { path: String, reason: String },

    /// El bundle no verifica. Los motivos ya se imprimieron uno por línea; este
    /// error es el que fija el código de salida.
    #[error("el bundle en {path} no verifica: {errors} error(es)")]
    VerifyFailed { path: String, errors: usize },

    #[error(transparent)]
    Wonb(#[from] wombat_core::v02::WonbError),

    #[error(transparent)]
    Risk(#[from] wombat_risk::RiskError),

    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
}

/// Lee un archivo de texto diciendo *cuál* si falla.
pub fn read_to_string(path: impl AsRef<std::path::Path>) -> Result<String, CliError> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|source| CliError::Read {
        path: path.display().to_string(),
        source,
    })
}

/// Lee un archivo binario diciendo *cuál* si falla.
pub fn read_bytes(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, CliError> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|source| CliError::Read {
        path: path.display().to_string(),
        source,
    })
}

/// Escribe `contents` creando los directorios que falten.
pub fn write(
    path: impl AsRef<std::path::Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), CliError> {
    let path = path.as_ref();
    let fail = |source: std::io::Error| CliError::Write {
        path: path.display().to_string(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    std::fs::write(path, contents).map_err(fail)
}
