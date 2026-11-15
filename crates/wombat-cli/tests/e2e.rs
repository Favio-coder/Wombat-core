//! De extremo a extremo: el CLI reemplaza al script bootstrap.
//!
//! El fixture `piura-demo/v0` lo firmó `gen_fixture_bundle.py` porque R14 no
//! tenía todavía quien lo cumpliera. Este test comprueba que ya lo tiene: toma
//! los insumos **sin firmar** de ese mismo bundle, construye uno nuevo con
//! `wombat build` y una clave recién generada, y exige que los 21 payloads
//! salgan idénticos a los publicados.
//!
//! Lo que no puede coincidir —y no debe— son los `id`, las firmas y el
//! `issuer`: la clave privada del fixture se generó en memoria y se descartó
//! (R19), así que firmar de nuevo da otra firma. El contrato es que el
//! **contenido** sea el mismo, no los bytes de la firma.
//!
//! Se ejercita el binario de verdad, no las funciones internas. Un test que
//! llama a `build::run` no comprueba que `clap` pase los flags, ni que el
//! código de salida sea el correcto, y los dos son parte de lo que usa el CI.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// El bundle publicado: fuente de los insumos y referencia de la comparación.
fn fixture_bundle() -> PathBuf {
    repo_root().join("contracts/v1/fixtures/bundle/piura-demo/v0")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn wombat() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wombat"));
    // Desde la raíz, para que `contracts/v1/VERSION` se resuelva igual que
    // cuando lo corre una persona.
    cmd.current_dir(repo_root());
    cmd
}

fn run(cmd: &mut Command) -> Output {
    cmd.output().expect("no se pudo ejecutar el binario wombat")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// La última línea del informe de `verify`: el resumen `OK`/`FALLA`.
///
/// Antes van las líneas `✗`, una por error, así que el resumen no es la
/// primera línea salvo que no haya fallado nada.
fn summary_line(report: &str) -> &str {
    report.trim_end().lines().last().unwrap_or_default()
}

/// Un banco de trabajo: insumos copiados del fixture, config propia y clave
/// nueva, todo en un temporal que no está versionado.
struct Workspace {
    _root: tempfile::TempDir,
    input_dir: PathBuf,
    config_dir: PathBuf,
    key_path: PathBuf,
    out_dir: PathBuf,
    issuer: String,
}

impl Workspace {
    /// Prepara los insumos sin firmar y una clave de confianza.
    fn new() -> Self {
        Self::with_manifest(|raw| raw.to_string())
    }

    /// Igual, pero dejando retocar el `manifest.json` antes de copiarlo.
    fn with_manifest(edit: impl Fn(&str) -> String) -> Self {
        let root = tempfile::tempdir().expect("temporal");
        let input_dir = root.path().join("in");
        let config_dir = root.path().join("config");
        let key_path = root.path().join("keys/demo.key");
        let out_dir = root.path().join("out/v1");

        // Los insumos que produce Python: evidencia sin firmar.
        for name in ["cells.geojson", "roads.geojson"] {
            copy(&fixture_bundle().join(name), &input_dir.join(name));
        }
        let manifest = std::fs::read_to_string(fixture_bundle().join("manifest.json")).unwrap();
        write(&input_dir.join("manifest.json"), &edit(&manifest));
        copy_dir(
            &fixture_bundle().join("overlays"),
            &input_dir.join("overlays"),
        );

        // La configuración vigente. `risk-config.json` sale del propio bundle,
        // que es lo que tiene a mano quien quiera reproducirlo.
        copy(
            &fixture_bundle().join("risk-config.json"),
            &config_dir.join("risk-config.json"),
        );

        // Clave nueva. El temporal está fuera de cualquier repo, así que R19
        // la deja pasar.
        let output = run(wombat()
            .arg("keygen")
            .arg("--out")
            .arg(&key_path)
            .arg("--name")
            .arg("wombat-e2e"));
        assert!(output.status.success(), "keygen falló: {}", stderr(&output));
        let issuer = issuer_from_keygen(&stdout(&output));
        write(
            &config_dir.join("trusted-issuers.json"),
            &format!(
                "{{\n  \"version\": 1,\n  \"issuers\": [\n    {{ \"issuer\": \"{issuer}\", \
                 \"name\": \"wombat-e2e\", \"roles\": [\"change\", \"context\", \"risk\", \"action\"] }}\n  ]\n}}\n"
            ),
        );

        Workspace {
            _root: root,
            input_dir,
            config_dir,
            key_path,
            out_dir,
            issuer,
        }
    }

    /// `wombat build` sobre este banco, con el `issued_at` del fixture para que
    /// los TTL del bundle nuevo coincidan con los del publicado.
    fn build(&self) -> Output {
        self.build_into(&self.out_dir, "v1")
    }

    fn build_into(&self, out: &Path, version: &str) -> Output {
        run(wombat()
            .arg("build")
            .arg("--in")
            .arg(&self.input_dir)
            .arg("--aoi")
            .arg("piura-demo")
            .arg("--version")
            .arg(version)
            .arg("--key")
            .arg(&self.key_path)
            .arg("--issued-at")
            .arg("2026-11-14T17:00:00Z")
            .arg("--out")
            .arg(out)
            .arg("--config")
            .arg(&self.config_dir))
    }

    /// `wombat verify` midiendo las firmas contra la lista de **este** banco.
    ///
    /// El flag es obligatorio acá porque el defecto es
    /// `config/trusted-issuers.json` relativo al directorio de trabajo, y el
    /// del test es el del crate, no la raíz del repositorio.
    fn verify(&self) -> Output {
        self.verify_against(&self.config_dir.join("trusted-issuers.json"))
    }

    fn verify_against(&self, trusted: &Path) -> Output {
        run(wombat()
            .arg("verify")
            .arg(&self.out_dir)
            .arg("--trusted")
            .arg(trusted))
    }
}

/// `ed25519:<hex>` de la salida de `keygen`.
fn issuer_from_keygen(out: &str) -> String {
    out.lines()
        .map(str::trim)
        .find(|line| line.starts_with("ed25519:"))
        .unwrap_or_else(|| panic!("keygen no imprimió la clave pública:\n{out}"))
        .to_string()
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(from, to).unwrap_or_else(|e| panic!("copiando {}: {e}", from.display()));
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            copy(&path, &to.join(path.file_name().unwrap()));
        }
    }
}

