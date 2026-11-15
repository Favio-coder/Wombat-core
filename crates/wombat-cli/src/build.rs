//! `wombat build`: de insumos sin firmar a bundle firmado.
//!
//! Este comando es el que hace cierto R14. Hasta que existió, el único productor
//! de bundles firmados era `gen_fixture_bundle.py`, que firmaba desde Python
//! porque no había alternativa. Ahora la hay, y el test de extremo a extremo
//! comprueba que los payloads que salen de acá son los mismos, byte a byte, que
//! los del bundle que ya está publicado.
//!
//! Lo que **no** hace: puntuar. El score, el ranking, el estado y la explicación
//! vienen de `wombat-risk` (R15). Si mañana cambia la fórmula, cambia allá y
//! este archivo no se toca.
//!
//! El orden de los pasos no es casual. Todo lo que puede rechazar la
//! construcción —la versión, el directorio ya publicado, el manifest no
//! comparable, el emisor no confiable— se comprueba **antes** de escribir el
//! primer byte. Un bundle a medio escribir es peor que uno que no existe.

use std::path::{Path, PathBuf};

use wombat_core::v02::{
    issuer_string, Block, BlockBody, BlockType, ChangePayload, ContextPayload, OverlayHashes,
    Payload, TrustedIssuers, WONB_VERSION,
};
use wombat_risk::{
    build_action_bodies, build_risk_bodies, evaluate, load, BlockContext, Exact, RiskConfig,
};

use crate::bundle::{self, BundlePurpose, Index};
use crate::error::{self, CliError};
use crate::inputs::{self, Manifest};
use crate::keys;

/// El demo conserva su topic histórico; todo AOI nuevo queda aislado por id.
const DEMO_TOPIC_PREFIX: &str = "wombat/pe/piura";
const AOI_TOPIC_PREFIX: &str = "wombat/aoi";

const SCIENTIFIC_ROLES: [BlockType; 4] = [
    BlockType::Change,
    BlockType::Context,
    BlockType::Risk,
    BlockType::Action,
];
const NON_OPERATIONAL_ROLES: [BlockType; 2] = [BlockType::Change, BlockType::Context];

/// Lo que `build` recibe de la línea de comandos.
#[derive(Debug, Clone)]
pub struct Request {
    pub input_dir: PathBuf,
    pub aoi_id: String,
    pub bundle_version: String,
    pub key_path: PathBuf,
    pub issued_at: Option<String>,
    pub out_dir: PathBuf,
    pub purpose: BundlePurpose,
    pub config_dir: PathBuf,
}

/// Lo que `build` deja escrito, para que quien lo llamó pueda contarlo.
#[derive(Debug, Clone)]
pub struct Report {
    pub out_dir: PathBuf,
    pub blocks: usize,
    pub issued_at: String,
    pub issuer: String,
}

