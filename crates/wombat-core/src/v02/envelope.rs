//! Envelope WONB 0.2: identidad por contenido y canonicalización JCS.
//!
//! El `id` de un bloque **es** el hash de su contenido:
//!
//! ```text
//! id = "sha256:" + hex(SHA-256(JCS(cuerpo sin id ni signature)))
//! ```
//!
//! La canonicalización es JCS (RFC 8785), no `serde_json::to_string`. Esa es la
//! diferencia que importa frente a WONB 0.1: JCS ordena las claves y normaliza
//! los números, así que Python, Rust y JavaScript producen los **mismos bytes**
//! y por tanto el mismo hash. Sin eso, tres implementaciones del mismo contrato
//! se rechazan entre sí.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::error::WonbError;
use super::payload::{ActionPayload, ChangePayload, ContextPayload, Payload, RiskPayload};

/// Versión del contrato WONB implementada por este módulo.
pub const WONB_VERSION: &str = "0.2";

/// Tipo de bloque. Los cuatro del plan v2: evidencia, contexto, juicio y pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Change,
    Context,
    Risk,
    Action,
}

impl BlockType {
    /// Vida útil del tipo, en segundos. `None` = no vence.
    ///
    /// Un `change` es una medición de un par de fechas fijas y un `context` una
    /// descarga de OSM fechada: ninguno "caduca", solo envejece. Un `risk`
    /// (24 h) y un `action` (1 h) sí, porque son juicios operativos.
    pub fn ttl_seconds(self) -> Option<u64> {
        match self {
            BlockType::Change | BlockType::Context => None,
            BlockType::Risk => Some(24 * 3600),
            BlockType::Action => Some(3600),
        }
    }

    /// Tipos de padre admisibles: Change/Context → Risk → Action.
    pub fn allowed_parent_types(self) -> &'static [BlockType] {
        match self {
            BlockType::Change | BlockType::Context => &[],
            BlockType::Risk => &[BlockType::Change, BlockType::Context],
            BlockType::Action => &[BlockType::Risk],
        }
    }
}

impl std::fmt::Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BlockType::Change => "change",
            BlockType::Context => "context",
            BlockType::Risk => "risk",
            BlockType::Action => "action",
        };
        f.write_str(s)
    }
}

/// Una fuente de datos citada en la proveniencia.
///
/// El esquema solo exige `product` y **permite** claves extra (`granule`,
/// `maturity`, `derived_from`, `retrieved_at`…). Las conservamos en `extra`
/// porque descartarlas cambiaría el JCS y rompería el hash de un bloque ajeno.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub product: String,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}

/// De dónde salió el bloque: fuentes, método y commit del código.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub sources: Vec<Source>,
    pub method: String,
    pub code_commit: String,
}

/// Cuerpo firmado de un bloque: todo menos `id` y `signature`.
///
/// Es lo que entra al JCS. Que sea un tipo aparte no es decoración: hace
/// imposible incluir por descuido el `id` o la firma en su propio hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawBody")]
pub struct BlockBody {
    pub wonb_version: String,
    #[serde(rename = "type")]
    pub block_type: BlockType,
    pub topic: String,
    pub issued_at: String,
    pub valid_until: Option<String>,
    pub issuer: String,
    pub parents: Vec<String>,
    pub payload: Payload,
    pub provenance: Provenance,
}

/// Envelope tal como llega del JSON, con el payload sin tipar todavía.
///
/// El payload depende de `type`, así que se deserializa en dos pasos: primero
/// el envelope, después el payload contra el struct que le corresponde.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBody {
    wonb_version: String,
    #[serde(rename = "type")]
    block_type: BlockType,
    topic: String,
    issued_at: String,
    valid_until: Option<String>,
    issuer: String,
    parents: Vec<String>,
    payload: Value,
    provenance: Provenance,
}

impl TryFrom<RawBody> for BlockBody {
    type Error = WonbError;

    fn try_from(raw: RawBody) -> Result<Self, Self::Error> {
        let payload = match raw.block_type {
            BlockType::Change => Payload::Change(from_payload::<ChangePayload>(raw.payload)?),
            BlockType::Context => Payload::Context(from_payload::<ContextPayload>(raw.payload)?),
            BlockType::Risk => Payload::Risk(from_payload::<RiskPayload>(raw.payload)?),
            BlockType::Action => Payload::Action(from_payload::<ActionPayload>(raw.payload)?),
        };
        Ok(BlockBody {
            wonb_version: raw.wonb_version,
            block_type: raw.block_type,
            topic: raw.topic,
            issued_at: raw.issued_at,
            valid_until: raw.valid_until,
            issuer: raw.issuer,
            parents: raw.parents,
            payload,
            provenance: raw.provenance,
        })
    }
}

fn from_payload<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, WonbError> {
    serde_json::from_value(v)
        .map_err(|e| WonbError::Schema(format!("payload fuera de contrato: {e}")))
}

