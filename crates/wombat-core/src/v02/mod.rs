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
//! Camino normal de uso:
//!
//! ```no_run
//! use wombat_core::v02::{Block, TrustedIssuers};
//!
//! let block = Block::from_json(&std::fs::read_to_string("wonb/change.json")?)?;
//! let trusted = TrustedIssuers::from_json(&std::fs::read_to_string("trusted-issuers.json")?)?;
//! block.verify(Some("2026-11-14T17:00:00Z"), &trusted)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! **R14: solo Rust firma.** Este módulo puede firmar ([`Block::seal`]), pero no
//! escribe ni lee claves privadas en disco: la clave se pasa en memoria y en los
//! tests se genera al vuelo.

pub mod chain;
pub mod envelope;
pub mod error;
pub mod payload;
pub mod sign;
pub mod trusted;

pub use chain::verify_chain;
pub use envelope::{Block, BlockBody, BlockType, Provenance, Source, WONB_VERSION};
pub use error::WonbError;
pub use payload::{
    ActionKind, ActionPayload, ChangePayload, ComponentsBp, ContextPayload, DataOrigin, Frequency,
    OperaStatus, OverlayHashes, Payload, RiskMetrics, RiskPayload, RiskStatus,
};
pub use sign::{generate_ephemeral_keypair, issuer_string, valid_until_from};
pub use trusted::{TrustedIssuer, TrustedIssuers};