pub fn run(request: &Request) -> Result<Report, CliError> {
    // --- 1. Todo lo que puede decir no, antes de escribir nada -------------
    bundle::check_version(&request.bundle_version)?;
    bundle::refuse_if_published(&request.out_dir)?;

    let manifest = Manifest::load(&request.input_dir.join("manifest.json"))?;
    if request.purpose == BundlePurpose::Scientific {
        manifest.check_comparable()?;
    }
    manifest.check_crs()?;
    check_aoi_matches(&manifest, &request.aoi_id)?;

    let signing_key = keys::load_signing_key(&request.key_path)?;
    let issuer = issuer_string(&signing_key.verifying_key());

    let trusted_path = request.config_dir.join("trusted-issuers.json");
    let trusted = TrustedIssuers::from_json(&error::read_to_string(&trusted_path)?)?;
    check_issuer_trusted(&trusted, &issuer, request.purpose, &trusted_path)?;

    let config_path = request.config_dir.join("risk-config.json");
    let config = load::load_config(&config_path)?;

    let cells_in = request.input_dir.join("cells.geojson");
    let crs_compute = manifest
        .crs_compute
        .as_deref()
        .ok_or_else(|| CliError::BadInput {
            path: "manifest.json".to_string(),
            reason: "crs_compute es obligatorio para bundles 1.2.0".to_string(),
        })?;
    check_cells_profile(&request.aoi_id, crs_compute, &cells_in)?;
    let roads_in = request.input_dir.join("roads.geojson");
    check_roads_profile(&request.aoi_id, &roads_in)?;
    let cell_inputs = load::load_inputs(&request.aoi_id, &cells_in)?;
    let scored = if request.purpose == BundlePurpose::Scientific {
        Some(evaluate(&config, &cell_inputs)?)
    } else {
        None
    };

    let issued_at = match &request.issued_at {
        Some(given) => given.clone(),
        None => now_rfc3339(),
    };
    // `valid_until_from` es la primera cosa que mira el formato de la fecha:
    // fallar acá es fallar antes de escribir, que es lo que queremos.
    wombat_core::v02::valid_until_from(&issued_at, 0)?;

    // --- 2. Copiar la evidencia tal como llegó ------------------------------
    let out = &request.out_dir;
    for name in inputs::COPIED_INPUTS {
        bundle::copy_verbatim(&request.input_dir.join(name), &out.join(name))?;
    }
    let overlay_hashes = copy_overlays(&request.input_dir, out)?;
    bundle::copy_verbatim(&config_path, &out.join("risk-config.json"))?;
    bundle::copy_verbatim(&trusted_path, &out.join("trusted-issuers.json"))?;

    // --- 3. Sellar la cadena Change/Context -> Risk -> Action --------------
    let code_commit = bundle::core_commit();

    let change = Block::seal(
        change_body(
            &manifest,
            &config,
            &cell_inputs.cells,
            &issued_at,
            &issuer,
            overlay_hashes,
            &bundle::sha256_file(&out.join("cells.geojson"))?,
            &code_commit,
        )?,
        &signing_key,
    )?;
    let context = Block::seal(
        context_body(&manifest, &config, &issued_at, &issuer, out, &code_commit)?,
        &signing_key,
    )?;

    let (risks, actions) = match scored {
        Some(scored) => {
            let block_context = BlockContext {
                aoi_id: request.aoi_id.clone(),
                topic_prefix: topic_prefix(&request.aoi_id).to_string(),
                issued_at: issued_at.clone(),
                issuer: issuer.clone(),
                change_id: change.id.clone(),
                context_id: context.id.clone(),
                provenance: manifest.full_provenance(&code_commit),
            };
            let risks: Vec<Block> = build_risk_bodies(&config, &scored, &block_context)?
                .into_iter()
                .map(|body| Block::seal(body, &signing_key))
                .collect::<Result<_, _>>()?;
            let actions: Vec<Block> = build_action_bodies(&config, &risks, &block_context)?
                .into_iter()
                .map(|body| Block::seal(body, &signing_key))
                .collect::<Result<_, _>>()?;
            (risks, actions)
        }
        None => (Vec::new(), Vec::new()),
    };

    // --- 4. Escribir los bloques y el índice -------------------------------
    bundle::write_json(&out.join("wonb/change.json"), &change)?;
    bundle::write_json(&out.join("wonb/context.json"), &context)?;
    for block in &risks {
        let cell_id = risk_cell_id(block)?;
        bundle::write_json(&out.join(format!("wonb/risk/{cell_id}.json")), block)?;
    }
    for block in &actions {
        let rank = action_rank(block)?;
        bundle::write_json(&out.join(format!("wonb/action/{rank}.json")), block)?;
    }

    let index = Index {
        contract_version: contract_version()?,
        bundle_version: request.bundle_version.clone(),
        aoi_id: request.aoi_id.clone(),
        data_origin: manifest.data_origin,
        purpose: Some(request.purpose),
        generated_at: issued_at.clone(),
        core_commit: code_commit,
        files: bundle::index_entries(out)?,
    };
    bundle::write_json(&out.join("index.json"), &index)?;

    Ok(Report {
        out_dir: out.clone(),
        blocks: 2 + risks.len() + actions.len(),
        issued_at,
        issuer,
    })
}

