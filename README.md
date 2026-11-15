# WOMBAT — El Niño Evidence Mesh

**Dónde mirar primero, con evidencia que se puede comprobar.**

WOMBAT toma el cambio que observa el radar **NISAR** entre dos fechas, lo contrasta
con una fuente independiente —**OPERA DSWx-S1**, derivada de Sentinel-1—, mide
cuántos metros de camino hay cerca de ese cambio y ordena celdas de 1 km × 1 km
por prioridad de verificación. Cada resultado sale como un bloque firmado con
Ed25519 que cualquiera puede verificar sin confiar en nosotros.

El usuario objetivo es el analista de riesgo del **Gobierno Regional de Piura**.
Esto sirve para **preparación**: decidir a qué celda mandar una brigada primero.
No emite alertas y no afirma que haya ocurrido una inundación.

| | |
|---|---|
| **Dashboard** | https://alexsander1207.github.io/wombat-dashboard/ · *(espejo en Vercel: pendiente)* |
| **Datos en vivo** | https://favio-coder.github.io/Wombat-core/data/tailandia-01/latest.json |
| **Bundle firmado** | [`data/tailandia-01/v1/`](data/tailandia-01/) — 605 bloques |

---

## El resultado: 600 celdas sobre una inundación real

El método está validado sobre un par NISAR real de la llanura central de
Tailandia, al oeste de Ayutthaya: **18 sep 2026 → 30 sep 2026**.

No es un producto para Piura. Es la prueba de que la cadena completa funciona
sobre dato real, porque para el AOI de Piura **no existe todavía un par de
escenas que encierre el evento sin mezclar madureces** (ver *Límites*).

| | celdas | fuente |
|---|---|---|
| Total | 600 | `data/tailandia-01/v1/wonb/risk/*.json` |
| Prioridad alta | 248 | `score_bp ≥ 6000` |
| Prioridad media | 151 | `3500 ≤ score_bp < 6000` |
| Prioridad baja | 104 | `score_bp < 3500` |
| Sin calidad suficiente | 97 | `valid_fraction < 0.60` → `rank`, `score_bp` y `components_bp` en `null` |
| Puntuadas (entran al ranking) | 503 | |

### Por qué 248 en alta no es un problema, y qué discrimina de verdad

Que 248 de 503 celdas caigan en prioridad alta no es un defecto del umbral: **el
AOI es la llanura que se inundó.** Casi todo el recuadro cambió. Un semáforo con
la mitad del mapa en rojo no le sirve a nadie, y por eso el producto **no es el
color: es el orden.**

```
score_bp:  máximo 8599  ·  mediana 5938  ·  mínimo 0
celdas con score_bp >= 8000:  26  (de 503)
```

Ahí está la discriminación: 248 pasan el umbral, pero solo **26** llegan a 8000.
A una brigada se le entregan las primeras diez, no las 248.

### Top 10 — las diez primeras celdas a verificar

| # | celda | `score_bp` | cambio | OPERA | vía a ≤250 m del cambio |
|---|---|---|---|---|---|
| 1 | `r19c12` | 8599 | 50.0 % | corroborada (82 %) | 6 663 m |
| 2 | `r18c14` | 8563 | 56.7 % | corroborada (89 %) | 5 053 m |
| 3 | `r18c12` | 8487 | 26.7 % | corroborada (68 %) | 8 787 m |
| 4 | `r16c05` | 8283 | 71.9 % | corroborada (88 %) | 3 755 m |
| 5 | `r16c13` | 8214 | 32.6 % | corroborada (80 %) | 4 913 m |
| 6 | `r16c07` | 8204 | 27.6 % | corroborada (82 %) | 4 472 m |
| 7 | `r04c06` | 8200 | 39.7 % | corroborada (85 %) | 3 965 m |
| 8 | `r19c15` | 8191 | 69.8 % | corroborada (99 %) | 1 149 m |
| 9 | `r09c07` | 8166 | 39.5 % | corroborada (86 %) | 3 537 m |
| 10 | `r18c10` | 8157 | 59.0 % | corroborada (90 %) | 2 737 m |

Cifras de `data/tailandia-01/v1/wonb/risk/tailandia-01-<celda>.json`.

