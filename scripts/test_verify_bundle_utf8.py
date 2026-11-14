#!/usr/bin/env python3
"""`verify_bundle.py` tiene que poder *reportar* un bundle roto, no solo detectarlo.

El verificador marca cada error con `✗`. En Windows, cuando su salida es una
tubería y no una consola, Python la abre en la página de códigos local (cp1252),
donde ese carácter no existe, y el `print` moría con `UnicodeEncodeError`. El
fallo estaba escondido detrás del caso feliz: mientras todo verifica no se
imprime ninguna `✗`, así que el verificador parecía sano y solo se rompía
cuando hacía falta que hablara.

Esta prueba rompe un bundle a propósito y exige tres cosas:

1. código de salida 1,
2. ningún `UnicodeEncodeError` ni traza de Python,
3. el informe `FALLA` con el nombre del archivo alterado.

Y lo exige **sin** `-X utf8` ni `PYTHONIOENCODING`: si el arreglo dependiera de
cómo lo invoca quien llama, no sería un arreglo del verificador.

    python scripts/test_verify_bundle_utf8.py
    python -m unittest discover -s scripts -p 'test_*.py'
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "contracts/v1/tools/verify_bundle.py"
FIXTURE = ROOT / "contracts/v1/fixtures/bundle/piura-demo/v0"


def dependencies_available() -> bool:
    """Las del verificador, que no son stdlib."""
    probe = subprocess.run(
        [sys.executable, "-I", "-c", "import jsonschema, referencing, cryptography"],
        capture_output=True,
    )
    return probe.returncode == 0


def hostile_environment() -> dict[str, str]:
    """Un entorno que empuja a Python a la codificación más estrecha posible.

    El objetivo es provocar el fallo, no evitarlo: sin esto, en Linux la tubería
    sale en UTF-8 y la prueba no probaría nada. Ninguna de estas variables es
    `PYTHONIOENCODING` ni equivale a `-X utf8`; todas van en la dirección
    contraria, a hacer el entorno más hostil.
    """
    env = dict(os.environ)
    env.pop("PYTHONIOENCODING", None)
    env.pop("PYTHONUTF8", None)
    if os.name == "nt":
        # Fuerza el stdio heredero basado en la página de códigos, también para
        # tuberías: es el escenario exacto en el que apareció el fallo.
        env["PYTHONLEGACYWINDOWSSTDIO"] = "1"
    else:
        # Locale C y sin coerción (PEP 538), para que la codificación preferida
        # sea ASCII y `✗` no quepa.
        env["LC_ALL"] = "C"
        env["LANG"] = "C"
        env["PYTHONCOERCECLOCALE"] = "0"
    return env


class VerifyBundleReportsBrokenBundles(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if not VERIFIER.is_file():
            raise unittest.SkipTest(f"no está {VERIFIER.relative_to(ROOT).as_posix()}")
        if not (FIXTURE / "index.json").is_file():
            raise unittest.SkipTest("no está el bundle fixture piura-demo/v0")
        if not dependencies_available():
            raise unittest.SkipTest(
                "faltan dependencias del verificador: "
                "pip install jsonschema referencing cryptography"
            )

    def broken_bundle(self) -> Path:
        """Una copia temporal del fixture con un byte distinto en cells.geojson.

        Se cambia el salto de línea final por un espacio: los bytes son otros
        —y por lo tanto el sha256 también— pero el JSON sigue siendo válido, así
        que los únicos errores son los dos hashes que no cuadran (el del
        `index.json` y el que repite el bloque Change firmado). Si además
        rompiéramos el JSON, el verificador fallaría al parsear y no llegaría a
        imprimir las `✗`, que es justo lo que queremos ejercitar.
        """
        temporary = Path(tempfile.mkdtemp(prefix="wombat-broken-"))
        self.addCleanup(shutil.rmtree, temporary, ignore_errors=True)
        bundle = temporary / "v0"
        shutil.copytree(FIXTURE, bundle)

        cells = bundle / "cells.geojson"
        data = bytearray(cells.read_bytes())
        self.assertEqual(data[-1], ord("\n"), "se esperaba un salto de línea final")
        data[-1] = ord(" ")
        cells.write_bytes(bytes(data))
        return bundle

    def run_verifier(self, bundle: Path, env: dict[str, str]) -> subprocess.CompletedProcess:
        """Invoca el verificador tal como lo haría una persona: sin flags de rescate."""
        return subprocess.run(
            [sys.executable, str(VERIFIER), str(bundle)],
            cwd=str(ROOT),
            capture_output=True,
            env=env,
            # Se decodifica aquí con utf-8 para poder leer el informe; lo que se
            # está probando es que el hijo pudo *escribirlo*.
            text=True,
            encoding="utf-8",
            errors="replace",
        )

    def assert_reported_the_failure(self, done: subprocess.CompletedProcess) -> None:
        output = (done.stdout or "") + (done.stderr or "")
        self.assertNotIn("UnicodeEncodeError", output, f"el verificador murió al reportar:\n{output}")
        self.assertNotIn("Traceback", output, f"el verificador lanzó una excepción:\n{output}")
        self.assertEqual(done.returncode, 1, f"se esperaba código 1:\n{output}")
        self.assertIn("FALLA", output, f"falta el informe de fallo:\n{output}")
        self.assertIn("cells.geojson", output, f"el informe debe nombrar el archivo:\n{output}")

    def test_a_broken_bundle_is_reported_through_a_plain_pipe(self) -> None:
        """Tubería normal, entorno sin tocar. En Windows esto ya es cp1252."""
        env = dict(os.environ)
        env.pop("PYTHONIOENCODING", None)
        env.pop("PYTHONUTF8", None)
        self.assert_reported_the_failure(self.run_verifier(self.broken_bundle(), env))

    def test_a_broken_bundle_is_reported_in_a_hostile_locale(self) -> None:
        """Codificación lo más estrecha que el sistema permita."""
        self.assert_reported_the_failure(
            self.run_verifier(self.broken_bundle(), hostile_environment())
        )

    def test_a_broken_bundle_is_reported_when_stdout_is_a_file(self) -> None:
        """Salida redirigida a archivo, que es el otro caso sin consola."""
        bundle = self.broken_bundle()
        destination = bundle.parent / "salida.txt"
        with destination.open("wb") as handle:
            done = subprocess.run(
                [sys.executable, str(VERIFIER), str(bundle)],
                cwd=str(ROOT),
                stdout=handle,
                stderr=subprocess.STDOUT,
                env=hostile_environment(),
            )
        output = destination.read_text(encoding="utf-8", errors="replace")
        self.assertNotIn("UnicodeEncodeError", output, output)
        self.assertEqual(done.returncode, 1, output)
        self.assertIn("FALLA", output, output)

    def test_the_intact_fixture_still_verifies(self) -> None:
        """El arreglo no cambia el caso feliz: 21 bloques, 0 errores, código 0."""
        done = self.run_verifier(FIXTURE, hostile_environment())
        output = (done.stdout or "") + (done.stderr or "")
        self.assertEqual(done.returncode, 0, output)
        self.assertIn("21 bloques", output)
        self.assertIn("0 error(es)", output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
