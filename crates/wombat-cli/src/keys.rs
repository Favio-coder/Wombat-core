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

/// En Windows, equivalente a 0600: elimina la herencia de permisos y otorga
/// acceso total exclusivamente al usuario actual, usando `icacls`.
///
/// IMPLEMENTACIÓN (prototipo): se invoca `icacls` via `std::process::Command`.
/// Es el camino más portable sin añadir crates adicionales (windows-acl, winapi).
/// Para producción debería usarse `SetNamedSecurityInfoW` directamente.
///
/// El comando es:
///   icacls <ruta> /inheritance:r /grant:r <USERNAME>:F
///
/// `/inheritance:r` elimina todas las ACEs heredadas del directorio padre.
/// `/grant:r` reemplaza los permisos del usuario (en lugar de agregar).
/// `:F` = Full Control solo para ese usuario.
///
/// Si `icacls` no está disponible o falla, el CLI emite un aviso pero no
/// aborta: el archivo ya fue escrito y la clave sigue siendo funcional.
#[cfg(target_os = "windows")]
fn restrict_permissions(path: &Path) -> Result<(), CliError> {
    // Resolvemos a una ruta absoluta para pasarla a icacls sin ambigüedad.
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

    // Obtenemos el nombre de usuario del entorno; fallamos silenciosamente si
    // no está disponible (no queremos abortar una generación de clave por esto).
    let username = std::env::var("USERNAME").unwrap_or_else(|_| std::env::var("USER").unwrap_or_default());
    if username.is_empty() {
        eprintln!(
            "ATENCIÓN: no se pudo determinar el usuario actual; \
             los permisos del archivo de clave no fueron endurecidos."
        );
        return Ok(());
    }

    // icacls <ruta> /inheritance:r /grant:r <user>:F
    let status = std::process::Command::new("icacls")
        .arg(&abs)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{username}:F"))
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!(
                "ATENCIÓN: icacls terminó con código {código}; \
                 los permisos del archivo de clave pueden no estar endurecidos. \
                 Ejecutá manualmente: icacls \"{ruta}\" /inheritance:r /grant:r {usuario}:F",
                código = s.code().unwrap_or(-1),
                ruta = abs.display(),
                usuario = username,
            );
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "ATENCIÓN: no se pudo ejecutar icacls ({e}); \
                 los permisos del archivo de clave no fueron endurecidos. \
                 Ejecutá manualmente: icacls \"{ruta}\" /inheritance:r /grant:r {usuario}:F",
                ruta = abs.display(),
                usuario = username,
            );
            Ok(())
        }
    }
}

/// En sistemas que no son Unix ni Windows (ej. WASI, bare-metal), no hacemos nada.
#[cfg(all(not(unix), not(target_os = "windows")))]
fn restrict_permissions(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

/// Lo que hay que decirle a quien acaba de generar una clave, según el sistema.
pub fn permissions_note() -> &'static str {
    if cfg!(unix) {
        "permisos 0600 (solo tu usuario)"
    } else if cfg!(target_os = "windows") {
        "permisos endurecidos via icacls (herencia eliminada, acceso solo para tu usuario). \
         Guardalo fuera de carpetas sincronizadas (OneDrive, Dropbox) y fuera de cualquier repo"
    } else {
        "ATENCIÓN: en este sistema no se aplicaron permisos restrictivos. \
         Protegé manualmente el archivo de clave."
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
}
