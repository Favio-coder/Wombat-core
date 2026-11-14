# WOMBAT — Protocolo de intercambio de bloques de evidencia (WONB)

WOMBAT (Wombat Observation and Monitoring Blockchain-style Transport) es un
prototipo de protocolo P2P para intercambiar bloques JSON de evidencia llamados
**WONB** (*Wombat Observation Notation Block*) entre nodos heterogéneos.

El MVP usa **Rust + Tokio + libp2p** como nodo P2P, con **Python** para un futuro
adaptador geoespacial, **TypeScript/React** para el futuro dashboard y agente de
riesgo, y **Raspberry Pi** como futuro nodo edge.

> ⚠️ Estado: núcleo, contratos compartidos, modo P2P TCP/GossipSub local, pruebas y estructura de integración.
> No incluye dashboard completo, procesamiento SAR real, integración NISAR real ni
> GPIO de Raspberry. Los valores NISAR de los fixtures son **simulados para la
> demo**.

## ¿Qué es un WONB?

Un WONB es un bloque JSON de evidencia firmado que contiene:

- `spec_version`: versión del esquema (`wombat/0.1`).
- `id`: identificador único (`wonb:<tipo>:<uuid>`).
- `type`: `anomaly` | `context` | `risk` | `tool`.
- `schema`: esquema de dominio (ej. `geospatial.surface-displacement/1.0`).
- `source`: proveedor, adaptador y referencia de origen.
- `payload`: datos del bloque (objeto JSON arbitrario).
- `provenance`: cuándo, por quién, confianza, hash SHA-256 y versión de regla.
- `routing`: topic, TTL y prioridad.
- `evidence_refs`: referencias a evidencia externa.
- `signature`: firma Ed25519 (hex) sobre el hash canónico del contenido.

## Arquitectura breve

```
crates/wombat-core   -> Modelo WONB, validación, hash SHA-256, firmas Ed25519
crates/wombat-node   -> Nodo libp2p + GossipSub, CLI (run/publish/validate)
contracts/schemas    -> Esquemas JSON de los 4 tipos de WONB
contracts/fixtures   -> Ejemplos válidos e inválidos para pruebas y demo
adapters/*           -> Futuro adaptador geoespacial Python y nodo edge Pi
apps/*               -> Futuro dashboard y landing
docs/*               -> Especificación WONB, arquitectura, guía de demo
```

P2P es transporte; A2A es coordinación semántica mediante WONBs. MCP o
adaptadores conectarán fuentes y herramientas locales (no implementados todavía).

## Requisitos

- Rust toolchain estable (1.88+ para el modo P2P libp2p 0.57.1).
- `cargo` y `clang`/`gcc` (libp2p compila código nativo).
- Opcional: Docker y Docker Compose para la demo local.

## Comandos exactos

Desde la raíz del workspace (`wombat/`):

```bash
# Formato
cargo fmt --check

# Lint (clippy, errores como advertencias)
cargo clippy --workspace -- -D warnings

# Pruebas automatizadas (criterio de aceptación #1)
cargo test --workspace

# Validar también transporte TCP/GossipSub opcional
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features

# Construir el nodo
cargo build --workspace

# Validar el fixture de anomalía (criterio de aceptación #3)
cargo run -p wombat-node -- validate --file \
  contracts/fixtures/anomaly-valid.json

# Publicar el fixture de anomalía (modo demo, sin red)
cargo run -p wombat-node -- publish --file \
  contracts/fixtures/anomaly-valid.json
```

## Verificar todo de una vez

`scripts/verify_all.py` corre las tres puertas de calidad, los tres verificadores
de bundle (Rust, Python, JS), el hash de cada archivo del `index.json` y la
higiene del árbol versionado, y lo resume en una tabla ✔/✘ con tiempos. Solo
biblioteca estándar: no hay nada que instalar, y corre igual en Windows y Linux.
Sale con 1 si algo falla; un paso saltado se marca ⊘ y **nunca** cuenta como ✔.

