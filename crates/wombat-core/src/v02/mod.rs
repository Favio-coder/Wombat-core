//! WONB 0.2: el contrato del plan maestro v2 (`contracts/v1/`).
//!
//! Módulo **aditivo**. WONB 0.1 ([`crate::wonb`], [`crate::crypto`],
//! [`crate::validation`]) sigue vivo y es el que usa `wombat-node`; aquí no se
//! toca. Son dos contratos distintos, no dos versiones del mismo tipo:
//!
//! | | 0.1 (legado) | 0.2 (`v02`) |
//! |---|---|---|
//! | Canonicalización | `serde_json::to_string` | **JCS (RFC 8785)** |
//! | Números del cuerpo firmado | `f64` (`confidence`) | solo enteros (R16) |
//! | Tipos | anomaly/context/risk/tool | change/context/risk/action |
//! | TTL | `ttl_seconds` relativo | `valid_until` absoluto |
//! | Confianza del emisor | clave suelta | `trusted-issuers.json` + roles |
//!
//! **R14: solo Rust firma.** Este módulo puede firmar, pero no escribe ni lee
//! claves privadas en disco: la clave se pasa en memoria y en los tests se
//! genera al vuelo.

pub mod envelope;
pub mod error;
pub mod payload;

pub use envelope::{Block, BlockBody, BlockType, Provenance, Source, WONB_VERSION};
pub use error::WonbError;
pub use payload::{
    ActionKind, ActionPayload, ChangePayload, ComponentsBp, ContextPayload, DataOrigin, Frequency,
    OperaStatus, OverlayHashes, Payload, RiskMetrics, RiskPayload, RiskStatus,
};
