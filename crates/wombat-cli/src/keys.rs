//! Claves Ed25519 en disco.
//!
//! `wombat-core` firma pero deliberadamente no sabe leer ni escribir claves
//! (ver `v02::mod`): la clave entra en memoria. Ese I/O vive acá, en el único
//! binario que lo necesita, con la guarda de R19 delante.
//!
//! Formato del archivo: los 32 bytes de la semilla en hexadecimal, una línea.
//! DECISIÓN: hex y no binario crudo ni PEM. Es lo que ya usa el resto del
//! contrato para claves y firmas (`ed25519:<hex>`, `signature: <hex>`), se
//! puede inspeccionar sin herramientas y sobrevive a un copiar y pegar.

use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};

use crate::error::{self, CliError};

/// Los 32 bytes de una semilla Ed25519.
const SEED_LEN: usize = 32;

/// Genera un par nuevo y escribe el secreto en `path`.
///
/// Rechaza antes de generar nada si `path` cae dentro de un repositorio git
/// (R19), para no dejar una clave escrita y después avisar.
pub fn generate_to_file(path: &Path) -> Result<VerifyingKey, CliError> {
    crate::gitguard::refuse_inside_git_repo(path)?;

    let (signing, verifying) = wombat_core::v02::generate_ephemeral_keypair();
    write_secret(path, &signing)?;
    Ok(verifying)
}

/// Lee una clave de firma desde disco.
pub fn load_signing_key(path: &Path) -> Result<SigningKey, CliError> {
    let raw = error::read_to_string(path)?;
    let hex_seed = raw.trim();
    let bad = |reason: &str| CliError::BadKeyFile {
        path: path.display().to_string(),
        reason: reason.to_string(),
    };

    let bytes = hex::decode(hex_seed).map_err(|_| bad("no es hexadecimal"))?;
    let seed: [u8; SEED_LEN] = bytes
        .try_into()
        .map_err(|_| bad("se esperaban 32 bytes de semilla en hexadecimal (64 caracteres)"))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Escribe la semilla con los permisos más restrictivos que ofrezca el sistema.
fn write_secret(path: &Path, key: &SigningKey) -> Result<(), CliError> {
    let contents = format!("{}\n", hex::encode(key.to_bytes()));
    error::write(path, &contents)?;
    restrict_permissions(path)
}

/// En Unix, 0600. En Windows no hay equivalente honesto: ver la nota de
/// [`permissions_note`].
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|source| {
        CliError::Write {
            path: path.display().to_string(),
            source,
        }
    })
}

#[cfg(windows)]
fn restrict_permissions(path: &Path) -> Result<(), CliError> {
    restrict_key_file_windows(path)
}

/// Only compiled on non-unix, non-windows (e.g. wasm, unsupported targets).
#[cfg(not(any(unix, windows)))]
fn restrict_permissions(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

/// Strips inherited ACEs and grants full control to the current user only.
///
/// Uses `icacls` (ships with every Windows since Vista) rather than the
/// win32 security API — no extra crates, still correct.
///
/// # Errors
/// Returns `CliError::Write` if `USERNAME` env var is unset, if `icacls` is
/// not found or exits non-zero. In all error paths the key file is deleted
/// before returning so no accessible private key is left behind.
#[cfg(windows)]
fn restrict_key_file_windows(path: &std::path::Path) -> Result<(), CliError> {
    let path_str = path.display().to_string();

    let make_err = |reason: String| CliError::Write {
        path: path_str.clone(),
        source: std::io::Error::other(reason),
    };

    // Helper: delete the key file, then propagate the original error.
    let delete_and_fail = |err: CliError| -> CliError {
        let _ = std::fs::remove_file(path);
        err
    };

    let username = std::env::var("USERNAME").map_err(|_| {
        delete_and_fail(make_err(
            "USERNAME env var not set; cannot restrict key-file ACL".to_string(),
        ))
    })?;

    // icacls <path> /inheritance:r /grant:r <user>:F
    // /inheritance:r  — remove all inherited ACEs
    // /grant:r        — replace (not add) an explicit grant
    // <user>:F        — full control
    let output = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{username}:F"))
        .output()
        .map_err(|e| delete_and_fail(make_err(format!("could not run icacls: {e}"))))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(delete_and_fail(make_err(format!(
            "icacls failed (exit {:?}): {stdout}{stderr}",
            output.status.code()
        ))));
    }

    Ok(())
}

/// Lo que hay que decirle a quien acaba de generar una clave, según el sistema.
pub fn permissions_note() -> &'static str {
    if cfg!(unix) {
        "permisos 0600 (solo tu usuario)"
    } else {
        "permisos de archivo restringidos (icacls: solo tu usuario)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wombat_core::v02::issuer_string;

    #[test]
    fn a_generated_key_can_be_read_back_and_signs_as_the_same_issuer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.key");

        let verifying = generate_to_file(&path).unwrap();
        let signing = load_signing_key(&path).unwrap();

        assert_eq!(
            issuer_string(&signing.verifying_key()),
            issuer_string(&verifying),
            "la clave leída debe ser el mismo emisor que la generada"
        );
    }

    #[test]
    fn the_file_holds_a_64_character_hex_seed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.key");
        generate_to_file(&path).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.trim().len(), SEED_LEN * 2);
        assert!(contents.trim().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[cfg(unix)]
    #[test]
    fn the_secret_is_not_readable_by_others() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.key");
        generate_to_file(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "modo {mode:o} deja permisos a otros");
    }

    #[test]
    fn a_file_that_is_not_hex_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roto.key");
        std::fs::write(&path, "no soy una clave\n").unwrap();

        let err = load_signing_key(&path).unwrap_err();
        assert!(matches!(err, CliError::BadKeyFile { .. }), "{err}");
    }

    #[test]
    fn a_hex_file_of_the_wrong_length_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corta.key");
        std::fs::write(&path, "aabbcc\n").unwrap();

        let err = load_signing_key(&path).unwrap_err();
        match err {
            CliError::BadKeyFile { reason, .. } => assert!(reason.contains("32 bytes"), "{reason}"),
            other => panic!("se esperaba BadKeyFile, llegó {other}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_key_file_is_restricted_to_current_user() {
        // Generate a key into a temp file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("win-test.key");
        generate_to_file(&path).unwrap();

        // Run icacls to inspect the resulting ACL.
        let output = std::process::Command::new("icacls")
            .arg(&path)
            .output()
            .expect("icacls must be available on Windows");

        assert!(output.status.success(), "icacls inspection failed");

        let acl_text = String::from_utf8_lossy(&output.stdout);

        // The ACL output must NOT contain "Everyone", "BUILTIN\\Users",
        // or "Authenticated Users" — only the current user (or SYSTEM/Administrators
        // which are acceptable on a dev machine but not wider groups).
        for forbidden in &[
            "Everyone",
            "BUILTIN\\\\Users",
            "NT AUTHORITY\\\\Authenticated",
        ] {
            assert!(
                !acl_text.contains(forbidden),
                "Key file ACL should not contain {forbidden}:\n{acl_text}"
            );
        }

        // The USERNAME must appear in the ACL.
        let username = std::env::var("USERNAME").expect("USERNAME must be set on Windows");
        assert!(
            acl_text.contains(&username),
            "Expected USERNAME '{username}' in ACL:\n{acl_text}"
        );
    }
}
