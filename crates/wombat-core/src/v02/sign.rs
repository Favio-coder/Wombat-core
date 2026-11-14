//! Firma y verificación de un bloque WONB 0.2.
//!
//! Lo que se firma son **los 32 bytes del hash**, no su texto hexadecimal. Es la
//! misma decisión que toman `verify_bundle.py` y `contracts/v1/tools/verify-rs`:
//! si una implementación firmara el hex y otra los bytes, las firmas no
//! cruzarían aunque el hash coincidiera.
//!
//! **R14: solo Rust firma.** Aquí no se lee ni se escribe ninguna clave privada
//! en disco: [`Block::seal`] recibe la clave ya cargada en memoria.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::envelope::{Block, BlockBody, WONB_VERSION};
use super::error::WonbError;
use super::trusted::TrustedIssuers;

/// Forma `ed25519:<64 hex>` de una clave pública, como la usa el contrato.
pub fn issuer_string(key: &VerifyingKey) -> String {
    format!("ed25519:{}", hex::encode(key.as_bytes()))
}

impl BlockBody {
    /// `valid_until` que le corresponde al tipo, a partir de `issued_at`.
    ///
    /// `None` para `change` y `context`, que no vencen.
    pub fn expected_valid_until(&self) -> Result<Option<String>, WonbError> {
        let Some(ttl) = self.block_type.ttl_seconds() else {
            return Ok(None);
        };
        let issued = parse_rfc3339(&self.issued_at)?;
        let until = issued + chrono::Duration::seconds(ttl as i64);
        // Formato explícito con `Z`: es el que usan los fixtures y el esquema.
        Ok(Some(until.format("%Y-%m-%dT%H:%M:%SZ").to_string()))
    }
}

impl Block {
    /// Calcula el `id` del cuerpo y lo firma con la clave dada.
    ///
    /// Comprueba que `body.issuer` sea esa clave: un cuerpo que declara un
    /// emisor distinto del que firma produciría un bloque que nadie puede
    /// verificar, y es mejor fallar aquí que publicarlo.
    pub fn seal(body: BlockBody, key: &SigningKey) -> Result<Self, WonbError> {
        let expected_issuer = issuer_string(&key.verifying_key());
        if body.issuer != expected_issuer {
            return Err(WonbError::Schema(format!(
                "el cuerpo declara issuer {} pero firma la clave {expected_issuer}",
                body.issuer
            )));
        }
        let digest = body.digest()?;
        let signature = key.sign(&digest);
        Ok(Block {
            id: format!("sha256:{}", hex::encode(digest)),
            signature: hex::encode(signature.to_bytes()),
            body,
        })
    }

    /// Verifica el bloque completo: identidad, emisor, firma, forma y TTL.
    ///
    /// El orden es el de `verify_bundle.py`, para que las tres implementaciones
    /// den el mismo error ante el mismo bloque roto:
    ///
    /// 1. el `id` es el hash real del cuerpo → [`WonbError::IdMismatch`]
    /// 2. el emisor está habilitado para ese tipo → [`WonbError::UntrustedIssuer`]
    /// 3. la firma es válida → [`WonbError::BadSignature`]
    /// 4. la forma coincide con el contrato → [`WonbError::Schema`]
    /// 5. el bloque no venció a `as_of` → [`WonbError::Expired`]
    ///
    /// `as_of` es el instante contra el que se mide el TTL.
    ///
    /// DECISIÓN: con `as_of = None` se usa el `issued_at` del propio bloque. Un
    /// bundle estático es una foto histórica: en los fixtures `issued_at` es el
    /// `generated_at` del índice, que es lo que pide el README en
    /// "TTL en bundles estáticos". Quien verifique en vivo debe pasar la hora
    /// real explícitamente; es la opción conservadora, porque el fallback nunca
    /// declara expirado un bloque que el emisor acababa de emitir.
    pub fn verify(&self, as_of: Option<&str>, trusted: &TrustedIssuers) -> Result<(), WonbError> {
        let digest = self.check_id()?;

        if !trusted.allows(&self.body.issuer, self.body.block_type) {
            return Err(WonbError::UntrustedIssuer {
                issuer: self.body.issuer.clone(),
                block_type: self.body.block_type.to_string(),
            });
        }

        self.check_signature(&digest)?;
        self.check_shape()?;
        self.check_ttl(as_of.unwrap_or(&self.body.issued_at))
    }

