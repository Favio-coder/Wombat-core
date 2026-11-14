//! Emisores de confianza (`trusted-issuers.json` del bundle).
//!
//! Una firma válida solo prueba que *alguien* con una clave firmó el bloque. La
//! lista de emisores es la que dice si ese alguien tiene permiso, y para qué
//! tipo de bloque: un emisor habilitado para `context` no puede emitir `risk`.

use serde::{Deserialize, Serialize};

use super::envelope::BlockType;
use super::error::WonbError;

/// Un emisor habilitado y los tipos de bloque que puede firmar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedIssuer {
    /// Clave pública en la forma `ed25519:<64 hex>`.
    pub issuer: String,
    pub name: String,
    pub roles: Vec<BlockType>,
}

/// Lista de emisores de confianza de un bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedIssuers {
    pub version: u32,
    pub issuers: Vec<TrustedIssuer>,
}

impl TrustedIssuers {
    /// Lee la lista desde el JSON del bundle.
    pub fn from_json(s: &str) -> Result<Self, WonbError> {
        Ok(serde_json::from_str(s)?)
    }

    /// Lista vacía: nada es de confianza. Útil para tests de rechazo.
    pub fn empty() -> Self {
        Self {
            version: 1,
            issuers: Vec::new(),
        }
    }

    /// Busca un emisor por su clave pública.
    pub fn get(&self, issuer: &str) -> Option<&TrustedIssuer> {
        self.issuers.iter().find(|i| i.issuer == issuer)
    }

    /// Si ese emisor puede firmar ese tipo de bloque.
    pub fn allows(&self, issuer: &str, block_type: BlockType) -> bool {
        self.get(issuer)
            .is_some_and(|i| i.roles.contains(&block_type))
    }
}