```bash
python scripts/verify_all.py            # todo
python scripts/verify_all.py --quick    # sin cargo fmt, clippy ni test
```

## Levantar dos nodos locales

El nodo conserva un **modo demo local sin red**. El modo P2P real requiere
compilar con `--features p2p`, TCP en loopback y una dirección del peer explícita;
no usa mDNS, DNS, WebSocket, QUIC ni servicios externos.

1. Terminal 1 — nodo A en modo demo:
   ```bash
   cargo run -p wombat-node -- run --demo
   ```

2. Terminal 2 — nodo B, valida el fixture:
   ```bash
   cargo run -p wombat-node -- validate --file \
     contracts/fixtures/anomaly-valid.json
   ```

3. El subcomando `publish` solo valida e imprime el bloque localmente; no
   transmite porque no comparte el Swarm de un proceso `run`.

### Dos nodos reales en loopback

Primero inicia el publicador A. Anota el PeerId y la multiaddr que imprime; el
proceso esperará a que un peer remoto se suscriba al topic antes de publicar:

```bash
cargo run -p wombat-node --features p2p -- run \
  --listen /ip4/127.0.0.1/tcp/9000 \
  --topic wombat/pe/piura/piura-demo/change \
  --trusted-issuers contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json \
  --publish contracts/v1/fixtures/bundle/piura-demo/v0/wonb/change.json \
  --timeout-secs 60
```

En una segunda terminal inicia B. Reemplaza `<PEER_ID_A>` por el PeerId impreso
por A:

```bash
cargo run -p wombat-node --features p2p -- run \
  --listen /ip4/127.0.0.1/tcp/9001 \
  --peer /ip4/127.0.0.1/tcp/9000/p2p/<PEER_ID_A> \
  --topic wombat/pe/piura/piura-demo/change \
  --trusted-issuers contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json \
  --timeout-secs 60
```

La demo se considera exitosa cuando B registra recepción del mismo ID WONB,
topic y peer de origen. GossipSub no almacena mensajes históricos ni ofrece ACK
de aplicación: el resultado solo puede verificarse mientras B está conectado y
antes del timeout. Ambos nodos validan el contrato WONB 0.2, la firma, el emisor
confiable, TTL y topic; A publica únicamente después del evento de suscripción
remota correspondiente.

Para validar el fixture de anomalía localmente:

```bash
cargo run -p wombat-node -- publish --file \
  contracts/fixtures/anomaly-valid.json
```

Este comando es una validación local y **no publica en la red**.
Consulta `docs/demo-runbook.md` para la guía detallada.

## Contratos y revisión

Los archivos bajo `contracts/schemas/` y `contracts/fixtures/` son **contratos
compartidos**. No modificarlos sin revisión del equipo (ver tabla de
responsables más abajo). Cualquier cambio de esquema debe ir acompañado de
actualización de fixtures y pruebas.

## Contribuir

1. Crea una rama desde `main`.
2. No modifiques `contracts/` sin revisión.
3. Ejecuta `cargo fmt --check`, `cargo clippy --workspace -- -D warnings` y
   `cargo test --workspace` antes de enviar el PR.
4. Abre un PR y espera la revisión del responsable del módulo.

## Tabla: módulo / responsable sugerido

| Módulo                        | Responsable sugerido |
|-------------------------------|----------------------|
| Core Rust / P2P               | Richard              |
| Adaptador geoespacial Python  | Gustavo              |
| Nodo edge Raspberry Pi        | Jelibeth             |
| Agente de riesgo / dashboard  | Mireya               |
| Landing / pitch / UI          | Ximena               |
| QA, integración, GitHub Actions, fixtures, modo demo | Jayo |

## Avisos importantes

- Una anomalía de deformación superficial **no confirma ni predice por sí sola**
  un deslizamiento de tierra.
- El MVP usa una **regla demostrativa**, no un modelo científico operacional.
- No incluir claves privadas ni secretos en el repositorio.

## Licencia

MIT. Véase `LICENSE`.