impl BlockBody {
    /// Bytes canónicos JCS (RFC 8785) del cuerpo.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, WonbError> {
        serde_jcs::to_vec(self)
            .map_err(|e| WonbError::Schema(format!("canonicalización JCS falló: {e}")))
    }

    /// Los 32 bytes de SHA-256 sobre el JCS del cuerpo. **Esto es lo que se firma.**
    pub fn digest(&self) -> Result<[u8; 32], WonbError> {
        Ok(Sha256::digest(self.canonical_bytes()?).into())
    }

    /// El `id` que le corresponde a este cuerpo.
    pub fn compute_id(&self) -> Result<String, WonbError> {
        Ok(format!("sha256:{}", hex::encode(self.digest()?)))
    }
}

/// Un bloque WONB 0.2 completo: cuerpo firmado, su `id` y la firma.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Value")]
pub struct Block {
    pub id: String,
    pub signature: String,
    #[serde(flatten)]
    pub body: BlockBody,
}

impl TryFrom<Value> for Block {
    type Error = WonbError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Block::from_value(v)
    }
}

impl Block {
    /// Lee un bloque desde JSON, rechazando floats antes de tipar nada (R16).
    ///
    /// El orden importa: los structs del payload solo tienen enteros, así que un
    /// `0.5` fallaría como error de esquema y perderíamos la razón real. El
    /// chequeo de floats va primero para que el error diga lo que pasa.
    pub fn from_json(s: &str) -> Result<Self, WonbError> {
        Block::from_value(serde_json::from_str(s)?)
    }

    /// Igual que [`Block::from_json`], partiendo de un `Value` ya parseado.
    pub fn from_value(v: Value) -> Result<Self, WonbError> {
        reject_floats(&v, "")?;
        let mut obj = match v {
            Value::Object(map) => map,
            other => {
                return Err(WonbError::Schema(format!(
                    "se esperaba un objeto, llegó {}",
                    kind_of(&other)
                )))
            }
        };
        let id = take_string(&mut obj, "id")?;
        let signature = take_string(&mut obj, "signature")?;
        // `obj` ya no tiene id ni signature: es exactamente el cuerpo firmado.
        // Aquí el JSON ya está parseado: cualquier fallo es de forma, no de
        // sintaxis, así que es Schema y no InvalidJson.
        let body: BlockBody = serde_json::from_value(Value::Object(obj))
            .map_err(|e| WonbError::Schema(e.to_string()))?;
        Ok(Block {
            id,
            signature,
            body,
        })
    }

    /// Serializa el bloque a JSON canónico JCS (para publicarlo o rehashearlo).
    pub fn to_canonical_json(&self) -> Result<String, WonbError> {
        let bytes = serde_jcs::to_vec(self)
            .map_err(|e| WonbError::Schema(format!("canonicalización JCS falló: {e}")))?;
        String::from_utf8(bytes).map_err(|e| WonbError::Schema(e.to_string()))
    }

    /// Comprueba que el `id` declarado sea el hash real del cuerpo.
    ///
    /// Devuelve el digest para que quien verifica la firma no lo recalcule.
    pub fn check_id(&self) -> Result<[u8; 32], WonbError> {
        let digest = self.body.digest()?;
        let expected = format!("sha256:{}", hex::encode(digest));
        if expected != self.id {
            return Err(WonbError::IdMismatch {
                expected,
                got: self.id.clone(),
            });
        }
        Ok(digest)
    }
}

