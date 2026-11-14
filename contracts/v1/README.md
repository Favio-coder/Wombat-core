# Contratos WOMBAT v1.0.0

Frontera entre personas y repos (plan maestro v2, §7 y R20/R26–R28). **No se cambia un campo "en caliente":**
un PR que toque este directorio actualiza esquema + fixture + `VERSION` a la vez.

> **Convivencia con lo anterior.** `contracts/schemas/` y `contracts/fixtures/` (WONB `wombat/0.1`: anomaly/risk/tool) son el
> diseño previo y siguen validándose en `ci.yml`. Los contratos del plan v2 viven **solo** en `contracts/v1/` (WONB `0.2`).
> Cuando `wonb-core` migre a 0.2, se retira lo viejo. Mientras tanto, nada se pisa.
> El dashboard vendoriza esta carpeta como `contracts/v1/` en su propio repo.

## Contenido

| Archivo | Qué valida |
|---|---|
| `manifest.schema.json` | `manifest.json` (Python → todos). Incluye `data_origin` (`nisar_real` \| `simulated`, ver R1) y obliga a `comparable_checks` todos `true` si `comparable: true` (R6). |
| `cells.schema.json` | `cells.geojson` (Python → Rust). Floats permitidos aquí. |
| `risk-config.schema.json` | `config/risk-config.json` (R3). |
| `wonb-envelope.schema.json` + `wonb-payloads.schema.json` | Bloques WONB 0.2. **Payload sin floats (R16).** Cada `type` fija sus `parents` y `valid_until`. |
| `bundle-index.schema.json` | `index.json` del bundle publicado. |
| `fixtures/bundle/piura-demo/v0/` | Bundle **SIMULADO y firmado**: 16 celdas, 21 bloques, PNG con marca "SIMULATED DATA". |
| `fixtures/expected/ranking.json` | Ranking dorado para el test de `wombat-risk` (Rust). |
| `tools/verify_bundle.py` | Verificador Python (esquemas + sha256 + hash + firma + issuer + TTL + cadena). |
| `tools/verify-rs/` | Verificador Rust independiente (`serde_jcs` + `ed25519-dalek 2`). Puede copiarse a `wonb-core`. |
| `tools/verify-js/` | Verificador JS (`canonicalize` + `@noble`). Base de `src/verify/` del dashboard. |
| `tools/gen_fixture_bundle.py` | **Bootstrap** que generó el fixture. Ver "Firma del fixture". |

## Comprobar un bundle

```bash
pip install jsonschema referencing cryptography
python3 contracts/v1/tools/verify_bundle.py contracts/v1/fixtures/bundle/piura-demo/v0
cargo run --release --manifest-path contracts/v1/tools/verify-rs/Cargo.toml -- contracts/v1/fixtures/bundle/piura-demo/v0
(cd contracts/v1/tools/verify-js && npm ci && node verify.mjs ../../fixtures/bundle/piura-demo/v0)
```

## Firma del fixture (leer antes de tocar)

- R14 dice que solo Rust firma. El fixture lo firmó `tools/gen_fixture_bundle.py` **únicamente para destrabar al dashboard y al CI**.
  Cuando `wombat-cli export-static` exista, el fixture se regenera con Rust y el script se archiva.
- La clave privada se generó en memoria y **se descartó** (R19). Solo existe la pública, en `trusted-issuers.json` del bundle.
  Por eso regenerar cambia todos los `id` y firmas: es esperado.
- El valor que tiene: **tres implementaciones independientes (Python, Rust, JS) verifican los mismos 21 bloques**. Si `wonb-core` produce
  bloques que alguna de las tres rechaza, el bug está en la canonicalización o en el contrato, no en el verificador.

## Reglas numéricas de referencia (para `wombat-risk`)

- Fracción → puntos básicos: `bp = floor(x * 10000 + 0.5)` (half-up). dB → centésimas: `cdb = floor(db * 100 + 0.5)`.
- Rank: orden por `score_bp` desc, luego `change_bp` desc, luego `cell_id` asc. `status` se decide sobre `score_bp` entero
  (`>= 6000` high, `>= 3500` medium).
- `road` se normaliza por el **máximo entre celdas que pasan el quality gate** (`valid_fraction >= quality_min_valid_fraction`).
- Celdas bajo el umbral de calidad **sí** reciben bloque Risk con `status: insufficient_quality`, `rank/score_bp/components_bp: null`
  (el mapa necesita poder decir "sin datos suficientes"). No entran al ranking ni a los Action.
- El texto de `explanation` es redacción de referencia; el test de Rust debe comparar los números, no la ortografía.

## TTL en bundles estáticos

Los bloques `risk` caducan a las 24 h y `action` a 1 h. Un bundle estático se abre **días después** (jueces globales): evaluado con la hora
actual, todo saldría "expirado" y el botón Verificar mostraría ✗. Regla propuesta: en modo estático el TTL se evalúa contra `generated_at`
del `index.json` (`verify_bundle.py` ya lo hace así; `--as-of` lo cambia) y la UI muestra "instantánea histórica". En modo vivo se evalúa con la hora real.

## Fixtures

Los fixtures tienen sha256 fijos; no los abras ni los guardes con editores que cambien saltos de línea.

## Versionado

`VERSION` es semver. Cambio que rompe → mayor +1 y aviso en el grupo **antes** del merge (R27). El dashboard declara qué mayor acepta.
