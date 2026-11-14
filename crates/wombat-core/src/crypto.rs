//! Criptografía para WONBs: hash SHA-256 canónico y firmas Ed25519.
//!
//! NOTA DE SEGURIDAD: este módulo NO genera ni almacena claves privadas
//! reales. Las claves se inyectan desde el entorno o se generan efímeras para
//! tests y demo local. Nunca incluir semillas o claves privadas en el repositorio.

use crate::error::{WonbError, WonbError::*};
use crate::wonb::Wonb;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;

/// Calcula el hash SHA-256 de la representación canónica JSON de un WONB.
///
/// El hash se devuelve con el prefijo `sha256:`.
pub fn compute_content_hash(wonb: &Wonb) -> Result<String, WonbError> {
    let canonical = wonb
        .to_canonical_json()
        .map_err(|e| Crypto(format!("serialización canónica falló: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let result = hasher.finalize();
    Ok(format!("sha256:{hex(result)}"))
}

/// Verifica que el `data_hash` del WONB coincida con el hash calculado de su
/// contenido canónico.
pub fn verify_content_hash(wonb: &Wonb) -> Result<(), WonbError> {
    let expected = compute_content_hash(wonb)?;
    let got = &wonb.provenance.data_hash;
    if got != &expected {
        return Err(HashMismatch {
            expected,
            got: got.clone(),
        });
    }
    Ok(())
}

/// Firma un WONB con una clave Ed25519 y actualiza el campo `signature`.
///
/// La firma se calcula sobre el hash canónico del contenido y se codifica en
/// hexadecimal.
pub fn sign_wonb(wonb: &mut Wonb, signing_key: &SigningKey) -> Result<(), WonbError> {
    let hash = compute_content_hash(wonb)?;
    // Firmamos los bytes del hash (sin el prefijo "sha256:" para separar
    // dominio de aplicación del identificador de algoritmo).
    let hash_bytes = hash
        .strip_prefix("sha256:")
        .ok_or_else(|| Crypto("hash sin prefijo sha256:".into()))?;
    let hash_decoded = hex::decode(hash_bytes).map_err(|e| Crypto(e.to_string()))?;
    let signature = signing_key.sign(&hash_decoded);
    wonb.signature = hex::encode(signature.to_bytes());
    Ok(())
}

/// Verifica la firma de un WONB usando una clave pública Ed25519.
///
/// Devuelve `Ok(())` si la firma es válida, `Err(InvalidSignature)` en caso
/// contrario.
pub fn verify_wonb_signature(wonb: &Wonb, verifying_key: &VerifyingKey) -> Result<(), WonbError> {
    if wonb.signature.is_empty() {
        return Err(InvalidSignature { id: wonb.id.clone() });
    }
    let hash = compute_content_hash(wonb)?;
    let hash_bytes = hash
        .strip_prefix("sha256:")
        .ok_or_else(|| Crypto("hash sin prefijo sha256:".into()))?;
    let hash_decoded = hex::decode(hash_bytes).map_err(|e| Crypto(e.to_string()))?;
    let signature_bytes = hex::decode(&wonb.signature).map_err(|e| Crypto(e.to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(
        signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| Crypto("longitud de firma inválida".into()))?,
    );
    verifying_key
        .verify(&hash_decoded, &signature)
        .map_err(|_| InvalidSignature { id: wonb.id.clone() })?;
    Ok(())
}

/// Genera un par de claves Ed25519 efímero (solo para tests y demo).
///
/// Esta función requiere la característica `rand-core` de `ed25519-dalek`.
#[cfg(feature = "rand-core")]
pub fn generate_ephemeral_keypair() -> (SigningKey, VerifyingKey) {
    use ed25519_dalek::Signer as _;
    let mut csprng = rand::rngs::OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wonb::tests::*;

    #[test]
    fn content_hash_is_reproducible() {
        let wonb = valid_wonb();
        let h1 = compute_content_hash(&wonb).unwrap();
        let h2 = compute_content_hash(&wonb).unwrap();
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
        assert_eq!(h1.len(), 71); // "sha256:" + 64 hex chars
    }

    #[test]
    fn content_hash_changes_when_payload_changes() {
        let mut wonb_a = valid_wonb();
        wonb_a.payload = serde_json::json!({"a": 1});
        let mut wonb_b = valid_wonb();
        wonb_b.payload = serde_json::json!({"a": 2});
        assert_ne!(
            compute_content_hash(&wonb_a).unwrap(),
            compute_content_hash(&wonb_b).unwrap()
        );
    }

    #[test]
    fn verify_content_hash_matches() {
        let mut wonb = valid_wonb();
        let hash = compute_content_hash(&wonb).unwrap();
        wonb.provenance.data_hash = hash.clone();
        assert!(verify_content_hash(&wonb).is_ok());
    }

    #[test]
    fn verify_content_hash_mismatch_rejected() {
        let mut wonb = valid_wonb();
        wonb.provenance.data_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
        let err = verify_content_hash(&wonb).unwrap_err();
        matches!(err, HashMismatch { .. });
    }

    #[test]
    fn sign_and_verify_signature() {
        let mut wonb = valid_wonb();
        let (signing_key, verifying_key) = generate_ephemeral_keypair();
        assert!(sign_wonb(&mut wonb, &signing_key).is_ok());
        assert!(!wonb.signature.is_empty());
        assert!(verify_wonb_signature(&wonb, &verifying_key).is_ok());
    }

    #[test]
    fn invalid_signature_rejected() {
        let mut wonb = valid_wonb();
        let (_signing_key, verifying_key) = generate_ephemeral_keypair();
        wonb.signature = "deadbeef".into();
        let err = verify_wonb_signature(&wonb, &verifying_key).unwrap_err();
        matches!(err, InvalidSignature { .. });
    }

    #[test]
    fn empty_signature_rejected() {
        let wonb = valid_wonb();
        let (_signing_key, verifying_key) = generate_ephemeral_keypair();
        let err = verify_wonb_signature(&wonb, &verifying_key).unwrap_err();
        matches!(err, InvalidSignature { .. });
    }

    #[test]
    fn signature_invalidates_on_payload_change() {
        let mut wonb = valid_wonb();
        let (signing_key, verifying_key) = generate_ephemeral_keypair();
        sign_wonb(&mut wonb, &signing_key).unwrap();
        wonb.payload = serde_json::json!({"changed": true});
        // Re-firmar con la nueva firma no debería coincidir con la verificación
        // contra la clave original porque el hash cambió; la firma antigua es
        // inválida para el nuevo contenido.
        let err = verify_wonb_signature(&wonb, &verifying_key).unwrap_err();
        matches!(err, InvalidSignature { .. });
    }
}

/// Wrapper para formatear bytes como hex sin importar `hex` directamente en
/// firmas (mantiene `hex` como dependencia declarada).
fn hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}
