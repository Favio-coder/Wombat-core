//! Payloads de los cuatro tipos de bloque WONB 0.2.
//!
//! Réplica exacta de `contracts/v1/wonb-payloads.schema.json`. Dos reglas que no
//! se negocian:
//!
//! - **Cero floats (R16).** Las fracciones son puntos básicos (`_bp`, 0–10000),
//!   las distancias metros enteros (`_m`) y los decibelios centésimas (`_cdb`).
//!   Un `f64` en estos structs sería un error de diseño: Rust y JavaScript
//!   serializan floats distinto y la verificación cruzada fallaría.
//! - **Las fechas son `String`.** Reparsear con `chrono` y volver a formatear
//!   cambia los bytes que se firmaron (`2026-10-14T23:00:00Z` vs
//!   `2026-10-14T23:00:00+00:00` son el mismo instante y distinto hash).
//!
//! `deny_unknown_fields` replica el `additionalProperties: false` del esquema.
//! Los `Option` se serializan como `null` explícito: omitirlos cambiaría el JCS.

use serde::{Deserialize, Serialize};

/// Payload de un bloque `change`: comparación de dos pasadas SAR sobre un AOI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePayload {
    pub aoi_id: String,
    pub data_origin: DataOrigin,
    pub t1: String,
    pub t2: String,
    pub polarization: String,
    pub frequency: Frequency,
    pub pixel_spacing_m: u32,
    // DECISIÓN: el prompt describe los centi-dB como i32, pero el esquema fija
    // `minimum: 1` para este umbral y `minimum: 0` para `p90_abs_delta_cdb`
    // (es un valor absoluto). Usamos u32 porque rechaza más: un negativo que el
    // esquema prohíbe no llega a construirse. Un delta con signo, si algún día
    // entra al contrato, sí será i32.
    pub delta_db_threshold_cdb: u32,
    pub aoi_valid_bp: u32,
    pub aoi_decrease_bp: u32,
    pub aoi_increase_bp: u32,
    pub overlay_sha256: OverlayHashes,
    pub cells_geojson_sha256: String,
}

/// Origen de los datos (R1): nunca se presenta una simulación como real.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataOrigin {
    NisarReal,
    Simulated,
}

/// Banda NISAR usada en la comparación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frequency {
    A,
    B,
}

/// sha256 de los tres PNG de overlay publicados en el bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayHashes {
    #[serde(rename = "t1.png")]
    pub t1_png: String,
    #[serde(rename = "t2.png")]
    pub t2_png: String,
    #[serde(rename = "delta.png")]
    pub delta_png: String,
}

/// Payload de un bloque `context`: red viaria OSM del AOI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPayload {
    pub aoi_id: String,
    pub roads_total_m: u32,
    pub road_buffer_m: u32,
    pub osm_retrieved_at: String,
    pub roads_geojson_sha256: String,
}

/// Payload de un bloque `risk`: una celda puntuada.
///
/// Con `status: insufficient_quality`, `rank`, `score_bp` y `components_bp` son
/// `null`: la celda existe en el mapa para poder decir "sin datos suficientes",
/// pero no entra al ranking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskPayload {
    pub cell_id: String,
    pub status: RiskStatus,
    pub rank: Option<u32>,
    pub score_bp: Option<u32>,
    pub components_bp: Option<ComponentsBp>,
    pub metrics: RiskMetrics,
    pub explanation: Vec<String>,
}

/// Veredicto de una celda. Nunca afirma un deslizamiento: pide verificación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    ReviewHigh,
    ReviewMedium,
    ReviewLow,
    InsufficientQuality,
}

/// Aporte de cada componente al score, en puntos básicos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentsBp {
    pub change: u32,
    pub opera: u32,
    pub road: u32,
}

/// Métricas crudas de la celda, antes de puntuar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskMetrics {
    pub valid_bp: u32,
    pub decrease_bp: u32,
    pub increase_bp: u32,
    pub changed_bp: u32,
    pub p90_abs_delta_cdb: Option<u32>,
    pub opera_status: OperaStatus,
    pub opera_overlap_bp: Option<u32>,
    pub road_m_near_change: u32,
    pub road_m_total: u32,
}

/// Si una fuente independiente (OPERA) corrobora el cambio detectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperaStatus {
    Corroborated,
    NotCorroborated,
    Unavailable,
}

/// Payload de un bloque `action`: qué se le pide a una persona.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPayload {
    pub cell_id: String,
    pub rank: u32,
    pub action: ActionKind,
    pub audience: String,
}

/// Única acción del contrato v1: verificar en campo antes de cualquier otra cosa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    FieldVerificationFirst,
}

/// Payload tipado de un bloque, elegido por su `type`.
///
/// Se serializa sin etiqueta: el `type` ya vive en el envelope, y añadir otra
/// cambiaría los bytes firmados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Payload {
    Change(ChangePayload),
    Context(ContextPayload),
    Risk(RiskPayload),
    Action(ActionPayload),
}