/// El manifest y la línea de comandos tienen que hablar del mismo AOI.
///
/// Si difieren, alguien se equivocó de directorio de insumos, y un bundle
/// firmado con el `aoi_id` de otra zona es evidencia apuntando al lugar
/// equivocado.
fn check_aoi_matches(manifest: &Manifest, aoi_id: &str) -> Result<(), CliError> {
    if manifest.aoi_id == aoi_id {
        return Ok(());
    }
    Err(CliError::BadInput {
        path: "manifest.json".to_string(),
        reason: format!(
            "el manifest declara aoi_id \"{}\" pero --aoi dice \"{aoi_id}\"",
            manifest.aoi_id
        ),
    })
}

fn check_issuer_trusted(
    trusted: &TrustedIssuers,
    issuer: &str,
    purpose: BundlePurpose,
    path: &Path,
) -> Result<(), CliError> {
    let required = if purpose == BundlePurpose::Scientific {
        &SCIENTIFIC_ROLES[..]
    } else {
        &NON_OPERATIONAL_ROLES[..]
    };
    let allowed = required.iter().all(|role| trusted.allows(issuer, *role));
    if allowed {
        return Ok(());
    }

    Err(CliError::IssuerNotTrusted {
        issuer: issuer.to_string(),
        path: path.display().to_string(),
    })
}

