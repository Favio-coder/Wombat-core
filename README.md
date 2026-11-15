# WOMBAT — El Niño Evidence Mesh

WOMBAT convierte cambios observados por **NISAR GCOV** en evidencia firmada: contrasta la señal con **OPERA DSWx-S1**, calcula exposición vial por celdas de **1 km × 1 km** y prioriza qué zona verificar primero. Cada resultado se expresa como bloques WONB verificables con Ed25519. El usuario objetivo es el analista del **Gobierno Regional de Piura (GORE Piura)**: esto sirve para **preparación, no para emitir alertas**.

> **Los datos de los fixtures son SIMULADOS.** El bundle `piura-demo/v0` no representa observaciones NISAR reales ni una evaluación científica operacional. Un cambio observado no confirma ni predice una inundación.

## Estado del proyecto

| IMPLEMENTADO | LEGADO | PLANIFICADO |
|---|---|---|
| `contracts/v1/`: contrato 1.0.0, bundle y los tres verificadores (Rust, Python, JS). | `wombat-node` implementa WONB **0.1**, un contrato distinto del vigente. | Pipeline geoespacial completo en `data-pipeline/`; hoy no hay insumos NISAR reales. |
| WONB 0.2 en `wombat-core` (`wonb-core` en el plan): JCS, hash por contenido, firma y verificación. | Su payload firmado lleva `confidence` como `f64`, lo que **viola R16**. | `wombat-gateway` REST/WS y modo `live`. |
| `wombat-cli`: `keygen`, `build` y `verify`. **R14: solo Rust firma.** | El transporte libp2p está detrás de `--features p2p` y **no se demuestra**. | Dashboard en el repositorio aparte `wombat-dashboard` (**R26**); modo `static` obligatorio (**R17**), `remote`/`live` opcionales. |
| `wombat-risk`: ranking determinista con prueba de ranking dorado. | `publish` valida e imprime el WONB; sin esa feature **no transmite**. | Edge Raspberry/ESP32 como P2; no hay hardware ni conexión implementados. |
| Fixture firmado y **simulado** `contracts/v1/fixtures/bundle/piura-demo/v0`. | Los esquemas y fixtures 0.1 siguen en CI mientras exista compatibilidad. |  |

WONB 0.1 y 0.2 son contratos distintos. El nodo actual continúa usando 0.1; el contrato v1 y el ranking usan 0.2. Consulta [la documentación del contrato v1](contracts/v1/README.md) y [la especificación legada 0.1](docs/wonb-spec.md).

## Repositorios y componentes

- **Core:** este repositorio `wombat-core`, con contratos, Rust y el directorio `data-pipeline/` reservado al pipeline.
- **Dashboard:** repositorio separado `wombat-dashboard`; no vive en este core.
- **Pipeline:** `data-pipeline/` dentro de este repositorio.
- **Edge:** `edge/` cuando exista; el prototipo Raspberry anterior no forma parte del árbol actual.
- **Landing/pitch:** no hay una aplicación landing implementada en este repositorio.

## El flujo que se demuestra

Clave → bundle firmado → verificación cruzada. El detalle paso a paso está en el
[runbook](docs/demo-runbook.md).

```bash
# Las cuatro verificaciones y las tres puertas de calidad, en una tabla.
python scripts/verify_all.py

# 1. Clave Ed25519 fuera del repositorio (R19: el comando rechaza un destino versionado).
cargo run -p wombat-cli -- keygen --out ~/.wombat-keys/demo.key --name wombat-core-demo

# 2. Bundle firmado e inmutable por versión (R28). Falla antes de escribir si
#    las escenas no son comparables (R6/R7).
cargo run -p wombat-cli -- build --in data-pipeline/out/piura-01   --aoi piura-01 --version v1 --key ~/.wombat-keys/demo.key --out data/piura-01/v1

# 3. Verificar: sha256, id por contenido, firma, emisor, TTL y cadena.
#    --trusted es la lista de emisores; el defecto es config/trusted-issuers.json
#    y nunca la que viene dentro del bundle. El fixture lo firmó una clave de
#    desarrollo que a propósito no está en la confianza de producción.
cargo run -p wombat-cli -- verify contracts/v1/fixtures/bundle/piura-demo/v0   --trusted contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json
```

Las tres implementaciones del verificador —Rust, `contracts/v1/tools/verify_bundle.py`
y `contracts/v1/tools/verify-js/verify.mjs`— leen el mismo bundle y deben
coincidir. Sobre el fixture dan **21 bloques, 0 con error**.

No comprueban lo mismo, y conviene no venderlo así: solo `wombat verify` mide la
confianza contra una lista externa al bundle, y solo Rust y Python comprueban la
integridad de los archivos contra `index.json`. JS y `verify-rs` comprueban `id`
y firma contra la clave que el propio bloque declara. El
[runbook](docs/demo-runbook.md#las-tres-no-comprueban-lo-mismo) tiene la tabla.

El nodo legado 0.1 (`validate`, `publish`, `--features p2p`) no participa de este
flujo; está documentado en el [runbook](docs/demo-runbook.md#wonb-01-y-el-nodo-p2p--legado).

## Arquitectura objetivo

El flujo planificado es **NISAR GCOV → cambio observado → contraste OPERA → exposición vial por celda → ranking Rust → bloques WONB firmados → bundle → dashboard**. El contrato de datos es la interfaz entre `wombat-core` y `wombat-dashboard`; el navegador no habla libp2p.

Los modos del dashboard descritos por el plan son:

- **static:** bundle fijado dentro del build del dashboard; no depende de un backend.
- **remote:** bundle publicado en GitHub Pages, cargado desde el repositorio core.
- **live:** gateway REST/WS para estado y bloques recientes.

El dashboard y el gateway que consumirían esos modos aún están planificados. El flujo existente de CI valida el fixture del contrato; eso no significa que haya una aplicación dashboard disponible.

Más detalles: [arquitectura](docs/architecture.md), [runbook](docs/demo-runbook.md) y [contrato WONB 0.2](contracts/v1/README.md).

## Licencia

MIT. Véase [LICENSE](LICENSE).
