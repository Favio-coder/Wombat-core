//! `wombat verify`: el verificador Rust de un bundle completo.
//!
//! Es el espejo de `contracts/v1/tools/verify_bundle.py`, y lo es a propósito.
//! El README de los contratos lo dice: el valor del fixture es que tres
//! implementaciones independientes —Python, Rust, JS— acepten los mismos 21
//! bloques. Si una de las tres discrepa, el bug está en la canonicalización o en
//! el contrato, no en el verificador. Para que esa comparación sirva de algo,
//! este comando tiene que comprobar lo mismo, en el mismo orden, y decirlo con
//! el mismo formato.
//!
//! Lo que no hace: validar contra los JSON Schema. Eso lo hace el verificador
//! Python, que tiene `jsonschema` a mano. Acá la forma la impone el sistema de
//! tipos al deserializar: un payload con un campo de más o un float donde va un
//! entero no llega a construirse (R16).
//!
//! No corta en el primer error. Un verificador que se detiene en el primer
//! problema obliga a arreglar y reejecutar de a uno; este junta todo lo que
//! encuentra y lo informa junto.
//!
//! # La lista de confianza viene de afuera
//!
//! El bundle trae un `trusted-issuers.json` propio, y este verificador **no lo
//! lee**. Un bundle que se autoavala no prueba nada: quien pueda escribir en el
//! directorio puede cambiar esa lista, volver a firmar con su clave y obtener un
//! "OK". La firma solo significa algo contra una lista que el verificador ya
//! tenía antes de ver el bundle, y por eso `--trusted` es una ruta explícita
//! cuyo defecto es `config/trusted-issuers.json`, fuera del bundle.
//!
//! El `trusted-issuers.json` de dentro del bundle sigue existiendo porque el
//! contrato lo lista y su hash está en `index.json`; sirve como declaración de
//! quién *dice* el productor que firmó. Para usarlo hay que nombrarlo a mano.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use wombat_core::v02::{Block, BlockType, Payload, TrustedIssuers};

use crate::bundle::{self, BundlePurpose, Index};
use crate::error::{self, CliError};

/// Resultado de una verificación: cuántos bloques se miraron y qué falló.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub blocks: usize,
    pub errors: Vec<String>,
    pub as_of: String,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Verifica el bundle, imprime el informe y falla si algo no cuadra.
pub fn run(
    bundle_dir: &Path,
    as_of: Option<&str>,
    trusted_path: &Path,
) -> Result<Outcome, CliError> {
    let outcome = check(bundle_dir, as_of, trusted_path)?;

    for problem in &outcome.errors {
        println!("  ✗ {problem}");
    }
    // Mismo formato que verify_bundle.py, para que las salidas se puedan
    // comparar de un `diff` cuando alguna de las dos discrepe.
    println!(
        "{}  {}  |  {} bloques | as_of {} | {} error(es)",
        if outcome.ok() { "OK " } else { "FALLA" },
        bundle_dir.display(),
        outcome.blocks,
        outcome.as_of,
        outcome.errors.len()
    );

    if outcome.ok() {
        Ok(outcome)
    } else {
        Err(CliError::VerifyFailed {
            path: bundle_dir.display().to_string(),
            errors: outcome.errors.len(),
        })
    }
}

/// La verificación propiamente dicha, sin imprimir nada.
///
/// Separada de [`run`] para que los tests puedan mirar los errores en vez de
/// leer la salida estándar.
pub fn check(
    bundle_dir: &Path,
    as_of: Option<&str>,
    trusted_path: &Path,
) -> Result<Outcome, CliError> {
    let mut errors = Vec::new();

    let index = Index::load(&bundle_dir.join("index.json"))?;
    check_contract_major(&index, &mut errors);
    check_files(bundle_dir, &index, &mut errors);

    // Deliberadamente `trusted_path` y no `bundle_dir.join(...)`: ver el doc de
    // módulo. Si el archivo no está, es un error y no un "OK" silencioso.
    let trusted = TrustedIssuers::from_json(&error::read_to_string(trusted_path)?)?;

    // Un bundle estático es una foto histórica: el TTL se mide contra
    // `generated_at` salvo que se pida otra cosa (README, "TTL en bundles
    // estáticos").
    let as_of = as_of.unwrap_or(&index.generated_at).to_string();

    let blocks = read_blocks(bundle_dir, &mut errors)?;
    let errors_before_block_verification = errors.len();
    for (relative, block) in &blocks {
        if let Err(err) = block.verify(Some(&as_of), &trusted) {
            errors.push(format!("{relative}: {err}"));
        }
    }
    let block_verification_failed = errors.len() > errors_before_block_verification;
    if contract_version_at_least(&index.contract_version, (1, 2, 0)) {
        check_bundle_purpose(bundle_dir, &index, &blocks, &mut errors);
        if !block_verification_failed {
            check_signing_roles(index.purpose, &blocks, &trusted, &mut errors);
        }
        check_aoi_integrity(bundle_dir, &index, &blocks, &mut errors);
    } else {
        if index.purpose.is_some() {
            errors.push(format!(
                "contract_version {} anterior a 1.2.0 no puede declarar purpose",
                index.contract_version
            ));
        } else {
            check_legacy_profile(bundle_dir, &mut errors);
        }
    }
    check_parents(&blocks, &mut errors);
    check_change_block(bundle_dir, &blocks, &mut errors);

    Ok(Outcome {
        blocks: blocks.len(),
        errors,
        as_of,
    })
}

