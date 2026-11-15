//! El bundle en disco: `index.json`, los sha256 y la inmutabilidad (R28).
//!
//! El índice es lo único del bundle que no está firmado, y por eso es lo único
//! que puede mentir. Lo que lo sostiene es que cada entrada lleva el sha256 del
//! archivo y que el bloque Change —que sí está firmado— repite los hashes de
//! `cells.geojson` y de los overlays. Un índice alterado se nota.

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wombat_core::v02::DataOrigin;

use crate::error::{self, CliError};

/// `index.json` de una versión publicada.
///
/// El orden de los campos es el del contrato (`bundle-index.schema.json`) y el
/// del fixture, para que un `diff` entre dos índices se lea de corrido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub contract_version: String,
    pub bundle_version: String,
    pub aoi_id: String,
    pub data_origin: DataOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<BundlePurpose>,
    pub generated_at: String,
    pub core_commit: String,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum BundlePurpose {
    Scientific,
    Baseline,
    Preparation,
}

/// Un archivo del bundle y su hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
}

impl Index {
    pub fn load(path: &Path) -> Result<Self, CliError> {
        let raw = error::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(|e| CliError::BadInput {
            path: path.display().to_string(),
            reason: e.to_string(),
        })
    }
}

/// sha256 hexadecimal del contenido de un archivo.
pub fn sha256_file(path: &Path) -> Result<String, CliError> {
    Ok(sha256_bytes(&error::read_bytes(path)?))
}

/// sha256 hexadecimal de unos bytes.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// R28: un bundle publicado es inmutable por versión.
///
/// DECISIÓN: se rechaza si `out` ya tiene un `index.json`, **sea la versión que
/// sea**. El enunciado pide rechazar cuando la versión difiere; rechazar
/// también cuando coincide es más estricto y es lo correcto: reconstruir
/// `v3` encima de `v3` cambia los `id` y las firmas de todos los bloques —basta
/// con que cambie `issued_at`— y cualquiera que ya hubiera fijado ese bundle
/// vería otros hashes bajo la misma versión. Publicar es irreversible; el CLI
/// se niega y pide un directorio nuevo.
pub fn refuse_if_published(out: &Path) -> Result<(), CliError> {
    let index_path = out.join("index.json");
    if !index_path.exists() {
        return Ok(());
    }
    // Si el índice existe pero no se puede leer, igual no se sobrescribe: se
    // informa lo que se sabe en vez de pisar algo que no se entiende.
    let existing = Index::load(&index_path)
        .map(|index| index.bundle_version)
        .unwrap_or_else(|_| "ilegible".to_string());
    Err(CliError::BundleExists {
        path: out.display().to_string(),
        existing,
    })
}

/// Valida la forma de `--version` contra el contrato antes de escribir nada.
pub fn check_version(version: &str) -> Result<(), CliError> {
    let shape_ok = version.starts_with('v')
        && version.len() > 1
        && version[1..].chars().all(|c| c.is_ascii_digit());
    if shape_ok {
        return Ok(());
    }
    Err(CliError::BadInput {
        path: "--version".to_string(),
        reason: format!("\"{version}\" no cumple ^v[0-9]+$ (p. ej. v0, v1, v12)"),
    })
}

/// Recorre el bundle y arma las entradas del índice.
///
/// Las rutas se emiten con `/` siempre: un índice generado en Windows tiene que
/// poder leerse en el dashboard, y `\` ahí no es un separador sino un carácter.
/// El orden es alfabético, el mismo que usa el fixture.
pub fn index_entries(out: &Path) -> Result<Vec<FileEntry>, CliError> {
    let mut paths = Vec::new();
    collect_files(out, out, &mut paths)?;
    paths.sort();

    paths
        .iter()
        .map(|relative| {
            Ok(FileEntry {
                path: relative.clone(),
                sha256: sha256_file(&out.join(relative))?,
            })
        })
        .collect()
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), CliError> {
    let entries = std::fs::read_dir(dir).map_err(|source| CliError::Read {
        path: dir.display().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CliError::Read {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.file_name().is_some_and(|n| n != "index.json") {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push(relative);
        }
    }
    Ok(())
}

/// Escribe un valor como JSON indentado con salto final, igual que el fixture.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), CliError> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    error::write(path, json)
}

/// Copia un archivo sin tocar sus bytes y devuelve su destino.
pub fn copy_verbatim(from: &Path, to: &Path) -> Result<PathBuf, CliError> {
    let bytes = error::read_bytes(from)?;
    error::write(to, &bytes)?;
    Ok(to.to_path_buf())
}

/// El commit de `wombat-core` con el que se construyó, para `index.core_commit`.
///
/// DECISIÓN: si no hay git o el comando falla, devuelve `"unknown"` en vez de
/// abortar. El commit es trazabilidad, no corrección: negarse a construir un
/// bundle verificable porque falta git sería cambiar un dato de auditoría por
/// la imposibilidad de publicar.
pub fn core_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_shape_follows_the_contract() {
        for ok in ["v0", "v1", "v12", "v999"] {
            check_version(ok).unwrap_or_else(|e| panic!("{ok} debería valer: {e}"));
        }
        for bad in ["", "v", "1", "V1", "v1.0", "v-1", "latest", "v1a"] {
            assert!(check_version(bad).is_err(), "{bad:?} no debería valer");
        }
    }

    #[test]
    fn an_empty_directory_is_not_published_yet() {
        let dir = tempfile::tempdir().unwrap();
        refuse_if_published(dir.path()).unwrap();
    }

    #[test]
    fn a_directory_with_an_index_is_refused_naming_its_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"contract_version":"1.0.0","bundle_version":"v7","aoi_id":"x",
                "data_origin":"simulated","generated_at":"2026-11-14T17:00:00Z",
                "core_commit":"abc","files":[{"path":"a","sha256":"00"}]}"#,
        )
        .unwrap();

        let err = refuse_if_published(dir.path()).unwrap_err();
        match err {
            CliError::BundleExists { existing, .. } => assert_eq!(existing, "v7"),
            other => panic!("se esperaba BundleExists, llegó {other}"),
        }
    }

    #[test]
    fn an_unreadable_index_still_blocks_the_write() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.json"), "{ roto").unwrap();

        let err = refuse_if_published(dir.path()).unwrap_err();
        match err {
            CliError::BundleExists { existing, .. } => assert_eq!(existing, "ilegible"),
            other => panic!("se esperaba BundleExists, llegó {other}"),
        }
    }

    #[test]
    fn entries_use_forward_slashes_and_skip_the_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("wonb/risk")).unwrap();
        std::fs::write(dir.path().join("index.json"), "{}").unwrap();
        std::fs::write(dir.path().join("manifest.json"), "a").unwrap();
        std::fs::write(dir.path().join("wonb/risk/c1.json"), "b").unwrap();

        let entries = index_entries(dir.path()).unwrap();
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["manifest.json", "wonb/risk/c1.json"]);
        assert_eq!(entries[0].sha256, sha256_bytes(b"a"));
    }

    #[test]
    fn sha256_matches_a_known_value() {
        // sha256("") — el vector de prueba más citado que existe.
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
