# WOMBAT — El Niño Evidence Mesh

WOMBAT convierte cambios observados por **NISAR GCOV** en evidencia firmada: contrasta la señal con **OPERA DSWx-S1**, calcula exposición vial por celdas de **1 km × 1 km** y prioriza qué zona verificar primero. Cada resultado se expresa como bloques WONB verificables con Ed25519. El usuario objetivo es el analista del **Gobierno Regional de Piura (GORE Piura)**: esto sirve para **preparación, no para emitir alertas**.

> **Los datos de los fixtures son SIMULADOS.** El bundle `piura-demo/v0` no representa observaciones NISAR reales ni una evaluación científica operacional. Un cambio observado no confirma ni predice una inundación.

## Estado del proyecto

| IMPLEMENTADO | LEGADO | PLANIFICADO |
|---|---|---|
| `contracts/v1/`: contrato, bundle y verificadores. | `wombat-node` todavía usa WONB 0.1. | `wombat-cli` para ingestión, firma y exportación. |
| WONB 0.2 en `wombat-core` (`wonb-core` en el plan): JCS, hash, firma y verificación. | El nodo funciona en modo demo; **no tiene transporte P2P real**. | `wombat-gateway` REST/WS. |
| `wombat-risk`: ranking determinista con prueba de ranking dorado. | `publish` valida e imprime el WONB; **no transmite**. | Pipeline geoespacial completo en `data-pipeline/`. |
| Fixture firmado y simulado `contracts/v1/fixtures/bundle/piura-demo/v0`. | Los esquemas y fixtures WONB 0.1 siguen en CI mientras exista compatibilidad. | Dashboard en el repositorio aparte `wombat-dashboard`; modo static obligatorio, remote/live como opciones. |
|  |  | Edge P2; hardware y conexión P2P no están implementados. |

WONB 0.1 y 0.2 son contratos distintos. El nodo actual continúa usando 0.1; el contrato v1 y el ranking usan 0.2. Consulta [la documentación del contrato v1](contracts/v1/README.md) y [la especificación legada 0.1](docs/wonb-spec.md).

## Repositorios y componentes

- **Core:** este repositorio `wombat-core`, con contratos, Rust y el directorio `data-pipeline/` reservado al pipeline.
- **Dashboard:** repositorio separado `wombat-dashboard`; no vive en este core.
- **Pipeline:** `data-pipeline/` dentro de este repositorio.
- **Edge:** `edge/` cuando exista; el prototipo Raspberry anterior no forma parte del árbol actual.
- **Landing/pitch:** no hay una aplicación landing implementada en este repositorio.

## Comprobar el estado Rust actual

Desde la raíz:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo run -p wombat-node -- validate --file contracts/fixtures/anomaly-valid.json
```

El CLI legado también acepta `publish`, pero solo valida e imprime el JSON; no publica por la red. No hay comunicación entre nodos en esta versión.

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
