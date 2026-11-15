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

use std::collections::BTreeMap;
use std::path::Path;

use wombat_core::v02::{Block, BlockType, Payload, TrustedIssuers};

use crate::bundle::{self, Index};
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
    for (relative, block) in &blocks {
        if let Err(err) = block.verify(Some(&as_of), &trusted) {
            errors.push(format!("{relative}: {err}"));
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

/// R27: una mayor distinta es incompatible y se rechaza con mensaje claro.
fn check_contract_major(index: &Index, errors: &mut Vec<String>) {
    let want = match std::fs::read_to_string(crate_contracts_version()) {
        Ok(raw) => raw.trim().to_string(),
        // Sin los contratos a mano no se puede comparar; no es culpa del bundle.
        Err(_) => return,
    };
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

    #[test]
    fn the_published_fixture_verifies_with_twenty_one_blocks() {
        let outcome = check(&fixture_bundle(), None, &fixture_trusted()).unwrap();
        assert_eq!(outcome.errors, Vec::<String>::new());
        assert_eq!(outcome.blocks, 21);
        // Sin --as-of, el TTL se mide contra generated_at del índice.
        assert_eq!(outcome.as_of, "2026-11-14T17:00:00Z");
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
}