fn check_bundle_purpose(
    bundle_dir: &Path,
    index: &Index,
    blocks: &BTreeMap<String, Block>,
    errors: &mut Vec<String>,
) {
    let manifest_path = bundle_dir.join("manifest.json");
    let manifest = match error::read_to_string(&manifest_path)
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).map_err(Into::into))
    {
        Ok(value) => value,
        Err(err) => {
            errors.push(format!("manifest.json: {err}"));
            return;
        }
    };
    if manifest.get("aoi_id").and_then(|value| value.as_str()) != Some(index.aoi_id.as_str()) {
        errors.push("manifest.aoi_id no coincide con index.aoi_id".to_string());
    }

    let Some(purpose) = index.purpose else {
        errors.push(format!(
            "contract_version {} requiere purpose explícito",
            index.contract_version
        ));
        return;
    };
    let comparable = manifest
        .pointer("/nisar/comparable")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if purpose == BundlePurpose::Scientific && !comparable {
        errors.push("purpose=scientific requiere manifest.nisar.comparable=true".to_string());
    }
    if index.purpose == Some(BundlePurpose::Scientific) {
        let checks = manifest.pointer("/nisar/comparable_checks");
        if checks.is_none_or(|checks| {
            [
                "same_frame",
                "same_orbit_direction",
                "same_mode",
                "same_frequency",
                "same_polarization",
                "same_maturity",
            ]
            .iter()
            .any(|name| checks.get(name).and_then(|value| value.as_bool()) != Some(true))
        }) {
            errors
                .push("purpose=scientific requiere los seis comparable_checks en true".to_string());
        }
        let selected = ["t1", "t2"].iter().all(|scene| {
            manifest
                .pointer(&format!("/nisar/{scene}/granule"))
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.is_empty())
                && manifest
                    .pointer(&format!("/nisar/{scene}/datetime"))
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.is_empty())
        });
        let t1_granule = manifest
            .pointer("/nisar/t1/granule")
            .and_then(|v| v.as_str());
        let t2_granule = manifest
            .pointer("/nisar/t2/granule")
            .and_then(|v| v.as_str());
        let t1_time = manifest
            .pointer("/nisar/t1/datetime")
            .and_then(|v| v.as_str());
        let t2_time = manifest
            .pointer("/nisar/t2/datetime")
            .and_then(|v| v.as_str());
        if !selected
            || t1_granule == t2_granule
            || !t1_time.zip(t2_time).is_some_and(|(t1, t2)| t1 < t2)
        {
            errors.push(
                "purpose=scientific requiere t1/t2 seleccionados, distintos y en orden temporal"
                    .to_string(),
            );
        }
    }
    if purpose == BundlePurpose::Scientific {
        let cells = bundle_dir.join("cells.geojson");
        let count = match error::read_to_string(&cells) {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(value) => value
                    .get("features")
                    .and_then(|features| features.as_array())
                    .map(Vec::len),
                Err(err) => {
                    errors.push(format!("cells.geojson: {err}"));
                    None
                }
            },
            Err(err) => {
                errors.push(format!("cells.geojson: {err}"));
                None
            }
        };
        let risk_count = blocks
            .values()
            .filter(|block| block.body.block_type == BlockType::Risk)
            .count();
        if let Some(cell_count) = count {
            if cell_count != risk_count {
                errors.push(format!(
                    "purpose=scientific requiere un bloque Risk por celda ({cell_count} celdas, {risk_count} Risk)"
                ));
            }
        }
    }
    check_purpose_block_types(purpose, blocks, errors);
}

fn contract_version_at_least(version: &str, minimum: (u64, u64, u64)) -> bool {
    let mut parts = version
        .split('.')
        .filter_map(|part| part.parse::<u64>().ok());
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(major), Some(minor), Some(patch), None) => (major, minor, patch) >= minimum,
        _ => false,
    }
}