/// Los payloads de un bundle, por ruta relativa dentro de `wonb/`.
fn payloads(bundle: &Path) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut found = std::collections::BTreeMap::new();
    collect(&bundle.join("wonb"), bundle, &mut found);
    assert!(!found.is_empty(), "sin bloques en {}", bundle.display());
    found
}

fn collect(
    dir: &Path,
    root: &Path,
    out: &mut std::collections::BTreeMap<String, serde_json::Value>,
) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let block: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            out.insert(relative, block);
        }
    }
}

// --- (a) END-TO-END ------------------------------------------------------

#[test]
fn the_bundle_built_from_the_unsigned_inputs_verifies_with_twenty_one_blocks() {
    let workspace = Workspace::new();
    let built = workspace.build();
    assert!(built.status.success(), "build falló: {}", stderr(&built));

    let verified = workspace.verify();
    let report = stdout(&verified);
    assert!(
        verified.status.success(),
        "verify falló:\n{report}{}",
        stderr(&verified)
    );
    assert!(report.contains("21 bloques"), "{report}");
    assert!(report.contains("0 error(es)"), "{report}");
    assert!(summary_line(&report).starts_with("OK "), "{report}");
}

#[test]
fn every_one_of_the_twenty_one_payloads_matches_the_published_fixture() {
    let workspace = Workspace::new();
    let built = workspace.build();
    assert!(built.status.success(), "build falló: {}", stderr(&built));

    let ours = payloads(&workspace.out_dir);
    let theirs = payloads(&fixture_bundle());

    assert_eq!(
        ours.keys().collect::<Vec<_>>(),
        theirs.keys().collect::<Vec<_>>(),
        "el bundle construido debe tener los mismos archivos de bloque"
    );
    assert_eq!(ours.len(), 21);

    for (relative, theirs_block) in &theirs {
        let ours_block = &ours[relative];
        assert_eq!(
            ours_block["payload"], theirs_block["payload"],
            "el payload de {relative} difiere del publicado"
        );
        // Lo que sí cambia, y tiene que cambiar: la clave del fixture se
        // descartó, así que firmamos con otra.
        assert_ne!(
            ours_block["issuer"], theirs_block["issuer"],
            "{relative}: se esperaba otro emisor"
        );
        // Y lo que no cambia aunque cambie la firma: el tipo, el tópico y la
        // vigencia salen del contrato, no de quién firma.
        for field in ["type", "topic", "issued_at", "valid_until", "wonb_version"] {
            assert_eq!(
                ours_block[field], theirs_block[field],
                "{relative}: {field} difiere"
            );
        }
    }
}

