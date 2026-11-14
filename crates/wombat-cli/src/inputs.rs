//! Los insumos sin firmar que produce el pipeline de Python.
//!
//! R14: Python produce evidencia, Rust la firma. Este módulo lee esa evidencia
//! —`manifest.json`, `roads.geojson`, los overlays— y la deja lista para que
//! [`crate::build`] arme los bloques. Las celdas no se leen acá: las lee
//! `wombat_risk::load`, porque el que define qué es una celda válida es el crate
//! que puntúa (R15).
//!
//! Del manifest solo se tipan los campos que el bundle necesita. Es
//! deliberado: un `deny_unknown_fields` sobre todo el manifest obligaría a
//! tocar este crate cada vez que el pipeline agregue un campo de diagnóstico,
//! y el CLI no tiene nada que opinar sobre campos que no usa.

use std::path::Path;

use serde::Deserialize;
use wombat_core::v02::{DataOrigin, Frequency, Provenance, Source};

use crate::error::{self, CliError};

/// Los tres overlays que el bloque Change hashea, en el orden del contrato.
pub const OVERLAY_FILES: [&str; 3] = ["t1.png", "t2.png", "delta.png"];

/// Archivos que `build` copia del directorio de insumos al bundle, tal cual.
///
/// DECISIÓN: se copian **byte a byte**, sin reserializar. El `sha256` de
/// `cells.geojson` entra al payload firmado del bloque Change: si el CLI
/// reformateara el JSON, el hash dejaría de describir el archivo que el
/// pipeline produjo y la evidencia apuntaría a otra cosa.
pub const COPIED_INPUTS: [&str; 3] = ["manifest.json", "cells.geojson", "roads.geojson"];

