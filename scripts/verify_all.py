#!/usr/bin/env python3
"""Pasa todo WOMBAT por la máquina de verdad y lo resume en una tabla.

Hay cuatro verificadores en este repositorio (Rust, Python, JS y el hash de
bytes) y tres puertas de calidad (fmt, clippy, test). Correrlos a mano significa
recordar siete comandos y acordarse de mirar los códigos de salida. Este script
los corre todos, dice qué pasó con cada uno y falla si algo falla.

Solo biblioteca estándar: tiene que correr en la máquina de cualquiera, sin
`pip install`, en Windows y en Linux.

    python scripts/verify_all.py            # todo
    python scripts/verify_all.py --quick    # sin fmt, clippy ni test

Sale con 0 solo si no hubo ningún ✘. Un ⊘ (saltado) no es un fallo, pero
**nunca** se cuenta como éxito: si el verificador JS no está instalado, lo dice
y explica cómo instalarlo, en vez de dar un visto bueno que nadie se ganó.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

# La raíz del repositorio, deducida de dónde vive este archivo. Así el script
# funciona igual invocado desde la raíz, desde scripts/ o por ruta absoluta.
ROOT = Path(__file__).resolve().parents[1]

#: Nada versionado debería pesar más que esto. Los crudos van al pipeline, no al repo.
MAX_FILE_BYTES = 5 * 1024 * 1024

#: Extensiones de dato crudo que no entran al árbol versionado.
RAW_SUFFIXES = (".h5", ".hdf5", ".he5")

#: Lo que delata una clave privada en el árbol (R19).
KEY_SUFFIXES = (".key", ".pem")

JS_DIR = Path("contracts/v1/tools/verify-js")
JS_INSTALL = "cd contracts/v1/tools/verify-js && npm ci"

OK, FAIL, SKIP = "ok", "fail", "skip"


def pick_marks() -> dict[str, str]:
    """Los símbolos de la tabla, degradando a ASCII si la consola no da.

    La consola de Windows arranca a veces en cp1252, donde `✔` no existe y un
    `print` directo levanta UnicodeEncodeError. Se intenta pasar la salida a
    UTF-8 y, si no se puede, se usan marcas ASCII: una tabla fea se lee, una
    excepción no.
    """
    try:
        sys.stdout.reconfigure(encoding="utf-8")  # type: ignore[union-attr]
    except Exception:
        pass
    fancy = {OK: "✔", FAIL: "✘", SKIP: "⊘"}
    encoding = sys.stdout.encoding or "ascii"
    try:
        "".join(fancy.values()).encode(encoding)
        return fancy
    except (UnicodeEncodeError, LookupError):
        return {OK: "OK", FAIL: "XX", SKIP: "--"}


MARKS = pick_marks()


class Row:
    """Una fila de la tabla: qué se comprobó, cómo salió y cuánto tardó."""

    def __init__(self, status: str, step: str, seconds: float | None, detail: str = ""):
        self.status = status
        self.step = step
        self.seconds = seconds
        self.detail = detail


class Report:
    def __init__(self) -> None:
        self.rows: list[Row] = []

    def add(self, status: str, step: str, seconds: float | None, detail: str = "") -> None:
        self.rows.append(Row(status, step, seconds, detail))

    def failed(self) -> bool:
        return any(row.status == FAIL for row in self.rows)

    def print(self, total_seconds: float) -> None:
        width = max((len(row.step) for row in self.rows), default=0)
        print()
        for row in self.rows:
            mark = MARKS[row.status]
            timing = f"{row.seconds:6.1f}s" if row.seconds is not None else "      -"
            line = f"  {mark}  {row.step.ljust(width)}  {timing}"
            if row.detail:
                line += f"  {row.detail}"
            print(line)

        counts = {status: 0 for status in (OK, FAIL, SKIP)}
        for row in self.rows:
            counts[row.status] += 1
        print()
        print(
            f"  {len(self.rows)} pasos: "
            f"{counts[OK]} {MARKS[OK]}, {counts[FAIL]} {MARKS[FAIL]}, {counts[SKIP]} {MARKS[SKIP]}"
            f"   (total {total_seconds:.1f}s)"
        )


def run(command: list[str], cwd: Path | None = None) -> tuple[int, str]:
    """Ejecuta un comando y devuelve (código, salida combinada).

    `encoding` explícito al decodificar, por Windows: las salidas de los
    verificadores traen `✗`, `≥` y `…`, y la decodificación por defecto las
    rompe. Lo simétrico —que el hijo pueda *escribirlas*— se resuelve donde se
    invoca cada uno, porque depende del intérprete.
    """
    try:
        done = subprocess.run(
            command,
            cwd=str(cwd or ROOT),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except FileNotFoundError:
        return 127, f"no se encontró el ejecutable: {command[0]}"
    except OSError as err:
        return 126, f"no se pudo ejecutar {command[0]}: {err}"
    return done.returncode, (done.stdout or "") + (done.stderr or "")


def timed(command: list[str], cwd: Path | None = None) -> tuple[int, str, float]:
    start = time.perf_counter()
    code, output = run(command, cwd)
    return code, output, time.perf_counter() - start


def last_line(output: str) -> str:
    lines = [line for line in output.strip().splitlines() if line.strip()]
    return lines[-1].strip() if lines else ""


def blocks_of(output: str) -> str:
    """El `N bloques` de la salida de un verificador, para la columna detalle."""
    found = re.search(r"(\d+)\s+bloques", output)
    return f"{found.group(1)} bloques" if found else last_line(output)[:60]


# --- 1. Puertas de calidad de Rust ---------------------------------------


def check_cargo(report: Report) -> None:
    """fmt, clippy y test. Son los tres que `--quick` saltea."""
    if shutil.which("cargo") is None:
        for step in ("cargo fmt --check", "cargo clippy -D warnings", "cargo test --workspace"):
            report.add(SKIP, step, None, "cargo no está en PATH")
        return

    gates = [
        ("cargo fmt --check", ["cargo", "fmt", "--all", "--", "--check"]),
        (
            "cargo clippy -D warnings",
            ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ),
        ("cargo test --workspace", ["cargo", "test", "--workspace"]),
    ]
    for step, command in gates:
        code, output, seconds = timed(command)
        if code == 0:
            detail = ""
            if step.endswith("--workspace"):
                passed = sum(int(n) for n in re.findall(r"(\d+) passed", output))
                detail = f"{passed} tests"
            report.add(OK, step, seconds, detail)
        else:
            report.add(FAIL, step, seconds, last_line(output)[:70])


# --- 2. Los bundles ------------------------------------------------------


def find_bundles() -> list[Path]:
    """Todo bundle del repo: los fixtures y, si existe, lo publicado.

    Un bundle es un directorio con `index.json`: es el archivo que lo declara.
    """
    candidates: list[Path] = []
    for pattern in ("contracts/v1/fixtures/bundle/*/*", "data/*/*"):
        candidates.extend(sorted(ROOT.glob(pattern)))
    return [d for d in candidates if (d / "index.json").is_file()]


def wombat_command() -> tuple[list[str], str]:
    """Cómo invocar el CLI, y una nota de dónde salió.

    Se prefiere un binario ya compilado porque es instantáneo. Si no hay, se
    cae a `cargo run`, que compila la primera vez y por eso tarda.
    """
    exe = "wombat.exe" if os.name == "nt" else "wombat"
    for profile in ("release", "debug"):
        candidate = ROOT / "target" / profile / exe
        if candidate.is_file():
            return [str(candidate)], f"target/{profile}"
    return ["cargo", "run", "-q", "-p", "wombat-cli", "--"], "cargo run"


def check_wombat_verify(report: Report, bundles: list[Path]) -> None:
    command, origin = wombat_command()
    if command[0] == "cargo" and shutil.which("cargo") is None:
        for bundle in bundles:
            report.add(SKIP, f"wombat verify  {rel(bundle)}", None, "sin binario ni cargo")
        return

    for bundle in bundles:
        code, output, seconds = timed([*command, "verify", str(bundle)])
        step = f"wombat verify  {rel(bundle)}"
        if code == 0:
            report.add(OK, step, seconds, f"{blocks_of(output)} ({origin})")
        else:
            report.add(FAIL, step, seconds, last_line(output)[:70])


def check_python_verifier(report: Report, bundles: list[Path]) -> None:
    script = ROOT / "contracts/v1/tools/verify_bundle.py"
    if not script.is_file():
        report.add(SKIP, "verify_bundle.py", None, "no está en contracts/v1/tools/")
        return

    # Sus dependencias no son stdlib: se comprueban antes para poder distinguir
    # "el bundle está mal" de "falta instalar jsonschema".
    probe, _ = run([sys.executable, "-I", "-c", "import jsonschema, referencing, cryptography"])
    if probe != 0:
        for bundle in bundles:
            report.add(
                SKIP,
                f"verify_bundle.py  {rel(bundle)}",
                None,
                "faltan dependencias: pip install jsonschema referencing cryptography",
            )
        return

    for bundle in bundles:
        # Sin `-X utf8`: el verificador ya fuerza UTF-8 en su propia salida, así
        # que no depende de cómo lo invoque quien lo llama. No lo repongas acá;
        # si vuelve el UnicodeEncodeError, el arreglo va en verify_bundle.py y la
        # prueba que lo cubre es scripts/test_verify_bundle_utf8.py.
        code, output, seconds = timed([sys.executable, "-I", str(script), str(bundle)])
        step = f"verify_bundle.py  {rel(bundle)}"
        if code == 0:
            report.add(OK, step, seconds, blocks_of(output))
        else:
            report.add(FAIL, step, seconds, last_line(output)[:70])


def check_js_verifier(report: Report, bundles: list[Path]) -> None:
    """El verificador JS, que solo corre si alguien instaló sus dependencias.

    Nunca se marca ✔ sin haber corrido. Un verificador ausente no es un
    verificador que aprueba.
    """
    script = ROOT / JS_DIR / "verify.mjs"
    modules = ROOT / JS_DIR / "node_modules"

    if shutil.which("node") is None:
        report.add(SKIP, "verify.mjs", None, "node no está en PATH; instalá Node.js 18+")
        return
    if not script.is_file():
        report.add(SKIP, "verify.mjs", None, f"no está en {JS_DIR.as_posix()}")
        return
    if not modules.is_dir():
        report.add(SKIP, "verify.mjs", None, f"node_modules no instalado: {JS_INSTALL}")
        return

    for bundle in bundles:
        code, output, seconds = timed(["node", str(script), str(bundle)])
        step = f"verify.mjs  {rel(bundle)}"
        if code == 0:
            report.add(OK, step, seconds, blocks_of(output))
        else:
            report.add(FAIL, step, seconds, last_line(output)[:70])


def check_bytes(report: Report, bundles: list[Path]) -> None:
    """Cada archivo del `index.json` contra su sha256 declarado.

    Es la comprobación que atrapa el CRLF. Si los bytes con `\\r\\n` fallan pero
    los mismos bytes con `\\n` dan el hash correcto, el archivo no está
    corrupto: lo normalizó git al hacer checkout, y lo que falta es una regla
    `-text` en `.gitattributes`. El mensaje lo dice, porque el síntoma
    ("sha256 distinto") no se parece en nada a la causa.
    """
    for bundle in bundles:
        start = time.perf_counter()
        step = f"bytes  {rel(bundle)}"
        try:
            index = json.loads((bundle / "index.json").read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as err:
            report.add(FAIL, step, time.perf_counter() - start, f"index.json ilegible: {err}")
            continue

        problems: list[str] = []
        entries = index.get("files", [])
        for entry in entries:
            path = bundle / entry["path"]
            if not path.is_file():
                problems.append(f"falta {entry['path']}")
                continue
            data = path.read_bytes()
            if hashlib.sha256(data).hexdigest() == entry["sha256"]:
                continue
            normalized = data.replace(b"\r\n", b"\n")
            if hashlib.sha256(normalized).hexdigest() == entry["sha256"]:
                problems.append(
                    f"{entry['path']}: CRLF en disco (con LF el hash coincide) "
                    f"-> falta una regla -text en .gitattributes"
                )
            else:
                problems.append(f"{entry['path']}: sha256 distinto")

        seconds = time.perf_counter() - start
        if problems:
            report.add(FAIL, step, seconds, f"{len(problems)} problema(s): {problems[0]}")
            for extra in problems[1:]:
                report.add(FAIL, f"bytes  {rel(bundle)}", None, extra)
        else:
            report.add(OK, step, seconds, f"{len(entries)} archivos")


# --- 3. Higiene del árbol versionado -------------------------------------


def versioned_tree() -> tuple[list[tuple[int, str]], str | None]:
    """(tamaño, ruta) de cada blob de HEAD, o un motivo para saltar.

    Se mira el árbol **versionado**, no el directorio de trabajo: lo que importa
    es qué se publicó, no qué quedó suelto en una carpeta local.
    """
    if shutil.which("git") is None:
        return [], "git no está en PATH"
    code, output = run(["git", "ls-tree", "-r", "-l", "HEAD"])
    if code != 0:
        return [], "no se pudo leer el árbol de HEAD (¿repositorio sin commits?)"

    blobs: list[tuple[int, str]] = []
    for line in output.splitlines():
        if "\t" not in line:
            continue
        meta, path = line.split("\t", 1)
        fields = meta.split()
        if len(fields) < 4 or fields[1] != "blob":
            continue
        try:
            blobs.append((int(fields[3]), path.strip()))
        except ValueError:
            continue
    return blobs, None


def check_hygiene(report: Report) -> None:
    start = time.perf_counter()
    blobs, reason = versioned_tree()
    elapsed = time.perf_counter() - start
    if reason:
        for step in ("higiene  crudos", "higiene  claves", "higiene  tamaño"):
            report.add(SKIP, step, None, reason)
        return

    raw = [path for _, path in blobs if path.lower().endswith(RAW_SUFFIXES)]
    keys = [
        path
        for _, path in blobs
        if path.lower().endswith(KEY_SUFFIXES) or "/.keys/" in f"/{path}"
    ]
    heavy = [(size, path) for size, path in blobs if size > MAX_FILE_BYTES]

    report.add(
        FAIL if raw else OK,
        "higiene  crudos",
        elapsed,
        f"{len(raw)}: {', '.join(raw[:3])}" if raw else f"0 de {len(blobs)} archivos",
    )
    report.add(
        FAIL if keys else OK,
        "higiene  claves",
        None,
        f"{len(keys)}: {', '.join(keys[:3])}" if keys else "ninguna (R19)",
    )
    if heavy:
        worst = max(heavy)
        detail = f"{len(heavy)} sobre 5 MB, el mayor {worst[1]} ({worst[0] / 1048576:.1f} MB)"
    else:
        biggest = max(blobs, default=(0, "-"))
        detail = f"mayor: {biggest[1]} ({biggest[0] / 1024:.0f} KB)"
    report.add(FAIL if heavy else OK, "higiene  tamaño", None, detail)


def rel(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Corre todas las verificaciones de WOMBAT y resume el resultado.",
        epilog="Sale con 1 si algo falla. Un paso saltado (⊘) no cuenta como éxito.",
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="salta cargo fmt, clippy y test (los tres pasos lentos)",
    )
    args = parser.parse_args()

    print(f"WOMBAT verify-all  —  {ROOT}")
    report = Report()
    started = time.perf_counter()

    if args.quick:
        for step in ("cargo fmt --check", "cargo clippy -D warnings", "cargo test --workspace"):
            report.add(SKIP, step, None, "--quick")
    else:
        check_cargo(report)

    bundles = find_bundles()
    if not bundles:
        report.add(SKIP, "bundles", None, "no se encontró ningún directorio con index.json")
    else:
        print(f"bundles encontrados: {', '.join(rel(b) for b in bundles)}")
        check_wombat_verify(report, bundles)
        check_python_verifier(report, bundles)
        check_js_verifier(report, bundles)
        check_bytes(report, bundles)

    check_hygiene(report)

    report.print(time.perf_counter() - started)
    return 1 if report.failed() else 0


if __name__ == "__main__":
    sys.exit(main())