**Las diez están corroboradas por OPERA.** Ninguna baja del 26 % de cambio. Eso
no estaba garantizado: las celdas con *más* camino cerca del cambio son otras
—`r10c19`, `r12c18`, `r10c18`— y las tres están **sin corroborar**. El peso de
OPERA las deja fuera del top 10 a propósito: más asfalto no vale más que una
segunda fuente que confirme.

Los tres bloques `action` del bundle dicen `field_verification_first`. Ninguna
celda afirma que hubo una inundación; dicen cuánto bajó la retrodispersión, si
OPERA ve agua nueva en la misma zona, y cuánto camino hay cerca.

---

## El método, en números

El ranking vive **solo** en `wombat-risk` (R15) y es determinista: la misma
entrada da los mismos bytes, con prueba de ranking dorado.

| | valor | fuente |
|---|---|---|
| Peso del cambio | 0.45 | [`config/risk-config.json`](config/risk-config.json) |
| Peso de OPERA | 0.35 | idem |
| Peso de la vía | 0.20 | idem |
| Umbral prioridad alta | 0.60 → `score_bp ≥ 6000` | `status_thresholds.review_high` |
| Umbral prioridad media | 0.35 → `score_bp ≥ 3500` | `status_thresholds.review_medium` |
| Calidad mínima | `valid_fraction ≥ 0.60` | `quality_min_valid_fraction` |
| Saturación del cambio | 0.25 | `change_saturation` |
| Umbral de cambio | 3.0 dB | `delta_db_threshold` |
| Radio de exposición vial | 250 m | `road_buffer_meters` |
| Corroboración OPERA | solape ≥ 0.20 | `corroboration_overlap_threshold` |

Nada de flotantes en lo que se firma (**R16**): el payload lleva puntos básicos,
metros enteros y centidecibelios.

### Las cuatro fuentes

| fuente | qué aporta | de quién |
|---|---|---|
| **NISAR L2 GCOV** | retrodispersión en dos fechas; su caída marca el cambio | NASA / ISRO |
| **OPERA L3 DSWx-S1** | agua superficial nueva, **derivada de Sentinel-1, no de NISAR** — por eso es fuente independiente | NASA JPL |
| **OpenStreetMap** | red vial, vía Overpass | © OpenStreetMap contributors |
| **ASF** | catálogo para encontrar pares comparables | Alaska Satellite Facility |

---

## Integridad: no hace falta confiar en nosotros

```bash
python scripts/verify_all.py          # las cuatro verificaciones y las tres puertas
```

Salida real sobre `data/tailandia-01/v1`:

```
  ✔  wombat verify      605 bloques
  ✔  verify_bundle.py   605 bloques
  ✔  verify.mjs         605 bloques
  ✔  bytes              614 archivos
  14 pasos: 14 ✔, 0 ✘, 0 ⊘
```

**Las tres no comprueban lo mismo**, y conviene saberlo antes de apoyarse en una:

| verificador | integridad | firma | lista de confianza | TTL y cadena |
|---|---|---|---|---|
| `wombat verify` (Rust) | sí, contra `index.json` | sí | **externa**, `--trusted` | sí |
| `verify_bundle.py` | sí + esquemas JSON | sí | la que viene **dentro** del bundle | sí |
| `verify.mjs` (JS) | no | sí | ninguna: la clave que el bloque declara | no |

Solo el verificador Rust mide las firmas contra una lista que el bundle no
controla. Un bundle que trae su propia lista de emisores **se autoavala**.

Los bytes que sirve GitHub Pages coinciden con el `sha256` del `index.json`
firmado — comprobado sobre `wonb/change.json`, `wonb/context.json`,
`wonb/risk/tailandia-01-r19c12.json` y `cells.geojson`.

---

## Reproducirlo

```bash
# 1. Clave Ed25519 fuera del repositorio (R19: rechaza un destino versionado)
cargo run -p wombat-cli -- keygen --out ~/.wombat/signing.key --name mi-clave

# 2. Exposición vial por celda  (necesita changed.tif, que sale de 03_change.py)
python data-pipeline/validacion/05_roads.py --in <par> --inputs data-pipeline/inputs/<aoi>

# 3. Celdas, overlays y la parte Python del bundle
python data-pipeline/validacion/06_cells.py --in <par> --inputs data-pipeline/inputs/<aoi> \
       --bundle <staging>

# 4. Firmar (R14: solo Rust firma) y verificar
cargo run -p wombat-cli -- build --in <staging> --aoi <aoi> --version v1 \
       --key ~/.wombat/signing.key --out data/<aoi>/v1
cargo run -p wombat-cli -- verify data/<aoi>/v1 --trusted config/trusted-issuers.json
```