#[test]
fn the_ranking_of_the_built_bundle_is_the_golden_one() {
    let workspace = Workspace::new();
    assert!(workspace.build().status.success());

    let golden: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("contracts/v1/fixtures/expected/ranking.json"))
            .unwrap(),
    )
    .unwrap();

    // Las celdas rankeadas del bundle construido, ordenadas por rank.
    let mut ranked: Vec<serde_json::Value> = payloads(&workspace.out_dir)
        .values()
        .filter(|b| b["type"] == "risk")
        .map(|b| b["payload"].clone())
        .filter(|p| !p["rank"].is_null())
        .collect();
    ranked.sort_by_key(|p| p["rank"].as_u64().unwrap());

    let expected = golden["ranking"].as_array().unwrap();
    assert_eq!(ranked.len(), expected.len(), "celdas en el ranking");

    for (ours, want) in ranked.iter().zip(expected) {
        let where_ = format!("rank {}", want["rank"]);
        assert_eq!(ours["cell_id"], want["cell_id"], "{where_}");
        assert_eq!(ours["rank"], want["rank"], "{where_}");
        assert_eq!(ours["score_bp"], want["score_bp"], "{where_}");
        assert_eq!(ours["status"], want["status"], "{where_}");
        assert_eq!(ours["components_bp"], want["components_bp"], "{where_}");
    }

    let blocks = payloads(&workspace.out_dir);
    let excluded: Vec<&str> = blocks
        .values()
        .filter(|b| b["type"] == "risk")
        .filter(|b| b["payload"]["rank"].is_null())
        .map(|b| b["payload"]["cell_id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let want_excluded: Vec<&str> = golden["insufficient_quality"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(excluded, want_excluded, "celdas fuera del ranking");
}

#[test]
fn two_builds_with_the_same_key_and_issued_at_are_identical() {
    let workspace = Workspace::new();
    assert!(workspace.build().status.success());

    let second = workspace._root.path().join("out/v2");
    assert!(workspace.build_into(&second, "v2").status.success());

    let first = payloads(&workspace.out_dir);
    let again = payloads(&second);
    for (relative, block) in &first {
        // Mismo cuerpo firmado -> mismo id y misma firma. Determinismo byte a
        // byte, que es lo que permite recalcular un bundle meses después.
        assert_eq!(block["id"], again[relative]["id"], "{relative}");
        assert_eq!(
            block["signature"], again[relative]["signature"],
            "{relative}"
        );
    }
}

// --- (b) PRUEBA CRUZADA con el verificador Python -------------------------

/// El bundle construido por Rust tiene que pasar también `verify_bundle.py`.
///
/// Es el chequeo que da valor al fixture: dos implementaciones independientes
/// sobre los mismos bytes. Si el entorno no tiene Python con `jsonschema`,
/// `referencing` y `cryptography`, el test **no falla y lo dice**: imprime una
/// línea `SKIP` (visible con `cargo test -- --nocapture`). No se usa
/// `#[ignore]` porque un ignore no deja rastro de por qué no se corrió.
#[test]
fn the_built_bundle_also_passes_the_python_verifier() {
    let Some(python) = python_with_dependencies() else {
        println!(
            "SKIP the_built_bundle_also_passes_the_python_verifier: \
             no hay Python con jsonschema + referencing + cryptography en este entorno. \
             La verificación cruzada no se ejecutó."
        );
        return;
    };

    let workspace = Workspace::new();
    let built = workspace.build();
    assert!(built.status.success(), "build falló: {}", stderr(&built));

    let output = run(Command::new(&python)
        .current_dir(repo_root())
        .arg("-I")
        .arg("contracts/v1/tools/verify_bundle.py")
        .arg(&workspace.out_dir));
    let report = stdout(&output);

    assert!(
        output.status.success(),
        "verify_bundle.py rechazó el bundle construido por Rust:\n{report}{}",
        stderr(&output)
    );
    assert!(report.contains("21 bloques"), "{report}");
    assert!(report.contains("0 error(es)"), "{report}");
}

/// El primer intérprete que tenga las tres dependencias del verificador.
fn python_with_dependencies() -> Option<PathBuf> {
    for candidate in ["python", "python3"] {
        let probe = Command::new(candidate)
            .arg("-I")
            .arg("-c")
            .arg("import jsonschema, referencing, cryptography")
            .output();
        if probe.is_ok_and(|out| out.status.success()) {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

// --- (c) RECHAZOS --------------------------------------------------------

#[test]
fn a_key_outside_trusted_issuers_cannot_build() {
    let workspace = Workspace::new();
    // Una lista que no incluye a nuestra clave.
    write(
        &workspace.config_dir.join("trusted-issuers.json"),
        "{\n  \"version\": 1,\n  \"issuers\": []\n}\n",
    );

    let output = workspace.build();
    assert!(
        !output.status.success(),
        "debía rechazar: {:?}",
        stdout(&output)
    );
    let message = stderr(&output);
    assert!(message.contains("trusted-issuers.json"), "{message}");
    assert!(message.contains(&workspace.issuer), "{message}");
    assert!(
        !workspace.out_dir.join("index.json").exists(),
        "no debe quedar nada escrito tras un rechazo"
    );
}

#[test]
fn building_over_a_published_version_is_refused_by_r28() {
    let workspace = Workspace::new();
    assert!(workspace.build().status.success());

    let again = workspace.build();
    assert!(!again.status.success(), "R28 debía rechazar la reescritura");
    let message = stderr(&again);
    assert!(message.contains("R28"), "{message}");
    assert!(message.contains("v1"), "{message}");
}

#[test]
fn a_non_comparable_pair_declared_real_is_refused_by_r1() {
    let workspace = Workspace::with_manifest(|raw| {
        raw.replace("\"comparable\": true", "\"comparable\": false")
            .replace(
                "\"data_origin\": \"simulated\"",
                "\"data_origin\": \"nisar_real\"",
            )
    });

    let output = workspace.build();
    assert!(!output.status.success(), "R1 debía rechazar el par");
    let message = stderr(&output);
    assert!(message.contains("R1"), "{message}");
    assert!(message.contains("comparable"), "{message}");
}

#[test]
fn a_version_that_is_not_vn_is_refused_before_writing() {
    let workspace = Workspace::new();
    let out = workspace._root.path().join("out/mala");
    let output = workspace.build_into(&out, "latest");

    assert!(!output.status.success());
    assert!(stderr(&output).contains("^v[0-9]+$"), "{}", stderr(&output));
    assert!(!out.exists(), "no debe crear el directorio de salida");
}

// --- (d) keygen dentro de un repo git ------------------------------------

#[test]
fn keygen_refuses_to_write_a_key_inside_a_git_repository() {
    // Dentro de este propio repo: es exactamente el accidente que R19 previene.
    let inside = repo_root().join("target/no-debe-existir.key");
    let _ = std::fs::remove_file(&inside);

    let output = run(wombat().arg("keygen").arg("--out").arg(&inside));

    assert!(!output.status.success(), "debía negarse");
    let message = stderr(&output);
    assert!(message.contains("R19"), "{message}");
    assert!(
        !inside.exists(),
        "no debe escribir la clave antes de negarse"
    );
}

#[test]
fn keygen_writes_a_usable_key_outside_any_repository() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("demo.key");

    let output = run(wombat().arg("keygen").arg("--out").arg(&path));
    assert!(output.status.success(), "{}", stderr(&output));

    let report = stdout(&output);
    let issuer = issuer_from_keygen(&report);
    assert_eq!(issuer.len(), "ed25519:".len() + 64, "{issuer}");
    // La línea lista para pegar trae los cuatro roles.
    assert!(report.contains("\"roles\": [\"change\", \"context\", \"risk\", \"action\"]"));
    assert!(path.exists());
}

// --- (e) TAMPER ----------------------------------------------------------

#[test]
fn flipping_one_byte_of_a_bundle_file_makes_verify_fail_naming_it() {
    let workspace = Workspace::new();
    assert!(workspace.build().status.success());
    assert!(
        workspace.verify().status.success(),
        "debía verificar limpio"
    );

    // Un byte de las celdas. No se toca el JSON para que siga siendo válido:
    // lo que tiene que saltar es el hash, no el parser.
    let cells = workspace.out_dir.join("cells.geojson");
    let mut bytes = std::fs::read(&cells).unwrap();
    let last = bytes.len() - 1;
    bytes[last] = if bytes[last] == b'\n' { b' ' } else { b'\n' };
    std::fs::write(&cells, &bytes).unwrap();

    let output = workspace.verify();
    let report = stdout(&output);

    assert!(!output.status.success(), "un byte cambiado debe fallar");
    assert!(
        report.contains("cells.geojson"),
        "el error debe nombrar el archivo:\n{report}"
    );
    assert!(report.contains("sha256 distinto"), "{report}");
    // Y además el bloque Change firmado lo delata, porque repite ese hash.
    assert!(
        report.contains("no coincide con el bloque change"),
        "el Change firmado debe delatar el cambio:\n{report}"
    );
    // El resumen va al final, después de una línea por error.
    assert!(summary_line(&report).starts_with("FALLA"), "{report}");
    assert!(summary_line(&report).contains("2 error(es)"), "{report}");
}

#[test]
fn flipping_one_byte_of_a_signed_block_makes_verify_fail_naming_it() {
    let workspace = Workspace::new();
    assert!(workspace.build().status.success());

    // Un dígito del score de un bloque Risk: el payload sigue siendo válido,
    // pero el id ya no es el hash de su cuerpo.
    let block_path = workspace.out_dir.join("wonb/risk/piura-demo-r02c01.json");
    let original = std::fs::read_to_string(&block_path).unwrap();
    let tampered = original.replace("\"score_bp\": 7636", "\"score_bp\": 9999");
    assert_ne!(original, tampered, "el score esperado cambió de forma");
    std::fs::write(&block_path, &tampered).unwrap();

    let output = workspace.verify();
    let report = stdout(&output);

    assert!(!output.status.success());
    assert!(
        report.contains("piura-demo-r02c01"),
        "el error debe nombrar el bloque:\n{report}"
    );
}

// --- (f) CONFIANZA EXTERNA ----------------------------------------------

#[test]
fn a_bundle_signed_by_another_key_is_rejected_against_the_real_trust_list() {
    // Dos bancos independientes: cada uno genera su propia clave con `keygen`,
    // así que firman con claves distintas. Es el caso que importa: un bundle
    // perfectamente bien construido, con todos sus hashes y firmas en orden,
    // pero emitido por alguien que no está en nuestra lista.
    let nuestro = Workspace::new();
    let ajeno = Workspace::new();
    assert_ne!(
        nuestro.issuer, ajeno.issuer,
        "keygen debería dar dos claves distintas"
    );

    assert!(
        ajeno.build().status.success(),
        "el bundle ajeno se construye"
    );
    assert!(
        ajeno.verify().status.success(),
        "y verifica contra su propia lista"
    );

    // Mismo bundle, nuestra lista.
    let output = ajeno.verify_against(&nuestro.config_dir.join("trusted-issuers.json"));
    let report = stdout(&output);

    assert!(
        !output.status.success(),
        "un emisor fuera de la lista debe fallar:
{report}"
    );
    assert!(summary_line(&report).starts_with("FALLA"), "{report}");
    // Los 21 bloques caen: la firma es válida, el firmante no es de confianza.
    assert!(
        summary_line(&report).contains("21 error(es)"),
        "deberían caer los 21 bloques:
{report}"
    );
}

#[test]
fn swapping_the_trust_list_inside_the_bundle_does_not_buy_a_pass() {
    // El ataque que `--trusted` cierra: quien pueda escribir en el directorio
    // del bundle reemplaza su `trusted-issuers.json` por uno que incluya su
    // propia clave. Antes eso alcanzaba, porque el verificador leía esa lista.
    let nuestro = Workspace::new();
    let atacante = Workspace::new();
    assert!(atacante.build().status.success());

    // El atacante se declara de confianza dentro de su propio bundle.
    let dentro = atacante.out_dir.join("trusted-issuers.json");
    std::fs::copy(atacante.config_dir.join("trusted-issuers.json"), &dentro).unwrap();

    let output = atacante.verify_against(&nuestro.config_dir.join("trusted-issuers.json"));
    let report = stdout(&output);

    assert!(
        !output.status.success(),
        "la lista de dentro del bundle no debe decidir nada:
{report}"
    );
}

#[test]
fn verify_without_a_trust_list_fails_naming_the_missing_file() {
    let workspace = Workspace::new();
    assert!(workspace.build().status.success());

    let ausente = workspace.config_dir.join("no-existe.json");
    let output = workspace.verify_against(&ausente);

    assert!(
        !output.status.success(),
        "sin lista de confianza no hay verificación posible"
    );
    let salida = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        salida.contains("no-existe.json"),
        "el error debe nombrar el archivo que falta:
{salida}"
    );
}
