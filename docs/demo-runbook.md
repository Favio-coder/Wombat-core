# Demo Runbook — WOMBAT

## Objetivo

Levantar el nodo WOMBAT en modo demo local o demostrar el transporte P2P
WONB 0.2 con dos nodos TCP sobre loopback y GossipSub.

## Requisitos

- Rust toolchain estable.
- Carpeta de trabajo: `wombat/`.
- Para P2P: Rust 1.88 o posterior; libp2p se activa opcionalmente con
  `--features p2p`.

## Comandos

### 1. Ejecutar pruebas

```bash
cargo test --workspace
cargo test --workspace --all-features
```

### 2. Verificar formato y lint

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --all-features -- -D warnings
```

### 3. Validar el fixture de anomalía (criterio de aceptación)

```bash
cargo run -p wombat-node -- validate --file \
  contracts/fixtures/anomaly-valid.json
```

Salida esperada: `WONB válido: wonb:anomaly:demo-001`.

### 4. Publicar el fixture de anomalía (modo demo)

```bash
cargo run -p wombat-node -- publish --file \
  contracts/fixtures/anomaly-valid.json
```

### 5. Rechazar un fixture vencido

```bash
cargo run -p wombat-node -- validate --file \
  contracts/fixtures/expired-invalid.json
```

Salida esperada: error (WONB expirado rechazado).

### 6. Rechazar un fixture malformado

```bash
cargo run -p wombat-node -- validate --file \
  contracts/fixtures/malformed-invalid.json
```

Salida esperada: error (campos obligatorios faltantes).

## Dos terminales (modo demo, sin red)

Terminal 1 — nodo A:

```bash
cargo run -p wombat-node -- run --demo
```

Terminal 2 — nodo B, validar y publicar:

```bash
cargo run -p wombat-node -- validate --file \
  contracts/fixtures/anomaly-valid.json
cargo run -p wombat-node -- publish --file \
  contracts/fixtures/anomaly-valid.json
```

`run --demo` no inicializa identidad libp2p ni abre sockets.

## Dos nodos P2P locales reales

La demo usa TCP en `127.0.0.1`, dial explícito y GossipSub; no depende de mDNS,
DNS, WiFi del evento ni servicios externos. Ambos procesos deben cargar la
misma lista de emisores confiables. El publicador valida el bloque antes de
emitirlo; el receptor valida JSON, firma, emisor, TTL, cadena y coincidencia
exacta del topic antes de aceptarlo.

Terminal 1 — A (publicador). Anota el PeerId y la multiaddr que imprime:

```bash
cargo run -p wombat-node --features p2p -- run \
  --listen /ip4/127.0.0.1/tcp/9000 \
  --topic wombat/pe/piura/piura-demo/change \
  --trusted-issuers contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json \
  --publish contracts/v1/fixtures/bundle/piura-demo/v0/wonb/change.json \
  --timeout-secs 60
```

Terminal 2 — B (receptor). Sustituye `<PEER_ID_A>` por el PeerId de A:

```bash
cargo run -p wombat-node --features p2p -- run \
  --listen /ip4/127.0.0.1/tcp/9001 \
  --peer /ip4/127.0.0.1/tcp/9000/p2p/<PEER_ID_A> \
  --topic wombat/pe/piura/piura-demo/change \
  --trusted-issuers contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json \
  --timeout-secs 60
```

Éxito: A registra la suscripción remota y la publicación; B registra el mismo
`wonb_id`, topic y peer de origen. GossipSub no mantiene historial ni provee un
ACK de aplicación, por lo que la recepción debe observarse en vivo antes del
timeout. A espera el evento de suscripción al topic y no usa sleeps arbitrarios.

El comando `publish --file` es solo validación/imprimir local; no transmite en
red, porque un proceso separado no comparte el Swarm de `run`.

## Alcance del contenedor Docker

`docker-compose.yml` levanta el nodo en modo demo. No incluye red P2P real.

## Avisos

- Fixtures NISAR simulados.
- Regla demostrativa, no modelo científico operacional.
