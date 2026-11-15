//! Entradas de la regla de riesgo: las celdas que produce Python.
//!
//! `cells.geojson` es la frontera Python → Rust (R14: Python produce evidencia
//! sin firmar, Rust canonicaliza y firma). Aquí **sí** hay decimales en la
//! entrada: `contracts/v1/cells.schema.json` los permite. Se convierten a
//! racionales exactos al construir [`CellInput`], y de ahí en adelante la
//! aritmética es entera.

use serde::Deserialize;

use crate::error::RiskError;
use crate::exact::Exact;
use wombat_core::v02::OperaStatus;

/// Una celda lista para puntuar, con sus fracciones ya exactas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellInput {
    pub cell_id: String,
    pub valid_fraction: Exact,
    pub decrease_fraction: Exact,
    pub increase_fraction: Exact,
    pub changed_fraction: Exact,
    /// p90 del |delta| en dB. `None` cuando no se pudo calcular.
    pub p90_abs_delta_db: Option<Exact>,
    pub opera_status: OperaStatus,
    /// Solapamiento con OPERA. `None` exactamente cuando el estado es
    /// `unavailable`, como exige el esquema.
    pub opera_overlap_ratio: Option<Exact>,
    pub road_m_near_change: u32,
    pub road_m_total: u32,
}

/// Todas las entradas de una evaluación.
///
/// No hay rutas de archivos aquí: [`crate::load`] es lo único que toca el disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inputs {
    pub aoi_id: String,
    pub cells: Vec<CellInput>,
}

impl Inputs {
    /// Lee las celdas desde el GeoJSON del contrato.
    ///
    /// La geometría se ignora: la regla de riesgo no la usa, y arrastrarla solo
    /// daría ocasión de que alguien calcule algo con ella fuera de este crate.
    pub fn from_cells_geojson(aoi_id: impl Into<String>, s: &str) -> Result<Self, RiskError> {
        let aoi_id = aoi_id.into();
        let fc: FeatureCollection = serde_json::from_str(s)?;
        if fc.aoi_id.as_deref().is_some_and(|id| id != aoi_id) {
            return Err(RiskError::Input(format!(
                "cells.geojson declara aoi_id {:?}, esperado {aoi_id:?}",
                fc.aoi_id
            )));
        }
        let mut cells = Vec::with_capacity(fc.features.len());
        for feature in fc.features {
            cells.push(feature.properties.convert(&aoi_id)?);
        }
        if cells.is_empty() {
            return Err(RiskError::Input("cells.geojson no tiene celdas".into()));
        }
        let mut seen: Vec<&str> = cells.iter().map(|c| c.cell_id.as_str()).collect();
        seen.sort_unstable();
        if let Some(dup) = seen.windows(2).find(|w| w[0] == w[1]) {
            return Err(RiskError::DuplicateCell {
                cell_id: dup[0].to_string(),
            });
        }
        Ok(Inputs { aoi_id, cells })
    }
}

