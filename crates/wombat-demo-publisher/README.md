# Demo P2P WOMBAT

El publicador emite únicamente WONBs `Change`. Firma en Rust usando
`wombat-core` (canonicalización JCS, ID por contenido, Ed25519 y verificación
local). Nunca calcula `Risk` ni `Action`.

Sin `--pipeline-input`, o si el manifest, las celdas, el resumen, CRS,
comparabilidad u overlays no pasan validación, usa fallback simulado. Imprime
el motivo y `DEMO SIMULADA — no es alerta científica`; el bloque marca
`simulation: true` y `data_origin: simulated`. Si selecciona datos de pipeline,
marca `simulation: false` y conserva manifest, CRS, procedencia, referencias,
resumen y hashes disponibles. Esto tampoco constituye una alerta científica.
No se modifican los archivos de entrada.

Genera la clave de firma con `wombat-cli keygen` fuera del repositorio y prepara
un `trusted-issuers.json` con la clave pública y rol `change`. Relay, receptor
y publicador deben usar listas de confianza compatibles. Mantén las semillas
privadas y las identidades libp2p fuera del repositorio.

## Demo simulada reproducible

### 1. Iniciar el relay

```bash
cargo run -p wombat-node --features p2p -- run \
  --listen /ip4/0.0.0.0/tcp/9000 \
  --topic wombat/pe/piura/piura-demo/change \
  --trusted-issuers "$HOME/.wombat-keys/demo-trusted-issuers.json" \
  --identity-key "$HOME/.wombat-keys/relay-identity.key"
```

Anota el PeerId y la multiaddr que informa el relay. El relay actual conoce el
topic Piura simulado mostrado arriba.

### 2. Iniciar la Raspberry receptora

```bash
cargo run -p wombat-node --features p2p -- run \
  --listen /ip4/0.0.0.0/tcp/0 \
  --peer "/ip4/<RELAY_IP>/tcp/9000/p2p/<RELAY_PEER_ID>" \
  --topic wombat/pe/piura/piura-demo/change \
  --trusted-issuers "$HOME/.wombat-keys/demo-trusted-issuers.json" \
  --identity-key "$HOME/.wombat-keys/raspberry-identity.key" \
  --event-out "$HOME/wombat-demo-events.jsonl"
```

El JSONL contiene solo eventos tras verificación del bloque y la política de
cadena; incluye `verified`, `simulation`, `data_origin`, `payload` y
`provenance` para que gateway y dashboard distingan la fuente.

### 3. Iniciar el publicador Piura simulado

```bash
cargo run -p wombat-demo-publisher -- \
  --scenario piura \
  --peer "/ip4/<RELAY_IP>/tcp/9000/p2p/<RELAY_PEER_ID>" \
  --key "$HOME/.wombat-keys/demo.key" \
  --trusted-issuers "$HOME/.wombat-keys/demo-trusted-issuers.json" \
  --interval-secs 30 \
  --count 10 \
  --identity-key "$HOME/.wombat-keys/publisher-identity.key"
```

El publicador espera una conexión y la suscripción al topic antes de emitir.
Cada emisión tiene `issued_at` e ID nuevos. Para generar y verificar bloques
sin abrir la red, añade `--dry-run`; no hacen falta `--peer` ni
`--identity-key`.

## Entradas opcionales del pipeline

`--pipeline-input` apunta a un directorio con `manifest.json`,
`cells.geojson`, `change_summary.json` y `overlays/{t1,t2,delta}.png`. Solo se
acepta la evidencia si el AOI coincide con el escenario, los checks de
comparabilidad pasan y las referencias, celdas y archivos requeridos son
válidos. Si no, el publisher informa el motivo y vuelve al fallback.

| Escenario | Ejemplo de entrada | Topic si se valida pipeline | Topic fallback |
|---|---|---|---|
| Piura | `data-pipeline/validacion/resultados/peru-fuentes-nasa/ciudades/piura-01/previo-07oct-19oct/` | `wombat/pe/piura/piura-01/change` | `wombat/pe/piura/piura-demo/change` |
| Tailandia | `data-pipeline/validacion/resultados/tailandia-01/evento-18sep-30sep/` | `wombat/th/thailand/tailandia-01/change` | `wombat/th/thailand/thailand-demo/change` |

Ejemplos de selección:

```bash
cargo run -p wombat-demo-publisher -- --scenario piura \
  --pipeline-input data-pipeline/validacion/resultados/peru-fuentes-nasa/ciudades/piura-01/previo-07oct-19oct/ \
  --key "$HOME/.wombat-keys/demo.key" \
  --trusted-issuers "$HOME/.wombat-keys/demo-trusted-issuers.json" \
  --interval-secs 30 --dry-run --count 2
```

```bash
cargo run -p wombat-demo-publisher -- --scenario thailand \
  --pipeline-input data-pipeline/validacion/resultados/tailandia-01/evento-18sep-30sep/ \
  --key "$HOME/.wombat-keys/demo.key" \
  --trusted-issuers "$HOME/.wombat-keys/demo-trusted-issuers.json" \
  --interval-secs 30 --dry-run --count 2
```

El directorio Piura de ejemplo es un par de referencia/control, no evidencia
de un evento de inundación. No se presupone que Piura tenga un par comparable:
si los controles fallan, se usa el fallback. Para publicar pipeline, configura
relay y receptores con el topic de la fuente seleccionada antes de iniciar el
publisher. Tailandia siempre utiliza su AOI y topic propios; solo se propaga si
relay y receptores también están suscritos a su topic de Tailandia, nunca al de
Piura.
