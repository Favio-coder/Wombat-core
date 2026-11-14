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

/// DECISIÓN: en Windows el CLI **no** toca los permisos y lo dice.
///
/// El equivalente real de 0600 en NTFS es reescribir la ACL del archivo, que
/// desde Rust puro significa llamar a la API de Windows. Poner el atributo
/// de solo lectura no restringe a nadie: lo único que hace es sugerir que la
/// clave está protegida cuando no lo está. Preferimos el aviso a la ilusión.
#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

/// Lo que hay que decirle a quien acaba de generar una clave, según el sistema.
pub fn permissions_note() -> &'static str {
    if cfg!(unix) {
        "permisos 0600 (solo tu usuario)"
    } else {
        "ATENCIÓN: en Windows este archivo hereda los permisos de su carpeta. \
         Guardalo fuera de carpetas sincronizadas (OneDrive, Dropbox) y fuera de cualquier repo"
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