Paso a paso en el [runbook](docs/demo-runbook.md). El pipeline completo
(`00_grid` → `06_cells`) está en [`data-pipeline/validacion/`](data-pipeline/validacion/README.md).

---

## Límites — lo que este proyecto todavía no puede decir

**1. PROVISIONAL es priorización, no medición validada.** El par NISAR usado está
marcado `PROVISIONAL` en su manifest: no pasó la validación de calidad de la
misión y sus valores absolutos pueden cambiar. Lo que sostenemos es el **orden
relativo** de las celdas, no la magnitud en decibelios.

**2. No hay par de evento para Piura.** El catálogo de ASF sobre el AOI de Piura
(`-80.80 -5.35 -80.55 -5.10`), consultado hasta el 30 nov 2026, devuelve escenas
el 7 oct y el 19 oct (PROVISIONAL) y el 31 oct (VALIDATED), en ambos tracks.
**Ningún par encierra el evento sin mezclar madureces**, y mezclarlas está
prohibido (R6/R7). Lo que sí existe es un par de **control** 7 oct → 19 oct, con
0,00 % de cambio medio: el resultado correcto para dos escenas previas al evento.

**3. `roads_total_m` del bloque `context` está sobreestimado.** Rust mide esa
longitud con constantes planas en grados, calibradas para la latitud de Piura:

```rust
// crates/wombat-cli/src/inputs.rs:232
const M_PER_DEG_LON: f64 = 110_400.0;
const M_PER_DEG_LAT: f64 = 110_570.0;
```

Medido sobre la misma geometría del bundle, reproyectada a EPSG:32647
(`geopandas.read_file(...).to_crs("EPSG:32647").length.sum()`):

```
longitud real en UTM 47N:   2 058 441 m
firmado por Rust:           2 082 247 m
diferencia:                   +23 806 m  =  +1.16 %
```

Afecta solo al total del AOI en el bloque `context`. **No afecta al ranking**: la
exposición vial por celda se mide en el CRS de la grilla, en metros reales.

**4. Un cambio observado no confirma ni predice una inundación.** La información
sobre el evento de Tailandia proviene de **resúmenes de prensa sin verificar**, y
por eso este repositorio no publica ninguna cifra de impacto.

---

## Fase 2 — el dashboard no depende de nada de esto (R17)

Existe código de malla P2P (`wombat-node`, TCP + GossipSub), un gateway HTTP de
solo lectura (`wombat-gateway`), terraform para un relay (`infra/relay/`) y
paquetes para Raspberry Pi ARM64 en los
[releases](https://github.com/Favio-coder/Wombat-core/releases), marcados
pre-release.

**Nada de eso está en el camino de la demo.** El dashboard lee archivos estáticos
firmados y funciona sin backend (**R17**); el navegador nunca habla libp2p (**R18**).
`wombat-node` implementa WONB **0.1**, un contrato distinto del vigente, cuyo
payload lleva `confidence` como `f64` y por tanto **viola R16**: está marcado
legado a propósito, y migrarlo es trabajo de fase 2. Detalles en
[arquitectura](docs/architecture.md) y en la
[especificación 0.1](docs/wonb-spec.md).

---

## Equipo

| módulo | responsable |
|---|---|
| Core Rust / P2P | Richard |
| Adaptador geoespacial Python | Gustavo |
| Nodo edge Raspberry Pi | Jelibeth |
| Agente de riesgo / dashboard | Mireya |
| Landing / pitch / UI | Ximena |
| QA, integración, Actions, fixtures, modo demo | Jayo |

Dos repositorios (**R26**): este core, y `wombat-dashboard` aparte.

## Contribuir

1. Rama desde `main`. No modificar `contracts/`, `config/` ni bundles publicados
   sin revisión y bump de versión (**R20**, **R27**, **R28**).
2. `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`,
   `cargo test --workspace`, `python -m pytest data-pipeline/validacion -q`.
3. `python scripts/verify_all.py` antes del PR.
4. Claves privadas nunca dentro del repositorio (**R19**).

Contrato vigente: [`contracts/v1/README.md`](contracts/v1/README.md).

## Licencia

MIT. Véase [LICENSE](LICENSE).