fn check_aoi_integrity(
    bundle_dir: &Path,
    index: &Index,
    blocks: &BTreeMap<String, Block>,
    errors: &mut Vec<String>,
) {
    let manifest = read_document(&bundle_dir.join("manifest.json"), "manifest.json", errors);
    let cells = read_document(&bundle_dir.join("cells.geojson"), "cells.geojson", errors);
    let roads = read_document(&bundle_dir.join("roads.geojson"), "roads.geojson", errors);
    let (Some(manifest), Some(cells), Some(roads)) = (manifest, cells, roads) else {
        return;
    };
    if manifest.get("aoi_id").and_then(|v| v.as_str()) != Some(index.aoi_id.as_str()) {
        errors.push("manifest.aoi_id no coincide con index.aoi_id".to_string());
    }

    let expected_crs = crate::inputs::expected_compute_crs(&index.aoi_id);
    if let Some(expected) = expected_crs {
        if manifest.get("crs_compute").and_then(|v| v.as_str()) != Some(expected) {
            errors.push(format!(
                "manifest.crs_compute debe ser {expected} para {}",
                index.aoi_id
            ));
        }
        if cells.get("crs_compute").and_then(|v| v.as_str()) != Some(expected) {
            errors.push(format!(
                "cells.geojson.crs_compute debe ser {expected} para {}",
                index.aoi_id
            ));
        }
    } else {
        errors.push(format!(
            "no hay una política CRS registrada para AOI {}",
            index.aoi_id
        ));
    }
    errors.extend(crate::inputs::cell_profile_errors(
        &index.aoi_id,
        expected_crs.unwrap_or_default(),
        &cells,
    ));
    errors.extend(crate::inputs::roads_profile_errors(&index.aoi_id, &roads));

    let mut cell_ids = HashSet::new();
    if let Some(features) = cells.get("features").and_then(|v| v.as_array()) {
        for feature in features {
            if let Some(cell_id) = feature
                .pointer("/properties/cell_id")
                .and_then(|v| v.as_str())
            {
                if !cell_ids.insert(cell_id.to_string()) {
                    errors.push(format!(
                        "cells.geojson contiene cell_id duplicado {cell_id}"
                    ));
                }
            }
        }
    }
    let mut by_id = HashMap::new();
    for block in blocks.values() {
        if by_id.insert(block.id.as_str(), block).is_some() {
            errors.push(format!("id de bloque duplicado {}", block.id));
        }
    }

    let (mut changes, mut contexts) = (0, 0);
    for (relative, block) in blocks {
        let expected_topic = aoi_topic(&index.aoi_id, block.body.block_type);
        if block.body.topic != expected_topic {
            errors.push(format!(
                "{relative}: topic {} no pertenece a {} (esperado {})",
                block.body.topic, index.aoi_id, expected_topic
            ));
        }
        match &block.body.payload {
            Payload::Change(payload) => {
                changes += 1;
                if payload.aoi_id != index.aoi_id {
                    errors.push(format!("{relative}: Change de AOI distinto"));
                }
                if !block.body.parents.is_empty() {
                    errors.push(format!("{relative}: Change no debe tener padres"));
                }
            }
            Payload::Context(payload) => {
                contexts += 1;
                if payload.aoi_id != index.aoi_id {
                    errors.push(format!("{relative}: Context de AOI distinto"));
                }
                if !block.body.parents.is_empty() {
                    errors.push(format!("{relative}: Context no debe tener padres"));
                }
            }
            Payload::Risk(payload) => {
                if !cell_belongs_to(&payload.cell_id, &index.aoi_id, &cell_ids) {
                    errors.push(format!(
                        "{relative}: Risk cell_id {} no pertenece a {}",
                        payload.cell_id, index.aoi_id
                    ));
                }
                let parents = parent_blocks(block, &by_id, 2, relative, errors);
                let has_change = parents.iter().any(|parent| {
                    matches!(&parent.body.payload, Payload::Change(value) if value.aoi_id == index.aoi_id)
                });
                let has_context = parents.iter().any(|parent| {
                    matches!(&parent.body.payload, Payload::Context(value) if value.aoi_id == index.aoi_id)
                });
                if !has_change || !has_context {
                    errors.push(format!(
                        "{relative}: Risk requiere padres Change y Context válidos del mismo AOI"
                    ));
                }
            }
            Payload::Action(payload) => {
                if !cell_belongs_to(&payload.cell_id, &index.aoi_id, &cell_ids) {
                    errors.push(format!(
                        "{relative}: Action cell_id {} no pertenece a {}",
                        payload.cell_id, index.aoi_id
                    ));
                }
                let parents = parent_blocks(block, &by_id, 1, relative, errors);
                if let Some(parent) = parents.first() {
                    match &parent.body.payload {
                        Payload::Risk(risk) if risk.cell_id == payload.cell_id => {}
                        Payload::Risk(_) => errors.push(format!(
                            "{relative}: Action cell_id no coincide con Risk padre"
                        )),
                        _ => errors.push(format!("{relative}: Action requiere un padre Risk")),
                    }
                }
            }
        }
    }
    if changes != 1 || contexts != 1 {
        errors.push(format!(
            "contract 1.2.0 requiere un Change y un Context (hay {changes} y {contexts})"
        ));
    }
}

fn read_document(path: &Path, label: &str, errors: &mut Vec<String>) -> Option<serde_json::Value> {
    match error::read_to_string(path)
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).map_err(Into::into))
    {
        Ok(value) => Some(value),
        Err(err) => {
            errors.push(format!("{label}: {err}"));
            None
        }
    }
}

fn cell_belongs_to(cell_id: &str, aoi_id: &str, cells: &HashSet<String>) -> bool {
    cell_id.starts_with(&format!("{aoi_id}-r")) && cells.contains(cell_id)
}

