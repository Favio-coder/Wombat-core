//! # wombat-core
//!
//! Núcleo del protocolo WOMBAT. Define el modelo de datos WONB (Wombat Observation
//! Notation Block), su serialización JSON, validación de campos requeridos,
//! cálculo de hash SHA-256 canónico, firmado/verificación con Ed25519 y
//! detección de duplicados en memoria.
//!
//! ## Dos contratos a la vez
//!
//! - [`v02`] implementa **WONB 0.2**, el contrato de `contracts/v1/` (plan
//!   maestro v2). Es el que se usa para todo lo nuevo.
//! - [`wonb`], [`crypto`] y [`validation`] son **WONB 0.1 (legado)**: el diseño
//!   previo de anomalías/deslizamientos que todavía consume `wombat-node` y que
//!   `ci.yml` sigue validando. No se borran hasta que esa migración ocurra, pero
//!   no deben usarse en código nuevo: su canonicalización no es JCS y su
//!   `confidence: f64` viola R16.

pub mod crypto;
pub mod error;
pub mod v02;
pub mod validation;
pub mod wonb;

// Re-export commonly used types.
pub use crypto::sign_wonb;
pub use error::WonbError;
pub use validation::{validate_wonb, ExpiryPolicy, WonbValidator};
pub use wonb::{Wonb, WonbProvenance, WonbRouting, WonbSource, WonbType};