fn check_cells_profile(aoi_id: &str, crs_compute: &str, path: &Path) -> Result<(), CliError> {
    let raw = error::read_to_string(path)?;
    let cells: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| CliError::BadInput {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
    let errors = inputs::cell_profile_errors(aoi_id, crs_compute, &cells);
    if errors.is_empty() {
        return Ok(());
    }
    Err(CliError::BadInput {
        path: path.display().to_string(),
        reason: errors.join("; "),
    })
}

fn check_roads_profile(aoi_id: &str, path: &Path) -> Result<(), CliError> {
    let raw = error::read_to_string(path)?;
    let roads: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| CliError::BadInput {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
    let errors = inputs::roads_profile_errors(aoi_id, &roads);
    if errors.is_empty() {
        return Ok(());
    }
    Err(CliError::BadInput {
        path: path.display().to_string(),
        reason: errors.join("; "),
    })
}

/// Copia los overlays y devuelve los hashes de los tres PNG del contrato.
///
/// Se copia todo lo que haya en `overlays/` —`bounds.json` incluido, que el mapa
/// necesita para georreferenciar— pero solo los tres PNG entran al payload
/// firmado.
fn copy_overlays(input_dir: &Path, out: &Path) -> Result<OverlayHashes, CliError> {
    let from_dir = input_dir.join("overlays");
    let to_dir = out.join("overlays");

    let entries = std::fs::read_dir(&from_dir).map_err(|source| CliError::Read {
        path: from_dir.display().to_string(),
        source,
    })?;
    for entry in entries {
        let path = entry
            .map_err(|source| CliError::Read {
                path: from_dir.display().to_string(),
                source,
            })?
            .path();
        if path.is_file() {
            if let Some(name) = path.file_name() {
                bundle::copy_verbatim(&path, &to_dir.join(name))?;
            }
        }
    }

    let mut hashes = Vec::with_capacity(inputs::OVERLAY_FILES.len());
    for name in inputs::OVERLAY_FILES {
        let copied = to_dir.join(name);
        if !copied.exists() {
            return Err(CliError::BadInput {
                path: from_dir.join(name).display().to_string(),
                reason: format!("falta el overlay {name}, que el bloque Change debe hashear"),
            });
        }
        hashes.push(bundle::sha256_file(&copied)?);
    }
    Ok(OverlayHashes {
        t1_png: hashes[0].clone(),
        t2_png: hashes[1].clone(),
        delta_png: hashes[2].clone(),
    })
}

#[allow(clippy::too_many_arguments)]
fn change_body(
    manifest: &Manifest,
    config: &RiskConfig,
    cells: &[wombat_risk::CellInput],
    issued_at: &str,
    issuer: &str,
    overlay_sha256: OverlayHashes,
    cells_geojson_sha256: &str,
    code_commit: &str,
) -> Result<BlockBody, CliError> {
    Ok(BlockBody {
        wonb_version: WONB_VERSION.to_string(),
        block_type: BlockType::Change,
        topic: topic(&manifest.aoi_id, BlockType::Change),
        issued_at: issued_at.to_string(),
        // El cambio observado es histórico: no caduca (§7.4).
        valid_until: None,
        issuer: issuer.to_string(),
        parents: Vec::new(),
        payload: Payload::Change(ChangePayload {
            aoi_id: manifest.aoi_id.clone(),
            data_origin: manifest.data_origin,
            t1: manifest.nisar.t1.datetime.clone(),
            t2: manifest.nisar.t2.datetime.clone(),
            polarization: manifest.nisar.polarization.clone(),
            frequency: manifest.nisar.frequency,
            pixel_spacing_m: manifest.nisar.pixel_spacing_m,
            delta_db_threshold_cdb: config.delta_db_threshold_cdb()?,
            aoi_valid_bp: mean_bp(cells.iter().map(|c| c.valid_fraction), cells.len())?,
            aoi_decrease_bp: mean_bp(cells.iter().map(|c| c.decrease_fraction), cells.len())?,
            aoi_increase_bp: mean_bp(cells.iter().map(|c| c.increase_fraction), cells.len())?,
            overlay_sha256,
            cells_geojson_sha256: cells_geojson_sha256.to_string(),
        }),
        provenance: manifest.change_provenance(code_commit),
    })
}

fn context_body(
    manifest: &Manifest,
    config: &RiskConfig,
    issued_at: &str,
    issuer: &str,
    out: &Path,
    code_commit: &str,
) -> Result<BlockBody, CliError> {
    let roads = out.join("roads.geojson");
    Ok(BlockBody {
        wonb_version: WONB_VERSION.to_string(),
        block_type: BlockType::Context,
        topic: topic(&manifest.aoi_id, BlockType::Context),
        issued_at: issued_at.to_string(),
        // El contexto vial está fechado por `osm_retrieved_at`, no caduca (§7.4).
        valid_until: None,
        issuer: issuer.to_string(),
        parents: Vec::new(),
        payload: Payload::Context(ContextPayload {
            aoi_id: manifest.aoi_id.clone(),
            roads_total_m: inputs::roads_total_m(&roads)?,
            road_buffer_m: config.road_buffer_meters,
            osm_retrieved_at: manifest.osm.retrieved_at.clone(),
            roads_geojson_sha256: bundle::sha256_file(&roads)?,
        }),
        provenance: manifest.context_provenance(code_commit),
    })
}

/// Media de un conjunto de fracciones, en puntos básicos.
///
/// Suma y divide con racionales exactos y redondea una sola vez al final. Es la
/// diferencia entre reproducir el fixture y quedar a un punto básico: redondear
/// cada celda antes de promediar da otro número.
fn mean_bp(values: impl Iterator<Item = Exact>, count: usize) -> Result<u32, CliError> {
    if count == 0 {
        return Ok(0);
    }
    let mut sum = Exact::ZERO;
    for value in values {
        sum = sum.checked_add(value)?;
    }
    Ok(sum.checked_div(Exact::int(count as i128))?.to_bp_u32()?)
}

fn topic(aoi_id: &str, block_type: BlockType) -> String {
    format!("{}/{aoi_id}/{block_type}", topic_prefix(aoi_id))
}

fn topic_prefix(aoi_id: &str) -> &'static str {
    if aoi_id == "piura-demo" {
        DEMO_TOPIC_PREFIX
    } else {
        AOI_TOPIC_PREFIX
    }
}

fn risk_cell_id(block: &Block) -> Result<&str, CliError> {
    match &block.body.payload {
        Payload::Risk(payload) => Ok(payload.cell_id.as_str()),
        other => Err(CliError::BadInput {
            path: "wonb/risk".to_string(),
            reason: format!("se esperaba un payload risk, llegó {}", kind(other)),
        }),
    }
}

fn action_rank(block: &Block) -> Result<u32, CliError> {
    match &block.body.payload {
        Payload::Action(payload) => Ok(payload.rank),
        other => Err(CliError::BadInput {
            path: "wonb/action".to_string(),
            reason: format!("se esperaba un payload action, llegó {}", kind(other)),
        }),
    }
}

fn kind(payload: &Payload) -> &'static str {
    match payload {
        Payload::Change(_) => "change",
        Payload::Context(_) => "context",
        Payload::Risk(_) => "risk",
        Payload::Action(_) => "action",
    }
}