#[derive(Debug, Deserialize)]
struct FeatureCollection {
    #[serde(default)]
    aoi_id: Option<String>,
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
struct Feature {
    properties: RawCell,
}

/// Propiedades de una celda tal como vienen del GeoJSON.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCell {
    #[serde(default)]
    aoi_id: Option<String>,
    cell_id: String,
    valid_fraction: f64,
    decrease_fraction: f64,
    increase_fraction: f64,
    changed_fraction: f64,
    p90_abs_delta_db: Option<f64>,
    opera_status: OperaStatus,
    opera_overlap_ratio: Option<f64>,
    road_m_near_change: u32,
    road_m_total: u32,
    #[serde(default)]
    source_status: Option<CellSourceStatus>,
    #[serde(default)]
    provenance_refs: Option<CellProvenanceRefs>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CellSourceStatus {
    nisar: SourceAvailability,
    opera: SourceAvailability,
    osm: SourceAvailability,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SourceAvailability {
    Available,
    Unavailable,
    Simulated,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CellProvenanceRefs {
    nisar: Vec<String>,
    opera: Vec<String>,
    osm: Vec<String>,
}

impl RawCell {
    fn convert(self, aoi_id: &str) -> Result<CellInput, RiskError> {
        if !self.cell_id.starts_with(&format!("{aoi_id}-r")) {
            return Err(RiskError::Input(format!(
                "{}: cell_id no pertenece a {aoi_id}",
                self.cell_id
            )));
        }
        if self.aoi_id.as_deref().is_some_and(|id| id != aoi_id) {
            return Err(RiskError::Input(format!(
                "{}: aoi_id {:?} no coincide con {aoi_id}",
                self.cell_id, self.aoi_id
            )));
        }
        // El esquema ata el solapamiento al estado: `unavailable` implica null y
        // cualquier otro estado implica un número. Si la entrada se contradice,
        // es mejor rechazarla que inventar un 0 que bajaría el score.
        match (self.opera_status, self.opera_overlap_ratio) {
            (OperaStatus::Unavailable, Some(v)) => {
                return Err(RiskError::Input(format!(
                    "{}: opera_status unavailable con opera_overlap_ratio {v}",
                    self.cell_id
                )))
            }
            (OperaStatus::Corroborated | OperaStatus::NotCorroborated, None) => {
                return Err(RiskError::Input(format!(
                    "{}: opera_status {:?} sin opera_overlap_ratio",
                    self.cell_id, self.opera_status
                )))
            }
            _ => {}
        }
        match (&self.source_status, &self.provenance_refs) {
            (Some(status), Some(refs)) => {
                if status.nisar == SourceAvailability::Unavailable
                    || (status.nisar != SourceAvailability::Simulated && refs.nisar.is_empty())
                    || (status.opera != SourceAvailability::Unavailable && refs.opera.is_empty())
                    || (status.osm != SourceAvailability::Unavailable && refs.osm.is_empty())
                {
                    return Err(RiskError::Input(format!(
                        "{}: disponibilidad de fuentes y referencias de procedencia no coinciden",
                        self.cell_id
                    )));
                }
                if (self.opera_status == OperaStatus::Unavailable)
                    != (status.opera == SourceAvailability::Unavailable)
                {
                    return Err(RiskError::Input(format!(
                        "{}: source_status.opera contradice opera_status",
                        self.cell_id
                    )));
                }
            }
            (None, None) => {}
            _ => {
                return Err(RiskError::Input(format!(
                    "{}: source_status y provenance_refs deben aparecer juntos",
                    self.cell_id
                )))
            }
        }
        Ok(CellInput {
            valid_fraction: fraction(&self.cell_id, "valid_fraction", self.valid_fraction)?,
            decrease_fraction: fraction(
                &self.cell_id,
                "decrease_fraction",
                self.decrease_fraction,
            )?,
            increase_fraction: fraction(
                &self.cell_id,
                "increase_fraction",
                self.increase_fraction,
            )?,
            changed_fraction: fraction(&self.cell_id, "changed_fraction", self.changed_fraction)?,
            p90_abs_delta_db: self
                .p90_abs_delta_db
                .map(Exact::from_f64_repr)
                .transpose()?,
            opera_status: self.opera_status,
            opera_overlap_ratio: self
                .opera_overlap_ratio
                .map(|v| fraction(&self.cell_id, "opera_overlap_ratio", v))
                .transpose()?,
            road_m_near_change: self.road_m_near_change,
            road_m_total: self.road_m_total,
            cell_id: self.cell_id,
        })
    }
}

/// Convierte una fracción de la entrada, comprobando que esté en `0..=1`.
fn fraction(cell_id: &str, field: &str, x: f64) -> Result<Exact, RiskError> {
    let value = Exact::from_f64_repr(x)?;
    if value.cmp_exact(&Exact::ZERO) == std::cmp::Ordering::Less
        || value.cmp_exact(&Exact::ONE) == std::cmp::Ordering::Greater
    {
        return Err(RiskError::Input(format!(
            "{cell_id}: {field} = {x} fuera de 0..=1"
        )));
    }
    Ok(value)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn one_cell_geojson(props: &str) -> String {
        format!(
            r#"{{"type":"FeatureCollection","features":[
                {{"type":"Feature",
                  "geometry":{{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}},
                  "properties":{props}}}]}}"#
        )
    }

