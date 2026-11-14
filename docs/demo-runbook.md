# Demo Runbook — WOMBAT

## Objetivo

Levantar el nodo WOMBAT localmente, validar WONBs y publicar el fixture de
anomalía en modo demo. Sin red P2P real (modo demo).

## Requisitos

- Rust toolchain estable.
- Carpeta de trabajo: `wombat/`.

## Comandos

### 1. Ejecutar pruebas

```bash
cargo test --workspace
```

### 2. Verificar formato y lint

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
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

## Dos nodos locales (modo demo, sin red)

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

## Red P2P (futuro)

Cuando libp2p esté completamente habilitado, cada nodo escuchará en
`/ip4/0.0.0.0/tcp/<puerto>` y se conectarán intercambiando Multiaddrs.

## Alcance del contenedor Docker

`docker-compose.yml` levanta el nodo en modo demo. No incluye P2P real entre
máquinas sin configuración de red adicional.

## Avisos

- Fixtures NISAR simulados.
- Regla demostrativa, no modelo científico operacional.