/// `contracts/v1/VERSION`, buscado desde el binario hacia arriba.
///
/// DECISIÓN: se lee del repo en vez de compilarse con `include_str!`. Un
/// `contract_version` incrustado en el binario mentiría en cuanto alguien
/// actualizara los contratos sin recompilar, y R27 pide que el número sea el
/// del contrato vigente, no el del día que se compiló.
fn contract_version() -> Result<String, CliError> {
    let path = contracts_version_path();
    Ok(error::read_to_string(&path)?.trim().to_string())
}

fn contracts_version_path() -> PathBuf {
    let relative = Path::new("contracts/v1/VERSION");
    if relative.exists() {
        return relative.to_path_buf();
    }
    // Ejecutado desde un directorio cualquiera (o desde `cargo test`, cuyo
    // directorio de trabajo es el del crate): se busca hacia arriba.
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

/// Ahora, en el formato del contrato: segundos enteros y `Z`.
fn now_rfc3339() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_are_namespaced_by_aoi_without_reusing_piura_for_other_regions() {
        assert_eq!(
            topic("tailandia-01", BlockType::Action),
            "wombat/aoi/tailandia-01/action"
        );
        assert_eq!(
            topic("piura-demo", BlockType::Action),
            "wombat/pe/piura/piura-demo/action"
        );
    }

    #[test]
    fn the_contract_version_is_readable_and_semver() {
        let version = contract_version().unwrap();
        assert_eq!(version.split('.').count(), 3, "{version}");
    }

    #[test]
    fn now_has_the_shape_the_contract_expects() {
        let now = now_rfc3339();
        assert!(now.ends_with('Z'), "{now}");
        assert_eq!(now.len(), 20, "{now}");
        wombat_core::v02::valid_until_from(&now, 60).unwrap();
    }

    #[test]
    fn an_empty_set_of_cells_averages_to_zero_instead_of_dividing_by_zero() {
        assert_eq!(mean_bp(std::iter::empty(), 0).unwrap(), 0);
    }

    #[test]
    fn the_mean_rounds_once_at_the_end_not_per_cell() {
        // 1/3 y 2/3: redondear cada una antes de promediar daría 3333 y 6667,
        // media 5000. Exacto: (1/3 + 2/3) / 2 = 1/2 = 5000 bp. Coinciden aquí,
        // pero el caso que importa es el del fixture: 1.45/16 = 906.25 bp, que
        // half-up da 906 y no 907.
        let third = Exact::new(1, 3).unwrap();
        let two_thirds = Exact::new(2, 3).unwrap();
        assert_eq!(mean_bp([third, two_thirds].into_iter(), 2).unwrap(), 5000);

        let values: Vec<Exact> = ["0.02", "0.05", "0.03", "0.01"]
            .iter()
            .map(|s| Exact::from_decimal_str(s).unwrap())
            .collect();
        // (0.02+0.05+0.03+0.01)/4 = 0.0275 -> 275 bp
        assert_eq!(mean_bp(values.into_iter(), 4).unwrap(), 275);
    }

    #[test]
    fn the_aoi_means_match_the_signed_change_block_of_the_fixture() {
        let bundle_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/fixtures/bundle/piura-demo/v0");
        let cells = load::load_inputs("piura-demo", bundle_dir.join("cells.geojson")).unwrap();

        let valid = mean_bp(
            cells.cells.iter().map(|c| c.valid_fraction),
            cells.cells.len(),
        )
        .unwrap();
        let decrease = mean_bp(
            cells.cells.iter().map(|c| c.decrease_fraction),
            cells.cells.len(),
        )
        .unwrap();
        let increase = mean_bp(
            cells.cells.iter().map(|c| c.increase_fraction),
            cells.cells.len(),
        )
        .unwrap();

        // Los tres números del bloque Change firmado de piura-demo/v0.
        assert_eq!((valid, decrease, increase), (9025, 906, 275));
    }

    #[test]
    fn an_issuer_missing_a_single_role_is_refused() {
        let trusted = TrustedIssuers::from_json(
            r#"{"version":1,"issuers":[
                 {"issuer":"ed25519:aa","name":"parcial",
                  "roles":["change","context","risk"]}]}"#,
        )
        .unwrap();
        let err = check_issuer_trusted(
            &trusted,
            "ed25519:aa",
            BundlePurpose::Scientific,
            Path::new("trusted-issuers.json"),
        )
        .unwrap_err();
        assert!(matches!(err, CliError::IssuerNotTrusted { .. }), "{err}");
    }

    #[test]
    fn an_issuer_with_the_four_roles_is_accepted() {
        let trusted = TrustedIssuers::from_json(
            r#"{"version":1,"issuers":[
                 {"issuer":"ed25519:aa","name":"completo",
                  "roles":["change","context","risk","action"]}]}"#,
        )
        .unwrap();
        check_issuer_trusted(
            &trusted,
            "ed25519:aa",
            BundlePurpose::Scientific,
            Path::new("trusted-issuers.json"),
        )
        .unwrap();
    }

    #[test]
    fn baseline_issuer_needs_only_change_and_context_roles() {
        let trusted = TrustedIssuers::from_json(include_str!(
            "../../../contracts/v1/fixtures/aoi-integration/baseline-minimum-roles.json"
        ))
        .unwrap();
        check_issuer_trusted(
            &trusted,
            "ed25519:baseline-test",
            BundlePurpose::Baseline,
            Path::new("trusted-issuers.json"),
        )
        .unwrap();
        assert!(check_issuer_trusted(
            &trusted,
            "ed25519:baseline-test",
            BundlePurpose::Scientific,
            Path::new("trusted-issuers.json"),
        )
        .is_err());
    }

    #[test]
    fn manifest_crs_must_match_the_aoi_policy() {
        let bundle_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/fixtures/bundle/piura-demo/v0");
        let manifest = Manifest::load(&bundle_dir.join("manifest.json")).unwrap();
        manifest.check_crs().unwrap();

        let mut wrong = manifest;
        wrong.crs_compute = Some("EPSG:32647".to_string());
        assert!(wrong.check_crs().is_err());
        wrong.crs_compute = None;
        assert!(wrong.check_crs().is_err());
    }

    #[test]
    fn scientific_manifest_fixture_satisfies_comparability_and_crs() {
        let manifest: Manifest = serde_json::from_str(include_str!(
            "../../../contracts/v1/fixtures/aoi-integration/manifest-scientific.json"
        ))
        .unwrap();
        manifest.check_comparable().unwrap();
        manifest.check_crs().unwrap();
    }

    #[test]
    fn a_manifest_for_another_aoi_is_refused() {
        let bundle_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/v1/fixtures/bundle/piura-demo/v0");
        let manifest = Manifest::load(&bundle_dir.join("manifest.json")).unwrap();

        check_aoi_matches(&manifest, "piura-demo").unwrap();
        let err = check_aoi_matches(&manifest, "otra-zona").unwrap_err();
        match err {
            CliError::BadInput { reason, .. } => {
                assert!(
                    reason.contains("piura-demo") && reason.contains("otra-zona"),
                    "{reason}"
                )
            }
            other => panic!("se esperaba BadInput, llegó {other}"),
        }
    }
}