fn parent_blocks<'a>(
    block: &Block,
    by_id: &HashMap<&'a str, &'a Block>,
    expected_count: usize,
    relative: &str,
    errors: &mut Vec<String>,
) -> Vec<&'a Block> {
    if block.body.parents.len() != expected_count {
        errors.push(format!(
            "{relative}: se esperaban {expected_count} padres, hay {}",
            block.body.parents.len()
        ));
    }
    block
        .body
        .parents
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .collect()
}

fn aoi_topic(aoi_id: &str, block_type: BlockType) -> String {
    let prefix = if aoi_id == "piura-demo" {
        "wombat/pe/piura"
    } else {
        "wombat/aoi"
    };
    format!("{prefix}/{aoi_id}/{block_type}")
}

fn check_legacy_profile(bundle_dir: &Path, errors: &mut Vec<String>) {
    let Some(cells) = read_document(&bundle_dir.join("cells.geojson"), "cells.geojson", errors)
    else {
        return;
    };
    if cells.get("aoi_id").is_some() {
        errors.push(
            "bundle legacy sin purpose contiene perfil cells 1.2.0; posible downgrade del índice"
                .to_string(),
        );
    }
}

fn check_signing_roles(
    purpose: Option<BundlePurpose>,
    blocks: &BTreeMap<String, Block>,
    trusted: &TrustedIssuers,
    errors: &mut Vec<String>,
) {
    let Some(purpose) = purpose else {
        return;
    };
    let required = if purpose == BundlePurpose::Scientific {
        &[
            BlockType::Change,
            BlockType::Context,
            BlockType::Risk,
            BlockType::Action,
        ][..]
    } else {
        &[BlockType::Change, BlockType::Context][..]
    };
    let issuers: HashSet<&str> = blocks
        .values()
        .map(|block| block.body.issuer.as_str())
        .collect();
    for issuer in issuers {
        for role in required {
            if !trusted.allows(issuer, *role) {
                errors.push(format!(
                    "purpose={purpose:?}: issuer {issuer} no tiene el rol {role}"
                ));
            }
        }
    }
}

fn check_purpose_block_types(
    purpose: BundlePurpose,
    blocks: &BTreeMap<String, Block>,
    errors: &mut Vec<String>,
) {
    if purpose != BundlePurpose::Scientific
        && blocks
            .values()
            .any(|block| matches!(block.body.block_type, BlockType::Risk | BlockType::Action))
    {
        errors.push(format!(
            "purpose={purpose:?} no puede contener bloques Risk ni Action"
        ));
    }
}

/// R27: una mayor distinta es incompatible y se rechaza con mensaje claro.
fn check_contract_major(index: &Index, errors: &mut Vec<String>) {
    let want = match std::fs::read_to_string(crate_contracts_version()) {
        Ok(raw) => raw.trim().to_string(),
        // Sin los contratos a mano no se puede comparar; no es culpa del bundle.
        Err(_) => return,
    };
    if !contract_version_at_least(&index.contract_version, (0, 0, 0)) {
        errors.push(format!(
            "contract_version inválida: {}",
            index.contract_version
        ));
        return;
    }
    let major = |v: &str| v.split('.').next().unwrap_or_default().to_string();
    if major(&index.contract_version) != major(&want) {
        errors.push(format!(
            "contract_version {} incompatible con contratos v{} (R27)",
            index.contract_version,
            major(&want)
        ));
    }
}

fn crate_contracts_version() -> std::path::PathBuf {
    let relative = Path::new("contracts/v1/VERSION");
    if relative.exists() {
        return relative.to_path_buf();
    }
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(current) = dir {
        let candidate = current.join(relative);
        if candidate.exists() {
            return candidate;
        }
        dir = current.parent();
    }
    relative.to_path_buf()
}

/// Cada archivo del índice existe y su sha256 es el declarado.
///
/// Es el chequeo que atrapa el byte cambiado, y por eso el mensaje nombra el
/// archivo: quien lo lea tiene que saber qué abrir.
fn check_files(bundle_dir: &Path, index: &Index, errors: &mut Vec<String>) {
    for entry in &index.files {
        let path = bundle_dir.join(&entry.path);
        if !path.exists() {
            errors.push(format!("falta {}", entry.path));
            continue;
        }
        match bundle::sha256_file(&path) {
            Ok(actual) if actual == entry.sha256 => {}
            Ok(actual) => errors.push(format!(
                "sha256 distinto: {} (índice {}, archivo {})",
                entry.path,
                &entry.sha256[..16],
                &actual[..16]
            )),
            Err(err) => errors.push(format!("{}: {err}", entry.path)),
        }
    }
}