    fn check_signature(&self, digest: &[u8; 32]) -> Result<(), WonbError> {
        let bad = || WonbError::BadSignature {
            id: self.id.clone(),
        };
        let key_hex = self.body.issuer.strip_prefix("ed25519:").ok_or_else(|| {
            WonbError::Schema(format!("issuer sin prefijo ed25519: {}", self.body.issuer))
        })?;
        let key_bytes: [u8; 32] = hex::decode(key_hex)
            .map_err(|_| bad())?
            .try_into()
            .map_err(|_| bad())?;
        let sig_bytes: [u8; 64] = hex::decode(&self.signature)
            .map_err(|_| bad())?
            .try_into()
            .map_err(|_| bad())?;
        VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| bad())?
            .verify(digest, &Signature::from_bytes(&sig_bytes))
            .map_err(|_| bad())
    }

    /// Reglas del envelope que no dependen de la criptografía.
    fn check_shape(&self) -> Result<(), WonbError> {
        if self.body.wonb_version != WONB_VERSION {
            return Err(WonbError::Schema(format!(
                "wonb_version {} no soportada (se espera {WONB_VERSION})",
                self.body.wonb_version
            )));
        }
        let suffix = format!("/{}", self.body.block_type);
        if !self.body.topic.starts_with("wombat/pe/") || !self.body.topic.ends_with(&suffix) {
            return Err(WonbError::Schema(format!(
                "topic {} no corresponde a un bloque {}",
                self.body.topic, self.body.block_type
            )));
        }
        // `change` y `context` no vencen; `risk` y `action` tienen que vencer.
        match (self.body.block_type.ttl_seconds(), &self.body.valid_until) {
            (None, Some(v)) => Err(WonbError::Schema(format!(
                "un bloque {} no vence, pero declara valid_until {v}",
                self.body.block_type
            ))),
            (Some(_), None) => Err(WonbError::Schema(format!(
                "un bloque {} debe declarar valid_until",
                self.body.block_type
            ))),
            _ => Ok(()),
        }
    }

    fn check_ttl(&self, as_of: &str) -> Result<(), WonbError> {
        let Some(valid_until) = &self.body.valid_until else {
            return Ok(()); // change y context no vencen.
        };
        let at = parse_rfc3339(as_of)?;
        let issued = parse_rfc3339(&self.body.issued_at)?;
        let until = parse_rfc3339(valid_until)?;
        if at < issued || at >= until {
            return Err(WonbError::Expired {
                id: self.id.clone(),
                valid_until: valid_until.clone(),
                as_of: as_of.to_string(),
            });
        }
        Ok(())
    }
}

/// Parsea una fecha **solo para comparar**.
///
/// Las fechas se guardan como `String` y se vuelven a escribir tal cual: lo que
/// se firmó son esos bytes. `2026-10-14T23:00:00Z` y `2026-10-14T23:00:00+00:00`
/// son el mismo instante y distinto hash, así que chrono entra aquí y no sale.
fn parse_rfc3339(s: &str) -> Result<chrono::DateTime<chrono::FixedOffset>, WonbError> {
    chrono::DateTime::parse_from_rfc3339(s).map_err(|_| WonbError::InvalidTimestamp(s.to_string()))
}