fn take_string(obj: &mut serde_json::Map<String, Value>, key: &str) -> Result<String, WonbError> {
    match obj.remove(key) {
        Some(Value::String(s)) => Ok(s),
        Some(other) => Err(WonbError::Schema(format!(
            "`{key}` debe ser string, llegó {}",
            kind_of(&other)
        ))),
        None => Err(WonbError::Schema(format!("falta `{key}`"))),
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Rechaza cualquier float dentro de un bloque firmado (R16).
///
/// Se aplica al bloque entero, no solo al `payload`: `verify_bundle.py` hace lo
/// mismo, y un float en `provenance` rompería la verificación cruzada igual.
pub fn reject_floats(v: &Value, path: &str) -> Result<(), WonbError> {
    match v {
        Value::Number(n) if n.as_i64().is_none() && n.as_u64().is_none() => {
            Err(WonbError::FloatInPayload {
                path: if path.is_empty() { "<raíz>" } else { path }.to_string(),
            })
        }
        Value::Object(map) => map
            .iter()
            .try_for_each(|(k, child)| reject_floats(child, &join_path(path, k))),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(i, child)| reject_floats(child, &join_path(path, &i.to_string()))),
        _ => Ok(()),
    }
}

fn join_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un bloque `context` mínimo y bien formado, en JSON.
    ///
    /// Los valores no son los del fixture: aquí solo se prueban hash y forma.
    pub(crate) fn context_json() -> Value {
        serde_json::json!({
            "wonb_version": "0.2",
            "id": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "type": "context",
            "topic": "wombat/pe/piura/piura-demo/context",
            "issued_at": "2026-11-14T17:00:00Z",
            "valid_until": null,
            "issuer": "ed25519:92b8169774b5f100de3f37b5ec63b9a10100ae6d1983c4bbdc9b9e7ca6f014be",
            "parents": [],
            "payload": {
                "aoi_id": "piura-demo",
                "roads_total_m": 16242,
                "road_buffer_m": 250,
                "osm_retrieved_at": "2026-11-14T15:00:00Z",
                "roads_geojson_sha256": "5c8e7345000a9b7b35c08714f15b7e7731ee26ce3f08cebd8f5ded1be0d735d0"
            },
            "provenance": {
                "sources": [{"product": "OpenStreetMap", "retrieved_at": "2026-11-14"}],
                "method": "risk-config/0.1",
                "code_commit": "fixture"
            },
            "signature": "00"
        })
    }

    #[test]
    fn jcs_sorts_keys_regardless_of_input_order() {
        // El mismo cuerpo escrito en otro orden debe dar el mismo hash. Esto es
        // precisamente lo que serde_json::to_string (WONB 0.1) NO garantiza.
        let a = Block::from_value(context_json()).unwrap();
        let mut raw = context_json();
        let obj = raw.as_object_mut().unwrap();
        let payload = obj.remove("payload").unwrap();
        obj.insert("payload".into(), payload); // reinsertado al final
        let b = Block::from_value(raw).unwrap();
        assert_eq!(a.body.compute_id().unwrap(), b.body.compute_id().unwrap());
    }

    #[test]
    fn canonical_bytes_have_no_whitespace_and_sorted_keys() {
        let block = Block::from_value(context_json()).unwrap();
        let bytes = block.body.canonical_bytes().unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(!s.contains(' '), "JCS no lleva espacios: {s}");
        assert!(s.starts_with(r#"{"issued_at":"#), "claves sin ordenar: {s}");
    }

    #[test]
    fn id_is_stable_across_runs() {
        let block = Block::from_value(context_json()).unwrap();
        assert_eq!(
            block.body.compute_id().unwrap(),
            block.body.compute_id().unwrap()
        );
        assert!(block.body.compute_id().unwrap().starts_with("sha256:"));
        assert_eq!(block.body.compute_id().unwrap().len(), 71);
    }

    #[test]
    fn id_changes_when_a_single_byte_changes() {
        let a = Block::from_value(context_json()).unwrap();
        let mut raw = context_json();
        raw["payload"]["roads_total_m"] = serde_json::json!(16243);
        let b = Block::from_value(raw).unwrap();
        assert_ne!(a.body.compute_id().unwrap(), b.body.compute_id().unwrap());
    }

    #[test]
    fn check_id_rejects_declared_id_that_is_not_the_hash() {
        let block = Block::from_value(context_json()).unwrap();
        let err = block.check_id().unwrap_err();
        assert!(matches!(err, WonbError::IdMismatch { .. }), "{err}");
    }

    #[test]
    fn float_in_payload_rejected() {
        let mut raw = context_json();
        raw["payload"]["roads_total_m"] = serde_json::json!(0.5);
        let err = Block::from_value(raw).unwrap_err();
        match err {
            WonbError::FloatInPayload { path } => assert_eq!(path, "payload.roads_total_m"),
            other => panic!("se esperaba FloatInPayload, llegó {other}"),
        }
    }

    #[test]
    fn float_anywhere_in_the_signed_body_rejected() {
        let mut raw = context_json();
        raw["provenance"]["sources"][0]["confidence"] = serde_json::json!(0.89);
        let err = Block::from_value(raw).unwrap_err();
        match err {
            WonbError::FloatInPayload { path } => {
                assert_eq!(path, "provenance.sources.0.confidence")
            }
            other => panic!("se esperaba FloatInPayload, llegó {other}"),
        }
    }

    #[test]
    fn unknown_payload_field_rejected() {
        let mut raw = context_json();
        raw["payload"]["inventado"] = serde_json::json!("x");
        let err = Block::from_value(raw).unwrap_err();
        assert!(matches!(err, WonbError::Schema(_)), "{err}");
    }

    #[test]
    fn extra_source_fields_survive_the_roundtrip() {
        // Si descartáramos `retrieved_at`, el hash de un bloque ajeno cambiaría.
        let block = Block::from_value(context_json()).unwrap();
        let back: Value = serde_json::to_value(&block).unwrap();
        assert_eq!(
            back["provenance"]["sources"][0]["retrieved_at"],
            serde_json::json!("2026-11-14")
        );
    }

    #[test]
    fn ttl_per_type() {
        assert_eq!(BlockType::Change.ttl_seconds(), None);
        assert_eq!(BlockType::Context.ttl_seconds(), None);
        assert_eq!(BlockType::Risk.ttl_seconds(), Some(86400));
        assert_eq!(BlockType::Action.ttl_seconds(), Some(3600));
    }
}
