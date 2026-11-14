//! # wombat-risk
//!
//! El **único** lugar del sistema donde se calcula el ranking de celdas (R15).
//! Ni el pipeline de Python ni el dashboard puntúan: Python produce evidencia
//! sin firmar, el dashboard consume el bundle publicado, y aquí en medio ocurre
//! el juicio.
//!
//! Tres propiedades que definen el diseño:
//!
//! 1. **Pura y determinista.** [`evaluate`] no mira el reloj, no sortea, no
//!    abre la red ni lee archivos. [`load`] es lo único que toca el disco. Dos
//!    ejecuciones con los mismos bytes dan la misma salida byte a byte, para que
//!    cualquiera pueda recalcular el ranking de un bundle publicado meses atrás.
//! 2. **Aritmética exacta (R16).** El score se calcula con racionales
//!    ([`Exact`]) y se expresa en puntos básicos enteros. No hay `f64` en la
//!    ruta que produce el payload firmado.
//! 3. **La configuración manda.** Ningún umbral, peso ni saturación está
//!    escrito en el código: todos vienen de `risk-config.json`.
//!
pub mod blocks;
pub mod config;
pub mod error;
pub mod evaluate;
pub mod exact;
pub mod explain;
pub mod inputs;
pub mod load;

pub use blocks::{
    build_action_bodies, build_action_payloads, build_risk_bodies, build_risk_payloads,
    BlockContext,
};
pub use config::{RiskConfig, StatusThresholdsBp, TtlSeconds, WeightsBp};
pub use error::RiskError;
pub use evaluate::{evaluate, CellRisk};
pub use exact::Exact;
pub use inputs::{CellInput, Inputs};