/// Todos los bloques bajo `wonb/`, por ruta relativa y en orden estable.
fn read_blocks(
    bundle_dir: &Path,
    errors: &mut Vec<String>,
) -> Result<BTreeMap<String, Block>, CliError> {
    let wonb = bundle_dir.join("wonb");
    let mut paths = Vec::new();
    collect_json(&wonb, &mut paths)?;
    paths.sort();

    let mut blocks = BTreeMap::new();
    for path in paths {
        let relative = path
            .strip_prefix(bundle_dir)
            .unwrap_or(&path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        match Block::from_json(&error::read_to_string(&path)?) {
            Ok(block) => {
                blocks.insert(relative, block);
            }
            Err(err) => errors.push(format!("{relative}: {err}")),
        }
    }
    Ok(blocks)
}

fn collect_json(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), CliError> {
    let entries = std::fs::read_dir(dir).map_err(|source| CliError::Read {
        path: dir.display().to_string(),
        source,
    })?;
    for entry in entries {
        let path = entry
            .map_err(|source| CliError::Read {
                path: dir.display().to_string(),
                source,
            })?
            .path();
        if path.is_dir() {
            collect_json(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
    Ok(())
}

/// Cada padre declarado existe en el conjunto (`chain_ok`).
///
/// No se usa `wombat_core::v02::verify_chain` porque ese corta en el primer
/// padre ausente y aquí se informan todos, con el archivo que los declara.
fn check_parents(blocks: &BTreeMap<String, Block>, errors: &mut Vec<String>) {
    let ids: std::collections::HashSet<&str> = blocks.values().map(|b| b.id.as_str()).collect();
    for (relative, block) in blocks {
        for parent in &block.body.parents {
            if !ids.contains(parent.as_str()) {
                errors.push(format!(
                    "{relative}: el padre {}… no existe (chain_ok = false)",
                    &parent[..19.min(parent.len())]
                ));
            }
        }
    }
}

/// El bloque Change es único y sus hashes describen los archivos del bundle.
///
/// Este es el chequeo que cierra el círculo: `index.json` no está firmado, pero
/// el Change sí, y repite los hashes de `cells.geojson` y de los tres overlays.
/// Alterar un overlay y recalcular el índice no alcanza: habría que volver a
/// firmar el Change, y para eso hace falta la clave.
fn check_change_block(
    bundle_dir: &Path,
    blocks: &BTreeMap<String, Block>,
    errors: &mut Vec<String>,
) {
    let changes: Vec<&Block> = blocks
        .values()
        .filter(|b| b.body.block_type == BlockType::Change)
        .collect();
    if changes.len() != 1 {
        errors.push(format!(
            "se esperaba 1 bloque change, hay {}",
            changes.len()
        ));
        return;
    }
    let Payload::Change(payload) = &changes[0].body.payload else {
        errors.push("el bloque change no lleva un payload change".to_string());
        return;
    };

    let overlays = bundle_dir.join("overlays");
    if overlays.exists() {
        for (name, declared) in [
            ("t1.png", &payload.overlay_sha256.t1_png),
            ("t2.png", &payload.overlay_sha256.t2_png),
            ("delta.png", &payload.overlay_sha256.delta_png),
        ] {
            match bundle::sha256_file(&overlays.join(name)) {
                Ok(actual) if &actual == declared => {}
                Ok(_) => errors.push(format!("overlay {name} no coincide con el bloque change")),
                Err(err) => errors.push(format!("overlay {name}: {err}")),
            }
        }
    }

    match bundle::sha256_file(&bundle_dir.join("cells.geojson")) {
        Ok(actual) if actual == payload.cells_geojson_sha256 => {}
        Ok(_) => errors.push("cells.geojson no coincide con el bloque change".to_string()),
        Err(err) => errors.push(format!("cells.geojson: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use serde_json::{json, Value};
    use wombat_core::v02::{issuer_string, BlockBody};

    fn fixture_bundle() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/fixtures/bundle/piura-demo/v0")
    }

    /// La lista del propio fixture, nombrada **a mano**.
    ///
    /// El fixture lo firmó una clave de desarrollo que a propósito no está en
    /// `config/trusted-issuers.json`: su privada se declaró descartada y la
    /// confianza de producción no tiene por qué cargar con ella. Estos tests
    /// miran la mecánica del verificador —hashes, ids, TTL, cadena—, así que
    /// pasan esa lista explícitamente. Que haya que escribir esta línea es el
    /// comportamiento buscado: la lista de dentro del bundle no se usa sola.
    fn fixture_trusted() -> std::path::PathBuf {
        fixture_bundle().join("trusted-issuers.json")
    }

    /// Un `trusted-issuers.json` que no contiene al emisor del fixture.
    fn foreign_trusted(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("trusted-issuers.json");
        std::fs::write(
            &path,
            r#"{"version":1,"issuers":[{"issuer":"ed25519:0000000000000000000000000000000000000000000000000000000000000000","name":"otra-clave","roles":["change","context","risk","action"]}]}"#,
        )
        .unwrap();
        path
    }

    fn test_block(kind: &str, topic: &str, parents: Vec<String>, payload: Value) -> Block {
        let key = SigningKey::from_bytes(&[7; 32]);
        let valid_until = match kind {
            "risk" => Some("2026-10-01T10:00:00Z"),
            "action" => Some("2026-09-30T11:00:00Z"),
            _ => None,
        };
        let body: BlockBody = serde_json::from_value(json!({
            "wonb_version": "0.2",
            "type": kind,
            "topic": topic,
            "issued_at": "2026-09-30T10:00:00Z",
            "valid_until": valid_until,
            "issuer": issuer_string(&key.verifying_key()),
            "parents": parents,
            "payload": payload,
            "provenance": {
                "sources": [],
                "method": "test",
                "code_commit": "test"
            }
        }))
        .unwrap();
        Block::seal(body, &key).unwrap()
    }

    fn test_trusted_all_roles() -> TrustedIssuers {
        let key = SigningKey::from_bytes(&[7; 32]);
        let issuer = issuer_string(&key.verifying_key());
        TrustedIssuers::from_json(&format!(
            r#"{{"version":1,"issuers":[{{"issuer":"{issuer}","name":"test","roles":["change","context","risk","action"]}}]}}"#
        ))
        .unwrap()
    }

    fn semantic_test_bundle(
        change_aoi: &str,
        risk_cell: &str,
        risk_topic: &str,
        risk_parents: Vec<&str>,
        include_crs: bool,
    ) -> (tempfile::TempDir, BTreeMap<String, Block>) {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = json!({"aoi_id": "tailandia-01"});
        if include_crs {
            manifest["crs_compute"] = json!("EPSG:32647");
        }
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("cells.geojson"),
            serde_json::to_vec(&json!({
                "type": "FeatureCollection",
                "aoi_id": "tailandia-01",
                "crs_compute": "EPSG:32647",
                "features": [{
                    "type": "Feature",
                    "geometry": {"type": "Polygon", "coordinates": []},
                    "properties": {
                        "aoi_id": "tailandia-01",
                        "cell_id": "tailandia-01-r00c00",
                        "source_status": {"nisar":"available","opera":"unavailable","osm":"unavailable"},
                        "provenance_refs": {"nisar":["m#/nisar"],"opera":[],"osm":[]}
                    }
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("roads.geojson"),
            r#"{"type":"FeatureCollection","aoi_id":"tailandia-01","features":[]}"#,
        )
        .unwrap();

        let change = test_block(
            "change",
            "wombat/aoi/tailandia-01/change",
            vec![],
            json!({
                "aoi_id": change_aoi,
                "data_origin": "nisar_real",
                "t1": "2026-09-18T10:00:00Z",
                "t2": "2026-09-30T10:00:00Z",
                "polarization": "HHHH",
                "frequency": "A",
                "pixel_spacing_m": 20,
                "delta_db_threshold_cdb": 100,
                "aoi_valid_bp": 9000,
                "aoi_decrease_bp": 100,
                "aoi_increase_bp": 100,
                "overlay_sha256": {"t1.png":"a","t2.png":"b","delta.png":"c"},
                "cells_geojson_sha256": "cells-hash"
            }),
        );
        let context = test_block(
            "context",
            "wombat/aoi/tailandia-01/context",
            vec![],
            json!({
                "aoi_id": "tailandia-01",
                "roads_total_m": 100,
                "road_buffer_m": 50,
                "osm_retrieved_at": "2026-09-30T10:00:00Z",
                "roads_geojson_sha256": "roads-hash"
            }),
        );
        let other_change = if risk_parents.contains(&"other-change") {
            Some(test_block(
                "change",
                "wombat/aoi/tailandia-01/change",
                vec![],
                json!({
                    "aoi_id": "tailandia-01",
                    "data_origin": "nisar_real",
                    "t1": "2026-09-18T10:00:00Z",
                    "t2": "2026-09-30T10:00:00Z",
                    "polarization": "HHHH",
                    "frequency": "A",
                    "pixel_spacing_m": 20,
                    "delta_db_threshold_cdb": 100,
                    "aoi_valid_bp": 9000,
                    "aoi_decrease_bp": 100,
                    "aoi_increase_bp": 100,
                    "overlay_sha256": {"t1.png":"a","t2.png":"b","delta.png":"c"},
                    "cells_geojson_sha256": "cells-hash"
                }),
            ))
        } else {
            None
        };
        let resolve_parent = |parent: &str| match parent {
            "change" => change.id.clone(),
            "context" => context.id.clone(),
            "other-change" => other_change.as_ref().unwrap().id.clone(),
            other => panic!("parent de prueba no reconocido: {other}"),
        };
        let risk_parents: Vec<String> = risk_parents
            .iter()
            .map(|parent| resolve_parent(parent))
            .collect();
        let risk = test_block(
            "risk",
            risk_topic,
            risk_parents,
            json!({
                "cell_id": risk_cell,
                "status": "review_low",
                "rank": 1,
                "score_bp": 100,
                "components_bp": {"change":50,"opera":25,"road":25},
                "metrics": {
                    "valid_bp":9000,"decrease_bp":100,"increase_bp":100,"changed_bp":100,
                    "p90_abs_delta_cdb":100,"opera_status":"unavailable","opera_overlap_bp":null,
                    "road_m_near_change":10,"road_m_total":100
                },
                "explanation": []
            }),
        );
        let action = test_block(
            "action",
            "wombat/aoi/tailandia-01/action",
            vec![risk.id.clone()],
            json!({
                "cell_id": risk_cell,
                "rank": 1,
                "action": "field_verification_first",
                "audience": "tailandia local operator"
            }),
        );
        let mut blocks = BTreeMap::from([
            ("change.json".to_string(), change),
            ("context.json".to_string(), context),
            ("risk.json".to_string(), risk),
            ("action.json".to_string(), action),
        ]);
        if let Some(other_change) = other_change {
            blocks.insert("other-change.json".to_string(), other_change);
        }
        (dir, blocks)
    }

    fn semantic_errors(dir: &Path, blocks: &BTreeMap<String, Block>) -> Vec<String> {
        let index = Index {
            contract_version: "1.2.0".to_string(),
            bundle_version: "v1".to_string(),
            aoi_id: "tailandia-01".to_string(),
            data_origin: wombat_core::v02::DataOrigin::NisarReal,
            purpose: Some(BundlePurpose::Scientific),
            generated_at: "2026-09-30T10:00:00Z".to_string(),
            core_commit: "test".to_string(),
            files: vec![],
        };
        let mut errors = Vec::new();
        check_aoi_integrity(dir, &index, blocks, &mut errors);
        errors
    }

    #[test]
    fn the_published_fixture_verifies_with_twenty_one_blocks() {
        let outcome = check(&fixture_bundle(), None, &fixture_trusted()).unwrap();
        assert_eq!(outcome.errors, Vec::<String>::new());
        assert_eq!(outcome.blocks, 21);
        // Sin --as-of, el TTL se mide contra generated_at del índice.
        assert_eq!(outcome.as_of, "2026-11-14T17:00:00Z");
    }

    #[test]
    fn legacy_bundle_without_purpose_keeps_its_historical_verification_path() {
        let index = Index::load(&fixture_bundle().join("index.json")).unwrap();
        assert!(index.purpose.is_none());
        assert!(!contract_version_at_least(
            &index.contract_version,
            (1, 2, 0)
        ));
        let outcome = check(&fixture_bundle(), None, &fixture_trusted()).unwrap();
        assert!(outcome.errors.is_empty(), "{:#?}", outcome.errors);
    }

    #[test]
    fn legacy_index_cannot_hide_a_12_integrated_cells_profile() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("cells.geojson"),
            r#"{"type":"FeatureCollection","aoi_id":"tailandia-01"}"#,
        )
        .unwrap();
        let mut errors = Vec::new();
        check_legacy_profile(dir.path(), &mut errors);
        assert!(errors
            .iter()
            .any(|error| error.contains("posible downgrade")));
    }

    #[test]
    fn contract_12_rejects_wrong_parent_types() {
        let (dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "tailandia-01-r00c00",
            "wombat/aoi/tailandia-01/risk",
            vec!["change", "other-change"],
            true,
        );
        let errors = semantic_errors(dir.path(), &blocks);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Change y Context válidos")),
            "{errors:?}"
        );
    }

    #[test]
    fn contract_12_accepts_an_aoi_consistent_chain() {
        let (dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "tailandia-01-r00c00",
            "wombat/aoi/tailandia-01/risk",
            vec!["change", "context"],
            true,
        );
        assert_eq!(semantic_errors(dir.path(), &blocks), Vec::<String>::new());
    }

    #[test]
    fn contract_12_rejects_a_parent_from_another_aoi() {
        let crossed: Value = serde_json::from_str(include_str!(
            "../../../contracts/v1/fixtures/aoi-integration/cross-aoi-chain.json"
        ))
        .unwrap();
        let (dir, blocks) = semantic_test_bundle(
            crossed
                .pointer("/change/aoi_id")
                .and_then(Value::as_str)
                .unwrap(),
            "tailandia-01-r00c00",
            crossed
                .pointer("/risk/topic")
                .and_then(Value::as_str)
                .unwrap(),
            vec!["change", "context"],
            true,
        );
        let trusted = test_trusted_all_roles();
        assert!(
            blocks
                .values()
                .all(|block| block.verify(Some("2026-09-30T10:00:00Z"), &trusted).is_ok()),
            "los bloques deben tener firmas y TTL válidos para aislar el chequeo AOI"
        );
        let errors = semantic_errors(dir.path(), &blocks);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Change de AOI distinto")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("padres Change y Context válidos")),
            "{errors:?}"
        );
    }

    #[test]
    fn contract_12_rejects_cell_and_topic_from_another_aoi() {
        let (dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "piura-01-r00c00",
            "wombat/aoi/piura-01/risk",
            vec!["change", "context"],
            true,
        );
        let errors = semantic_errors(dir.path(), &blocks);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("cell_id piura-01-r00c00")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("topic wombat/aoi/piura-01/risk")),
            "{errors:?}"
        );
    }

    #[test]
    fn contract_12_requires_a_manifest_compute_crs() {
        let (dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "tailandia-01-r00c00",
            "wombat/aoi/tailandia-01/risk",
            vec!["change", "context"],
            false,
        );
        std::fs::write(
            dir.path().join("manifest.json"),
            include_str!("../../../contracts/v1/fixtures/aoi-integration/crs-missing.json"),
        )
        .unwrap();
        let errors = semantic_errors(dir.path(), &blocks);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("manifest.crs_compute debe ser EPSG:32647")),
            "{errors:?}"
        );
    }

    #[test]
    fn contract_12_rejects_an_invalid_manifest_compute_crs() {
        let (dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "tailandia-01-r00c00",
            "wombat/aoi/tailandia-01/risk",
            vec!["change", "context"],
            true,
        );
        std::fs::write(
            dir.path().join("manifest.json"),
            include_str!("../../../contracts/v1/fixtures/aoi-integration/crs-invalid.json"),
        )
        .unwrap();
        let errors = semantic_errors(dir.path(), &blocks);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("manifest.crs_compute debe ser EPSG:32647")),
            "{errors:?}"
        );
    }

    #[test]
    fn baseline_and_preparation_cannot_contain_risk_or_action() {
        let (_dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "tailandia-01-r00c00",
            "wombat/aoi/tailandia-01/risk",
            vec!["change", "context"],
            true,
        );
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../contracts/v1/fixtures/aoi-integration/baseline-risk-action.json"
        ))
        .unwrap();
        assert_eq!(fixture["purpose"], "baseline");
        let mut errors = Vec::new();
        check_purpose_block_types(BundlePurpose::Baseline, &blocks, &mut errors);
        assert!(errors
            .iter()
            .any(|error| error.contains("no puede contener bloques Risk ni Action")));
    }

    #[test]
    fn evaluated_a_week_later_the_risk_and_action_blocks_are_expired() {
        let outcome = check(
            &fixture_bundle(),
            Some("2026-11-21T17:00:00Z"),
            &fixture_trusted(),
        )
        .unwrap();
        // 16 risk + 3 action caducan; change y context son históricos.
        assert_eq!(outcome.errors.len(), 19, "{:#?}", outcome.errors);
        assert!(outcome.errors.iter().all(|e| e.contains("expirado")));
    }

    #[test]
    fn a_trust_list_without_the_issuer_rejects_every_block() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = check(&fixture_bundle(), None, &foreign_trusted(dir.path())).unwrap();

        // El bundle es byte por byte el publicado: hashes, ids y firmas están
        // bien. Lo único que cambió es con qué lista se mide, y eso basta para
        // que los 21 bloques caigan.
        assert_eq!(outcome.blocks, 21);
        assert_eq!(outcome.errors.len(), 21, "{:#?}", outcome.errors);
    }

    #[test]
    fn the_list_inside_the_bundle_is_never_read_on_its_own() {
        // Mismo bundle, dos listas. Si `check` leyera la de dentro, el
        // resultado sería el mismo en los dos casos.
        let dir = tempfile::tempdir().unwrap();
        let con_la_propia = check(&fixture_bundle(), None, &fixture_trusted()).unwrap();
        let con_una_ajena = check(&fixture_bundle(), None, &foreign_trusted(dir.path())).unwrap();

        assert!(con_la_propia.ok());
        assert!(!con_una_ajena.ok());
    }

    #[test]
    fn a_missing_trust_list_is_an_error_and_not_a_pass() {
        let dir = tempfile::tempdir().unwrap();
        let err = check(&fixture_bundle(), None, &dir.path().join("no-existe.json")).unwrap_err();
        assert!(
            err.to_string().contains("no-existe.json"),
            "el error debería nombrar el archivo que falta: {err}"
        );
    }

    #[test]
    fn verification_accepts_baseline_roles_but_requires_all_scientific_roles() {
        let (_dir, blocks) = semantic_test_bundle(
            "tailandia-01",
            "tailandia-01-r00c00",
            "wombat/aoi/tailandia-01/risk",
            vec!["change", "context"],
            true,
        );
        let roots: BTreeMap<String, Block> = blocks
            .into_iter()
            .filter(|(_, block)| {
                matches!(
                    block.body.block_type,
                    BlockType::Change | BlockType::Context
                )
            })
            .collect();
        let mut role_fixture: Value = serde_json::from_str(include_str!(
            "../../../contracts/v1/fixtures/aoi-integration/baseline-minimum-roles.json"
        ))
        .unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        role_fixture["issuers"][0]["issuer"] = json!(issuer_string(&key.verifying_key()));
        let trusted = TrustedIssuers::from_json(&role_fixture.to_string()).unwrap();

        let mut baseline_errors = Vec::new();
        check_signing_roles(
            Some(BundlePurpose::Baseline),
            &roots,
            &trusted,
            &mut baseline_errors,
        );
        assert!(baseline_errors.is_empty(), "{baseline_errors:?}");

        let mut scientific_errors = Vec::new();
        check_signing_roles(
            Some(BundlePurpose::Scientific),
            &roots,
            &trusted,
            &mut scientific_errors,
        );
        assert!(scientific_errors
            .iter()
            .any(|error| error.contains("rol risk")));
        assert!(scientific_errors
            .iter()
            .any(|error| error.contains("rol action")));
    }
}
