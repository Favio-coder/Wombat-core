# Runbook — WOMBAT

Este runbook separa lo que se puede ejecutar hoy de los flujos que siguen en el plan. El bundle de ejemplo es **SIMULADO**; no es una corrida con datos reales de NISAR.

## Requisitos

- Rust estable y Cargo.
- Ejecutar los comandos desde la raíz de `wombat-core`.

## Verificación del workspace actual

### 1. Formato, lint y pruebas

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

### 2. Validar un WONB 0.1 legado

```bash
cargo run -p wombat-node -- validate --file contracts/fixtures/anomaly-valid.json
```

Salida esperada: `WONB válido: wonb:anomaly:demo-001`.

### 3. Probar `publish` legado sin red

```bash
cargo run -p wombat-node -- publish --file contracts/fixtures/anomaly-valid.json
```

Este comando valida e imprime el JSON. **No lo transmite ni lo publica a otros nodos.**

### 4. Comprobar rechazos del validador 0.1

Estos comandos deben terminar con error:

```bash
cargo run -p wombat-node -- validate --file contracts/fixtures/expired-invalid.json
cargo run -p wombat-node -- validate --file contracts/fixtures/malformed-invalid.json
```

### 5. Arrancar el modo demo legado

```bash
cargo run -p wombat-node -- run --demo
```

El comando registra que está en modo local y termina. No deja un nodo escuchando, no inicia libp2p y no permite que dos instancias se comuniquen.

## Estado de los modos objetivo

Los siguientes flujos son **[PLANIFICADO]**. No hay comandos de dashboard o gateway para ejecutar en este checkout.

| Modo | Objetivo | Estado |
|---|---|---|
| `static` | `wombat-dashboard` incluye una copia fijada del bundle y funciona sin backend. | **[PLANIFICADO]** No hay dashboard implementado en este repositorio. |
| `remote` | El dashboard obtiene un bundle publicado por `wombat-core` en GitHub Pages. | **[PLANIFICADO]** El workflow puede publicar el fixture simulado; el dashboard no tiene loader remoto. |
| `live` | El dashboard consulta REST/WS de `wombat-gateway` para estado y bloques recientes. | **[PLANIFICADO]** No hay gateway ni transporte P2P real. |

### Flujo futuro de datos y demo — [PLANIFICADO]

1. Implementar el pipeline en `data-pipeline/`: buscar dos escenas comparables, calcular el cambio, contrastar con OPERA, medir exposición vial y producir celdas y overlays.
2. Implementar `wombat-cli` para validar entradas, crear y firmar bloques WONB 0.2 y exportar bundles versionados e inmutables.
3. Consumir el bundle desde `wombat-dashboard`, repositorio separado, con modo `static` por defecto y `remote` opcional.
4. Añadir `wombat-gateway` y modo `live` solo después; el navegador se comunica con el gateway, nunca directamente con libp2p.
5. Considerar edge Raspberry/ESP32 como P2, sin presentarlo como hardware operativo antes de implementarlo.

## Verificación de contratos v1

Los workflows de CI incluyen pruebas cruzadas de esquemas, hashes y firmas del bundle en Python, Rust y JavaScript. El fixture `contracts/v1/fixtures/bundle/piura-demo/v0` está simulado; su existencia no implica que el pipeline geoespacial esté terminado.