/// Lo que el manifest le dice al bundle.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub aoi_id: String,
    pub data_origin: DataOrigin,
    pub nisar: Nisar,
    pub opera: Opera,
    pub osm: Osm,
    pub pipeline: Pipeline,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Nisar {
    pub product: String,
    pub maturity: String,
    pub polarization: String,
    pub frequency: Frequency,
    pub pixel_spacing_m: u32,
    pub t1: Acquisition,
    pub t2: Acquisition,
    pub comparable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Acquisition {
    pub granule: String,
    pub datetime: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Opera {
    pub product: String,
    pub derived_from: String,
    pub t1: OperaAcquisition,
    pub t2: OperaAcquisition,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OperaAcquisition {
    pub granules: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Osm {
    pub retrieved_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pipeline {
    pub risk_config_version: String,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, CliError> {
        let raw = error::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(|e| CliError::BadInput {
            path: path.display().to_string(),
            reason: e.to_string(),
        })
    }

    /// R1: un par no comparable solo se publica declarándolo simulado.
    ///
    /// La regla existe porque la primera pantalla del dashboard promete dato
    /// NISAR real. Publicar un par que no es comparable como si fuera real
    /// convierte esa promesa en una mentira firmada.
    pub fn check_comparable(&self) -> Result<(), CliError> {
        if !self.nisar.comparable && self.data_origin != DataOrigin::Simulated {
            return Err(CliError::NotComparable {
                data_origin: match self.data_origin {
                    DataOrigin::NisarReal => "nisar_real".to_string(),
                    DataOrigin::Simulated => "simulated".to_string(),
                },
            });
        }
        Ok(())
    }

    /// Fuente NISAR para la procedencia de los bloques.
    ///
    /// DECISIÓN: el `granule` sale del manifest (`t1/t2`), no de la constante
    /// `"SIMULATED-T1/T2"` que usaba el script bootstrap. El script la escribía
    /// a mano porque él mismo inventaba los datos; el CLI tiene el manifest
    /// delante y puede decir de qué gránulos salió el bloque. La procedencia no
    /// entra al payload, así que el cambio no mueve ningún número comparado
    /// contra el fixture: mejora la evidencia sin tocar el contrato.
    fn nisar_source(&self) -> Source {
        Source {
            product: self.nisar.product.clone(),
            extra: [
                (
                    "granule".to_string(),
                    serde_json::json!(format!(
                        "{}/{}",
                        self.nisar.t1.granule, self.nisar.t2.granule
                    )),
                ),
                (
                    "maturity".to_string(),
                    serde_json::json!(self.nisar.maturity),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    fn opera_source(&self) -> Source {
        let granules: Vec<&str> = self
            .opera
            .t1
            .granules
            .iter()
            .chain(self.opera.t2.granules.iter())
            .map(String::as_str)
            .collect();
        Source {
            product: self.opera.product.clone(),
            extra: [
                (
                    "derived_from".to_string(),
                    serde_json::json!(self.opera.derived_from),
                ),
                ("granule".to_string(), serde_json::json!(granules.join("/"))),
            ]
            .into_iter()
            .collect(),
        }
    }

    /// DECISIÓN: `retrieved_at` de OSM se recorta a la fecha (`YYYY-MM-DD`),
    /// como en el fixture. El manifest trae el instante completo; la atribución
    /// de OpenStreetMap se hace por día, y el instante exacto ya queda en el
    /// payload del bloque Context.
    fn osm_source(&self) -> Source {
        let date = self.osm.retrieved_at.split('T').next().unwrap_or_default();
        Source {
            product: "OpenStreetMap".to_string(),
            extra: [("retrieved_at".to_string(), serde_json::json!(date))]
                .into_iter()
                .collect(),
        }
    }

    /// Procedencia del bloque Change: de dónde salió el cambio observado.
    pub fn change_provenance(&self, code_commit: &str) -> Provenance {
        self.provenance(vec![self.nisar_source()], code_commit)
    }

    /// Procedencia del bloque Context: de dónde salió la exposición vial.
    pub fn context_provenance(&self, code_commit: &str) -> Provenance {
        self.provenance(vec![self.osm_source()], code_commit)
    }

    /// Procedencia de Risk y Action: el juicio depende de las tres fuentes.
    pub fn full_provenance(&self, code_commit: &str) -> Provenance {
        self.provenance(
            vec![self.nisar_source(), self.opera_source(), self.osm_source()],
            code_commit,
        )
    }

    fn provenance(&self, sources: Vec<Source>, code_commit: &str) -> Provenance {
        Provenance {
            sources,
            method: self.pipeline.risk_config_version.clone(),
            code_commit: code_commit.to_string(),
        }
    }
}

/// Metros de carretera en un `roads.geojson`, redondeados hacia abajo.
///
/// DECISIÓN: se reproduce la fórmula del script bootstrap
/// (`gen_fixture_bundle.py`, `seglen_m`): proyección equirectangular con
/// 110400 m por grado de longitud y 110570 m por grado de latitud, hipotenusa
/// por segmento, suma en el orden en que vienen los archivos, truncado a
/// entero. No es una geodésica y no pretende serlo —el pipeline real medirá en
/// EPSG:32717— pero es la fórmula con la que se firmó el fixture, y el CLI
/// tiene que reproducir ese número para poder reemplazar al script.
///
/// El `f64` vive **solo acá**: lo que sale es un entero en metros, y es ese
/// entero el que entra al payload firmado. R16 queda intacto.
pub fn roads_total_m(path: &Path) -> Result<u32, CliError> {
    /// Metros por grado, en la latitud del AOI. Ver la nota de arriba.
    const M_PER_DEG_LON: f64 = 110_400.0;
    const M_PER_DEG_LAT: f64 = 110_570.0;

    let raw = error::read_to_string(path)?;
    let bad = |reason: String| CliError::BadInput {
        path: path.display().to_string(),
        reason,
    };
    let collection: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| bad(e.to_string()))?;

    let features = collection["features"]
        .as_array()
        .ok_or_else(|| bad("falta el array \"features\"".to_string()))?;

    let mut total = 0.0_f64;
    for feature in features {
        for line in line_strings(&feature["geometry"]) {
            for pair in line.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let dx = (b.0 - a.0) * M_PER_DEG_LON;
                let dy = (b.1 - a.1) * M_PER_DEG_LAT;
                total += dx.hypot(dy);
            }
        }
    }
    Ok(total as u32)
}

/// Las polilíneas de una geometría: una para `LineString`, varias para
/// `MultiLineString`. Cualquier otra geometría no aporta longitud.
fn line_strings(geometry: &serde_json::Value) -> Vec<Vec<(f64, f64)>> {
    let coordinates = &geometry["coordinates"];
    match geometry["type"].as_str() {
        Some("LineString") => vec![positions(coordinates)],
        Some("MultiLineString") => coordinates
            .as_array()
            .map(|lines| lines.iter().map(positions).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn positions(value: &serde_json::Value) -> Vec<(f64, f64)> {
    value
        .as_array()
        .map(|points| {
            points
                .iter()
                .filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_bundle() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/fixtures/bundle/piura-demo/v0")
    }

    #[test]
    fn the_fixture_roads_measure_what_the_signed_context_block_says() {
        let measured = roads_total_m(&fixture_bundle().join("roads.geojson")).unwrap();
        // El bloque Context firmado del fixture declara 16242 m. Si este número
        // cambia, el CLI dejó de poder reproducir el bundle publicado.
        assert_eq!(measured, 16242);
    }

    #[test]
    fn the_fixture_manifest_is_comparable_and_simulated() {
        let manifest = Manifest::load(&fixture_bundle().join("manifest.json")).unwrap();
        assert_eq!(manifest.aoi_id, "piura-demo");
        assert_eq!(manifest.data_origin, DataOrigin::Simulated);
        assert!(manifest.nisar.comparable);
        manifest.check_comparable().unwrap();
    }

    #[test]
    fn a_non_comparable_pair_declared_real_is_refused() {
        let raw = std::fs::read_to_string(fixture_bundle().join("manifest.json")).unwrap();
        let altered = raw
            .replace("\"comparable\": true", "\"comparable\": false")
            .replace(
                "\"data_origin\": \"simulated\"",
                "\"data_origin\": \"nisar_real\"",
            );
        let manifest: Manifest = serde_json::from_str(&altered).unwrap();

        let err = manifest.check_comparable().unwrap_err();
        assert!(matches!(err, CliError::NotComparable { .. }), "{err}");
    }

    #[test]
    fn a_non_comparable_pair_declared_simulated_is_allowed() {
        let raw = std::fs::read_to_string(fixture_bundle().join("manifest.json")).unwrap();
        let altered = raw.replace("\"comparable\": true", "\"comparable\": false");
        let manifest: Manifest = serde_json::from_str(&altered).unwrap();
        manifest.check_comparable().unwrap();
    }

    #[test]
    fn provenance_carries_the_granules_from_the_manifest() {
        let manifest = Manifest::load(&fixture_bundle().join("manifest.json")).unwrap();
        let provenance = manifest.full_provenance("abc123");

        assert_eq!(provenance.sources.len(), 3);
        assert_eq!(provenance.method, "risk-config/0.1");
        assert_eq!(provenance.code_commit, "abc123");
        assert_eq!(
            provenance.sources[0].extra["granule"],
            serde_json::json!("SIMULATED-T1/SIMULATED-T2")
        );
        assert_eq!(
            provenance.sources[2].extra["retrieved_at"],
            serde_json::json!("2026-11-14"),
            "la atribución de OSM se hace por día"
        );
    }

    #[test]
    fn a_geometry_without_lines_contributes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roads.geojson");
        std::fs::write(
            &path,
            r#"{"type":"FeatureCollection","features":[
                 {"type":"Feature","geometry":{"type":"Point","coordinates":[-80.7,-5.2]}}]}"#,
        )
        .unwrap();
        assert_eq!(roads_total_m(&path).unwrap(), 0);
    }

    #[test]
    fn multilinestring_counts_every_part() {
        let dir = tempfile::tempdir().unwrap();
        let single = dir.path().join("single.geojson");
        let multi = dir.path().join("multi.geojson");
        std::fs::write(
            &single,
            r#"{"type":"FeatureCollection","features":[
                 {"type":"Feature","geometry":{"type":"LineString",
                  "coordinates":[[0.0,0.0],[0.01,0.0]]}},
                 {"type":"Feature","geometry":{"type":"LineString",
                  "coordinates":[[1.0,0.0],[1.01,0.0]]}}]}"#,
        )
        .unwrap();
        std::fs::write(
            &multi,
            r#"{"type":"FeatureCollection","features":[
                 {"type":"Feature","geometry":{"type":"MultiLineString",
                  "coordinates":[[[0.0,0.0],[0.01,0.0]],[[1.0,0.0],[1.01,0.0]]]}}]}"#,
        )
        .unwrap();
        assert_eq!(
            roads_total_m(&single).unwrap(),
            roads_total_m(&multi).unwrap()
        );
    }
}