    pub(crate) fn props(cell_id: &str, valid: &str, dec: &str, inc: &str) -> String {
        let changed = Exact::from_decimal_str(dec)
            .unwrap()
            .checked_add(Exact::from_decimal_str(inc).unwrap())
            .unwrap()
            .trimmed_decimal()
            .unwrap();
        format!(
            r#"{{"cell_id":"{cell_id}","valid_fraction":{valid},"decrease_fraction":{dec},
                 "increase_fraction":{inc},"changed_fraction":{changed},"p90_abs_delta_db":3.3,
                 "opera_status":"not_corroborated","opera_overlap_ratio":0.05,
                 "road_m_near_change":300,"road_m_total":2100}}"#
        )
    }

    #[test]
    fn fractions_are_read_as_exact_decimals() {
        let json = one_cell_geojson(&props("piura-demo-r00c00", "0.95", "0.02", "0.01"));
        let inputs = Inputs::from_cells_geojson("piura-demo", &json).unwrap();
        let cell = &inputs.cells[0];
        assert_eq!(
            cell.valid_fraction,
            Exact::from_decimal_str("0.95").unwrap()
        );
        assert_eq!(
            cell.changed_fraction,
            Exact::from_decimal_str("0.03").unwrap()
        );
        assert_eq!(cell.road_m_near_change, 300);
    }

    #[test]
    fn integrated_cells_are_accepted_but_a_mixed_aoi_is_rejected() {
        let json = r#"{"type":"FeatureCollection","aoi_id":"tailandia-01","crs_compute":"EPSG:32647",
            "features":[{"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0]]]},
            "properties":{"aoi_id":"tailandia-01","cell_id":"tailandia-01-r00c00",
            "valid_fraction":0.9,"decrease_fraction":0.1,"increase_fraction":0.0,
            "changed_fraction":0.1,"p90_abs_delta_db":4.0,"opera_status":"unavailable",
            "opera_overlap_ratio":null,"road_m_near_change":0,"road_m_total":0,
            "source_status":{"nisar":"available","opera":"unavailable","osm":"available"},
            "provenance_refs":{"nisar":["manifest.json#/nisar"],
            "opera":["manifest.json#/opera"],"osm":["roads.geojson"]}}}]}"#;
        assert!(Inputs::from_cells_geojson("tailandia-01", json).is_ok());

        let mixed = json.replace(
            "\"aoi_id\":\"tailandia-01\",\"cell_id\":\"tailandia-01-r00c00\"",
            "\"aoi_id\":\"piura-01\",\"cell_id\":\"piura-01-r00c00\"",
        );
        assert!(matches!(
            Inputs::from_cells_geojson("tailandia-01", &mixed),
            Err(RiskError::Input(_))
        ));
    }

    #[test]
    fn collection_aoi_must_match_the_requested_aoi() {
        let json = one_cell_geojson(&props("tailandia-01-r00c00", "0.95", "0.02", "0.01")).replace(
            "\"type\":\"FeatureCollection\"",
            "\"type\":\"FeatureCollection\",\"aoi_id\":\"piura-01\"",
        );
        assert!(matches!(
            Inputs::from_cells_geojson("tailandia-01", &json),
            Err(RiskError::Input(_))
        ));
    }

    #[test]
    fn unavailable_with_an_overlap_value_rejected() {
        let json = one_cell_geojson(
            r#"{"cell_id":"piura-demo-r00c00","valid_fraction":0.9,"decrease_fraction":0.1,
                "increase_fraction":0.0,"changed_fraction":0.1,"p90_abs_delta_db":4.0,
                "opera_status":"unavailable","opera_overlap_ratio":0.2,
                "road_m_near_change":0,"road_m_total":0}"#,
        );
        let err = Inputs::from_cells_geojson("piura-demo", &json).unwrap_err();
        assert!(matches!(err, RiskError::Input(_)), "{err}");
    }

    #[test]
    fn corroborated_without_an_overlap_value_rejected() {
        let json = one_cell_geojson(
            r#"{"cell_id":"piura-demo-r00c00","valid_fraction":0.9,"decrease_fraction":0.1,
                "increase_fraction":0.0,"changed_fraction":0.1,"p90_abs_delta_db":4.0,
                "opera_status":"corroborated","opera_overlap_ratio":null,
                "road_m_near_change":0,"road_m_total":0}"#,
        );
        let err = Inputs::from_cells_geojson("piura-demo", &json).unwrap_err();
        assert!(matches!(err, RiskError::Input(_)), "{err}");
    }

    #[test]
    fn duplicate_cell_id_rejected() {
        let p = props("piura-demo-r00c00", "0.95", "0.02", "0.01");
        let json = format!(
            r#"{{"type":"FeatureCollection","features":[
                {{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[[[0,0]]]}},"properties":{p}}},
                {{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[[[0,0]]]}},"properties":{p}}}]}}"#
        );
        let err = Inputs::from_cells_geojson("piura-demo", &json).unwrap_err();
        assert!(matches!(err, RiskError::DuplicateCell { .. }), "{err}");
    }

    #[test]
    fn fraction_above_one_rejected() {
        let json = one_cell_geojson(&props("piura-demo-r00c00", "1.5", "0.02", "0.01"));
        let err = Inputs::from_cells_geojson("piura-demo", &json).unwrap_err();
        assert!(matches!(err, RiskError::Input(_)), "{err}");
    }

    #[test]
    fn empty_collection_rejected() {
        let err = Inputs::from_cells_geojson(
            "piura-demo",
            r#"{"type":"FeatureCollection","features":[]}"#,
        )
        .unwrap_err();
        assert!(matches!(err, RiskError::Input(_)), "{err}");
    }
}