/// Genera un par de claves Ed25519 en memoria, para tests y demos.
///
/// R14/R19: la clave privada nunca se escribe en disco ni en la repo.
pub fn generate_ephemeral_keypair() -> (SigningKey, VerifyingKey) {
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v02::envelope::{tests::context_json, BlockType};
    use crate::v02::trusted::{TrustedIssuer, TrustedIssuers};
    use serde_json::Value;

    /// Construye un bloque firmado por una clave nueva, con el tipo y las
    /// fechas que se le pidan.
    fn sealed(raw: Value) -> (Block, SigningKey, TrustedIssuers) {
        let (key, vk) = generate_ephemeral_keypair();
        let mut raw = raw;
        raw["issuer"] = Value::String(issuer_string(&vk));
        let block = Block::from_value(raw).unwrap();
        let roles = vec![block.body.block_type];
        let trusted = TrustedIssuers {
            version: 1,
            issuers: vec![TrustedIssuer {
                issuer: issuer_string(&vk),
                name: "test en memoria".into(),
                roles,
            }],
        };
        (Block::seal(block.body, &key).unwrap(), key, trusted)
    }

    fn risk_json() -> Value {
        serde_json::json!({
            "wonb_version": "0.2",
            "id": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "type": "risk",
            "topic": "wombat/pe/piura/piura-demo/risk",
            "issued_at": "2026-11-14T17:00:00Z",
            "valid_until": "2026-11-15T17:00:00Z",
            "issuer": "ed25519:92b8169774b5f100de3f37b5ec63b9a10100ae6d1983c4bbdc9b9e7ca6f014be",
            "parents": [
                "sha256:02da13aa3d9cfb4c94ed1512d9728ddb3ed7a945e98c9a3bbd98c9cb791de683",
                "sha256:f5e7bd0f6e3c316fb8c97f15af3bb03cc515a9282ecfd3f1c8be18e7f3cc2fb2"
            ],
            "payload": {
                "cell_id": "piura-demo-r02c01",
                "status": "review_high",
                "rank": 1,
                "score_bp": 7200,
                "components_bp": {"change": 8000, "opera": 10000, "road": 4000},
                "metrics": {
                    "valid_bp": 9500, "decrease_bp": 2200, "increase_bp": 100,
                    "changed_bp": 2300, "p90_abs_delta_cdb": 740,
                    "opera_status": "corroborated", "opera_overlap_bp": 6000,
                    "road_m_near_change": 900, "road_m_total": 1200
                },
                "explanation": ["Backscatter dropped over 22% of the cell."]
            },
            "provenance": {
                "sources": [{"product": "NISAR_L2_GCOV", "granule": "SIMULATED"}],
                "method": "risk-config/0.1",
                "code_commit": "test"
            },
            "signature": "00"
        })
    }

    #[test]
    fn seal_then_verify_roundtrip() {
        let (block, _key, trusted) = sealed(context_json());
        block.verify(None, &trusted).unwrap();
    }

    #[test]
    fn sealed_id_is_stable_across_runs() {
        let (key, vk) = generate_ephemeral_keypair();
        let mut raw = context_json();
        raw["issuer"] = Value::String(issuer_string(&vk));
        let body = Block::from_value(raw).unwrap().body;
        let a = Block::seal(body.clone(), &key).unwrap();
        let b = Block::seal(body, &key).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.signature, b.signature, "Ed25519 es determinista");
    }

    #[test]
    fn seal_rejects_a_body_that_names_another_issuer() {
        let (key, _vk) = generate_ephemeral_keypair();
        let body = Block::from_value(context_json()).unwrap().body;
        let err = Block::seal(body, &key).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }

    #[test]
    fn tampered_payload_fails_as_id_mismatch() {
        let (block, _key, trusted) = sealed(risk_json());
        let mut raw = serde_json::to_value(&block).unwrap();
        raw["payload"]["score_bp"] = serde_json::json!(7201);
        let tampered = Block::from_value(raw).unwrap();
        let err = tampered.verify(None, &trusted).unwrap_err();
        assert!(matches!(err, WonbError::IdMismatch { .. }), "{err}");
    }

    #[test]
    fn tampered_signature_fails_as_bad_signature() {
        let (block, _key, trusted) = sealed(risk_json());
        let mut raw = serde_json::to_value(&block).unwrap();
        // Una firma del tamaño correcto pero que no es la del bloque. El `id`
        // sigue siendo válido: la firma no entra en el cuerpo firmado.
        raw["signature"] = serde_json::json!("00".repeat(64));
        let tampered = Block::from_value(raw).unwrap();
        let err = tampered.verify(None, &trusted).unwrap_err();
        assert!(matches!(err, WonbError::BadSignature { .. }), "{err}");
    }

    #[test]
    fn issuer_outside_the_trusted_list_rejected() {
        let (block, _key, _trusted) = sealed(risk_json());
        let err = block.verify(None, &TrustedIssuers::empty()).unwrap_err();
        assert!(matches!(err, WonbError::UntrustedIssuer { .. }), "{err}");
    }

    #[test]
    fn trusted_issuer_without_the_role_rejected() {
        // Firma válida y emisor conocido, pero habilitado para otro tipo.
        let (block, _key, trusted) = sealed(risk_json());
        let downgraded = TrustedIssuers {
            version: 1,
            issuers: vec![TrustedIssuer {
                roles: vec![BlockType::Context],
                ..trusted.issuers[0].clone()
            }],
        };
        let err = block.verify(None, &downgraded).unwrap_err();
        assert!(matches!(err, WonbError::UntrustedIssuer { .. }), "{err}");
    }

    #[test]
    fn risk_expires_after_24h() {
        let (block, _key, trusted) = sealed(risk_json());
        // A las 23 h todavía vale; a las 25 h, no.
        block
            .verify(Some("2026-11-15T16:00:00Z"), &trusted)
            .unwrap();
        let err = block
            .verify(Some("2026-11-15T18:00:00Z"), &trusted)
            .unwrap_err();
        assert!(matches!(err, WonbError::Expired { .. }), "{err}");
    }

    #[test]
    fn change_and_context_never_expire() {
        let (block, _key, trusted) = sealed(context_json());
        block
            .verify(Some("2099-01-01T00:00:00Z"), &trusted)
            .unwrap();
    }

    #[test]
    fn expected_valid_until_follows_the_type() {
        let body = Block::from_value(risk_json()).unwrap().body;
        assert_eq!(
            body.expected_valid_until().unwrap().as_deref(),
            Some("2026-11-15T17:00:00Z")
        );
        let body = Block::from_value(context_json()).unwrap().body;
        assert_eq!(body.expected_valid_until().unwrap(), None);
    }

    #[test]
    fn risk_without_valid_until_rejected() {
        let mut raw = risk_json();
        raw["valid_until"] = Value::Null;
        let (block, _key, trusted) = sealed(raw);
        let err = block.verify(None, &trusted).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }

    #[test]
    fn wrong_topic_for_the_type_rejected() {
        let mut raw = risk_json();
        raw["topic"] = serde_json::json!("wombat/pe/piura/piura-demo/action");
        let (block, _key, trusted) = sealed(raw);
        let err = block.verify(None, &trusted).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }
}
