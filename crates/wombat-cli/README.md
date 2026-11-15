# wombat-cli

El productor oficial del bundle firmado. **R14: solo Rust firma.**

El fixture `contracts/v1/fixtures/bundle/piura-demo/v0` lo generó
`contracts/v1/tools/gen_fixture_bundle.py` como bootstrap, para destrabar al
dashboard antes de que este binario existiera. Este crate es su reemplazo: el
test de extremo a extremo (`tests/e2e.rs`) toma los insumos **sin firmar** de ese
mismo bundle, los reconstruye con `wombat build` y exige que los 21 payloads
salgan idénticos a los publicados. Cuando ese test pasa, el script se puede
archivar.

## El flujo

```bash
# 1. Una sola vez. La ruta NO puede estar dentro de un repo git (R19).
wombat keygen --out ~/.wombat-keys/demo.key --name wombat-core-demo
#    Imprime la clave pública y la línea lista para pegar en
#    config/trusted-issuers.json. La privada queda solo en ese archivo.

# 2. Construir una versión. Inmutable una vez publicada (R28).
#    El destino es data/<aoi_id>/<bundle_version>/, que es la ruta que fija R28
#    y la que `.gitattributes` protege con `-text`. No uses dist/: está ignorado.
wombat build --in data-pipeline/out/piura-01 \
             --aoi piura-01 --version v1 \
             --key ~/.wombat-keys/demo.key \
             --out data/piura-01/v1

# 3. Verificar antes de publicar.
wombat verify data/piura-01/v1

# 4. Commitear el bundle. La clave no, nunca.
git add data/piura-01/v1 && git commit -m "data(piura-01): bundle v1"
```

`publish` (libp2p) es P1 y no está implementado a propósito.

## Los tres comandos

### `keygen --out <ruta> [--name <nombre>]`

Genera un par Ed25519, escribe los 32 bytes de la semilla en hexadecimal y
imprime la pública como `ed25519:<hex>` más la línea de `trusted-issuers.json`.

Se niega a escribir si `<ruta>` cae dentro de un repositorio git —cualquier
ancestro con `.git`, sea directorio o archivo de worktree—, que es el accidente
que R19 previene. No hay ruta por defecto: hay que elegirla, y elegirla fuera.

En Unix el archivo queda en `0600`. En Windows el comando **no** toca los
permisos y lo dice: el equivalente real sería reescribir la ACL, y poner el
atributo de solo lectura sugeriría una protección que no existe.

### `build --in <insumos> --aoi <id> --version <vN> --key <ruta> --out <dir>`

Lee de `<insumos>`: `manifest.json`, `cells.geojson`, `roads.geojson` y
`overlays/`. Lee de `--config` (por defecto `config/`): `risk-config.json` y
`trusted-issuers.json`. Llama a `wombat-risk` para puntuar —el ranking vive solo
ahí (R15)— y sella la cadena Change/Context → Risk → Action.

Escribe el bundle completo e `index.json` con el `sha256` de cada archivo.
Los insumos se copian **byte a byte**, sin reserializar: el hash de
`cells.geojson` entra al payload firmado del bloque Change, y tiene que describir
el archivo que el pipeline produjo.

Con el mismo `--key` y el mismo `--issued-at`, la salida es idéntica byte a byte.
Sin `--issued-at` usa la hora actual.

Se niega a construir si:

| Motivo | Regla |
|---|---|
| La clave pública no está en `trusted-issuers.json` con los cuatro roles | — |
| `manifest.nisar.comparable` es `false` o algún `comparable_checks` es `false` | R1: solo se procesan escenas comparables |
| `--out` ya contiene un `index.json` | R28 |
| `--version` no cumple `^v[0-9]+$` | contrato |
| El `aoi_id` del manifest no coincide con `--aoi` | — |

Todo eso se comprueba **antes** de escribir el primer byte: un bundle a medio
escribir es peor que uno que no existe.

### `verify <dir> [--as-of <iso>] [--trusted <archivo>]`

Comprueba el `sha256` de cada archivo contra `index.json`, el `id` de cada
bloque, la firma, el emisor y su rol, el TTL y que cada padre exista. Cierra el
círculo verificando que los hashes que declara el bloque Change —firmado—
describan los overlays y el `cells.geojson` del bundle: alterar un archivo y
recalcular el índice no alcanza, habría que volver a firmar el Change.

No corta en el primer error: informa todos, uno por línea, y termina con un
resumen en el mismo formato que `verify_bundle.py`.

```text
OK   data/piura-01/v1  |  21 bloques | as_of 2026-11-14T17:00:00Z | 0 error(es)
```

Sale con 0 solo si no hubo ningún error.

Sin `--as-of`, el TTL se mide contra `index.generated_at`: un bundle estático es
una foto histórica, y evaluarlo con la hora real haría que todo saliera expirado
días después. En modo vivo hay que pasar la hora explícitamente.

#### La lista de confianza viene de afuera

`--trusted` apunta al `trusted-issuers.json` con el que se miden las firmas, y
su defecto es `config/trusted-issuers.json`, el del repositorio.

El bundle trae un `trusted-issuers.json` propio y **no se lee nunca solo**. Un
bundle que se autoavala no prueba nada: quien pueda escribir en el directorio
cambia esa lista, vuelve a firmar con su clave y se lleva un `OK`. Una firma solo
significa algo contra una lista que el verificador ya tenía antes de ver el
bundle. El `trusted-issuers.json` de dentro sigue en el contrato, con su hash en
`index.json`, como declaración de quién *dice* el productor que firmó; para
usarlo hay que nombrarlo a mano:

```bash
# Lo normal: la confianza la decide config/, no el bundle.
wombat verify data/piura-01/v1

# El fixture está firmado por una clave de desarrollo que a propósito no está
# en config/trusted-issuers.json. Se nombra su lista explícitamente.
wombat verify contracts/v1/fixtures/bundle/piura-demo/v0   --trusted contracts/v1/fixtures/bundle/piura-demo/v0/trusted-issuers.json
```

Si el archivo no existe, `verify` falla nombrándolo. No hay defecto silencioso:
sin lista de confianza no hay verificación posible.

## Las tres implementaciones

El valor del fixture es que tres verificadores independientes acepten los mismos
bloques. `wombat verify` es el de Rust; los otros están en `contracts/v1/tools/`.
El test `the_built_bundle_also_passes_the_python_verifier` corre el de Python
sobre un bundle construido por este CLI. Si falta Python con `jsonschema`,
`referencing` y `cryptography`, el test imprime `SKIP` y dice por qué —visible
con `cargo test -- --nocapture`— en vez de usar un `#[ignore]` que no deja
rastro.

Si alguno de los tres rechaza un bloque que los otros aceptan, el problema está
en la canonicalización o en el contrato, no en el verificador.
