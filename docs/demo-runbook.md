# Runbook — WOMBAT

Lo que este repositorio demuestra hoy es una cadena de evidencia: **generar una
clave → construir un bundle firmado → verificarlo con tres implementaciones
independientes**. Ese es el flujo de la demo y el orden de este documento.

> **El bundle `piura-demo/v0` es SIMULADO.** Sus granules se llaman
> `SIMULATED-T1` y `SIMULATED-T2`; no son observaciones NISAR reales ni una
> evaluación científica operacional. Un cambio observado no confirma ni predice
> una inundación: esto sirve para **preparación**, no para emitir alertas.

## Requisitos

- Rust estable y Cargo.
- Python 3.12 con `jsonschema` y `referencing` (para `verify_bundle.py`).
- Node 20 y `npm ci` en `contracts/v1/tools/verify-js/` (para `verify.mjs`).
- Todos los comandos se ejecutan desde la raíz de `wombat-core`.

## 1. La puerta de calidad

Un solo comando corre los cuatro verificadores y las tres puertas, y resume todo
en una tabla:

```bash
python scripts/verify_all.py
```

Sale con 0 solo si no hubo ningún `✘`. Un `⊘` significa saltado —por ejemplo, el
verificador JS sin `node_modules`— y **nunca** cuenta como éxito. Las puertas por
separado, si se quieren mirar de a una:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 2. Generar la clave de firma

**R14: solo Rust firma.** Python produce evidencia sin firmar; la canonicalización
JCS, el `id` por contenido y la firma Ed25519 viven en `wombat-core`.

```bash
cargo run -p wombat-cli -- keygen --out ~/.wombat-keys/demo.key --name wombat-core-demo
```

El comando **rechaza** cualquier destino que caiga dentro de un repositorio git
(R19: una clave privada jamás entra al árbol versionado). Imprime la clave
pública y la línea lista para pegar en `config/trusted-issuers.json`; la privada
queda solo en ese archivo, fuera del repo.

## 3. Construir un bundle firmado

```bash
cargo run -p wombat-cli -- build \
  --in data-pipeline/out/piura-01 \
  --aoi piura-01 --version v1 \
  --key ~/.wombat-keys/demo.key \
  --out data/piura-01/v1
```

El destino es `data/<aoi_id>/<bundle_version>/`, que es la ruta que fija **R28**:
una versión publicada es inmutable, se publica una nueva y nunca se sobrescribe
la anterior. `build` falla **antes** de escribir nada si el par de escenas no es
comparable —`manifest.nisar.comparable` o cualquiera de los seis
`comparable_checks` en `false`— porque **R6/R7** prohíben puntuar escenas que no
comparten frame, dirección de órbita, modo, frecuencia, polarización y madurez.

Mientras `data-pipeline/` no entregue insumos reales, el único bundle construible
es el fixture simulado que ya está en el árbol.

## 4. Verificar con las tres implementaciones

Las tres leen el mismo bundle y deben coincidir en el número de bloques y en los
errores. Que coincidan es el punto: una sola implementación no demuestra nada.

```bash
# Rust. --trusted apunta a la lista de emisores; el defecto es
# config/trusted-issuers.json. El fixture está firmado por una clave de
# desarrollo que a propósito NO está en la confianza de producción, así que
# aquí se nombra su propia lista a mano.
cargo run -p wombat-cli -- verify contracts/v1/fixtures/bundle/piura-demo/v0   --trusted contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json

# Python
python contracts/v1/tools/verify_bundle.py contracts/v1/fixtures/bundle/piura-demo/v0

# JavaScript
cd contracts/v1/tools/verify-js && npm ci && cd -
node contracts/v1/tools/verify-js/verify.mjs contracts/v1/fixtures/bundle/piura-demo/v0
```

Salida esperada en las tres: **21 bloques, 0 con error.**

Sobre un bundle publicado en `data/`, `wombat verify` se corre sin `--trusted`:
el defecto es `config/trusted-issuers.json`, y es ahí donde se decide en quién
se confía.

### Las tres no comprueban lo mismo

Que coincidan en los 21 bloques es valioso —si discreparan, el bug estaría en la
canonicalización o en el contrato—, pero **no son tres verificaciones
equivalentes** y el pitch no debería decir que lo son:

| Verificador | Integridad de archivos | `id` + firma | Lista de confianza | TTL y cadena |
|---|---|---|---|---|
| `wombat verify` (Rust) | `sha256` de cada archivo contra `index.json`, más los hashes que repite el bloque Change firmado | sí | **externa**, `--trusted`, defecto `config/trusted-issuers.json` | sí |
| `contracts/v1/tools/verify_bundle.py` | `sha256` contra `index.json`, más los JSON Schema | sí | la que viene **dentro del bundle** | sí |
| `contracts/v1/tools/verify-js/verify.mjs` | no la mira | sí | **ninguna** | no |
| `contracts/v1/tools/verify-rs` (el de CI) | no la mira | sí | **ninguna** | no |

Lo que esto significa en concreto: JS y `verify-rs` comprueban que cada bloque
esté firmado por la clave que **el propio bloque declara** en su campo `issuer`.
Es una prueba de que el bloque no fue alterado desde que se firmó, y nada más:
un bloque firmado por una clave desconocida también pasa. La pregunta "¿confío
en quien firmó esto?" solo la responde `wombat verify`, y solo contra una lista
que tenía antes de ver el bundle.

El TTL se mide contra `index.generated_at` salvo que se pase `--as-of`, porque un
bundle estático es una foto histórica y no caduca por el paso del tiempo.

## 5. Publicar

`.github/workflows/publish-data.yml` ensambla `data/**` más el fixture, corre
`verify_bundle.py` sobre **cada** bundle y escribe `latest.json` apuntando a la
versión vigente. Requiere *Settings → Pages → Source: "GitHub Actions"*.

## WONB 0.1 y el nodo P2P — LEGADO

`wombat-node` implementa **WONB 0.1**, un contrato distinto del vigente. Su
payload firmado lleva `confidence` como `f64`, lo que **viola R16** (el payload
firmado no lleva floats, para que Rust y JavaScript no serialicen distinto). No
forma parte del contrato `contracts/v1/` ni del bundle, y **no se demuestra**.

El transporte libp2p existe detrás de una feature opcional
(`--features p2p`: TCP, Noise, Yamux, GossipSub) y sin esa feature el binario
falla con `modo P2P no disponible`. Nada de esto participa de la cadena de
evidencia de arriba. Los comandos legados quedan documentados para quien
mantenga 0.1:

```bash
cargo run -p wombat-node -- validate --file contracts/fixtures/anomaly-valid.json
cargo run -p wombat-node -- publish  --file contracts/fixtures/anomaly-valid.json
```

`publish` valida e imprime el JSON; sin `--features p2p` **no lo transmite**.
La especificación 0.1 está en [docs/wonb-spec.md](wonb-spec.md), marcada como
legada. El contrato vigente es [contracts/v1/README.md](../contracts/v1/README.md).

## Lo que todavía no existe — PLANIFICADO

| Pieza | Estado |
|---|---|
| Insumos NISAR reales en `data-pipeline/out/` | No hay ninguno; todo granule del árbol es `SIMULATED-*`. |
| Dashboard (`wombat-dashboard`, repo aparte, **R26**) | No vive en este repositorio. Modo `static` obligatorio (**R17**); `remote` y `live` opcionales. |
| `wombat-gateway` REST/WS y modo `live` | No implementado. El navegador habla con el gateway, nunca con libp2p (**R18**). |
| Edge Raspberry/ESP32 | P2. No hay hardware ni conexión implementados. |
