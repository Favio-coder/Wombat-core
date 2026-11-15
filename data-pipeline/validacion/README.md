# data-pipeline/validacion — herramientas de validación con datos NISAR reales

Complemento del pipeline oficial de `data-pipeline/nisar/` (Gustavo). No lo reemplaza ni lo modifica.
Sirve para probar el método con escenas reales en cualquier lugar y para medir tres cosas que
cambian el resultado: el ruido de speckle, las zonas que cambian sin evento y el contraste con OPERA.
Python no firma (R14) ni rankea (R15).

## En qué se diferencia del pipeline oficial

| | `data-pipeline/nisar/` (oficial) | `data-pipeline/validacion/` |
|---|---|---|
| AOI | Fijo: `piura-01`, EPSG:32717 | Cualquiera; otra zona UTM con `--crs` |
| Lectura de escenas | HDF5 locales | Locales o por partes desde Earthdata |
| Cambio | Píxel a píxel | Promediado a 100 m antes del umbral |
| R13 | Acepta una máscara ya hecha | Construye la máscara con un par sin evento |
| OPERA | Consume `opera-context.json` de Jelibeth | Lo calcula para validar (`validate_opera.py`) |
| Salidas por defecto | `data-pipeline/out/<aoi>/` | `data-pipeline/out/validacion/<aoi>/` |

La grilla de `piura-01` es la publicada en `data-pipeline/inputs/piura-01/grid.geojson`; estas
herramientas la leen tal cual. Los formatos de intercambio con Jelibeth son los del README oficial.

## Preparar

```bash
python -m venv .venv
.venv/Scripts/python -m pip install -r data-pipeline/validacion/requirements.txt
.venv/Scripts/python -m pytest data-pipeline/validacion -q
```

Para leer escenas desde Earthdata, cada persona corre una vez en su terminal
`python data-pipeline/validacion/earthdata_login.py`. El login queda en su equipo y nunca se sube.

## Orden de trabajo

| Paso | Comando | Salida |
|---|---|---|
| Grilla de un AOI nuevo | `00_grid.py --aoi <aoi> --bbox O S E N [--crs EPSG:xxxxx] --out …/grid.geojson` | grilla de 1 km |
| Buscar escenas | `01_search.py --bbox O S E N --start … --end … --out …/candidates.json` | granules, cobertura y pares de igual madurez |
| Revisar un granule local | `inspect_granule.py archivo.h5` | rutas HDF5 y valores de la máscara (R11) |
| Recorte t1/t2 | `02_subset.py --aoi <aoi> --aoi-name "…" --t1 <granule> --t2 <granule>` | `t1.tif`, `t2.tif`, máscaras, looks, `manifest.json` |
| Cambio con signo | `03_change.py --in <carpeta> [--exclude exclude.tif]` | `delta_db.tif`, `valid.tif`, `changed.tif`, `change_summary.json` |
| Máscara de exclusión (R13) | `make_exclude.py --base <par sin evento> --out …/exclude.tif` | agua permanente y zonas que cambian solas |
| Contraste OPERA | `validate_opera.py --in <carpeta> --grid … --opera-t1 … --opera-t2 … --out …/opera_cells.json` | coincidencia por celda y global |
| Celdas y PNG | `06_cells.py --in <carpeta> --inputs <carpeta con grid.geojson>` | `cells.geojson`, `overlays/` |

Todos se corren con `python data-pipeline/validacion/<script>` desde la raíz del repo.

## Qué mide y por qué importa

- **Ruido de speckle.** Las escenas tienen 4 a 9 looks por píxel (10–20 m): 2 a 3 dB de desviación
  en `delta_db` solo por ruido. Con ±3 dB píxel a píxel, una celda sin cambio real sale con 17 %
  (Piura, 20 m) o 27 % (Jequetepeque, 10 m) de "cambio". Promediando gamma0 a 100 m sale con 0 %.
  `03_change.py` informa el ruido esperado y avisa si el umbral queda dominado por él.
  Parámetro en `change-config.json`; `--multilook-m 0` vuelve al cálculo píxel a píxel.
- **Cambio habitual (R13).** `make_exclude.py` marca agua permanente (oscura en las dos fechas de un
  par sin evento) y bloques que ya cambian ≥ umbral sin evento. Lo excluido cuenta como inválido y
  baja `valid_fraction`. El umbral de agua (−15 dB) se ajustó en Tumbes; en Piura marca también
  zonas secas y lisas, así que hay que revisarlo por AOI.
- **Contraste independiente.** `validate_opera.py` aplica §6.6 con OPERA DSWx-S1 (Sentinel-1, R4) y
  respeta el máximo de días entre NISAR y OPERA de `risk-config.json`.

## Resultados obtenidos (14 nov 2026)

| Caso | Par | Madurez | Área con cambio ≥ 3 dB | Celdas con > 25 % |
|---|---|---|---|---|
| Tailandia, inundación | 6 sep → 30 sep | PROVISIONAL | 48,6 % bajó | 369 de 503 |
| Tailandia, sin evento | 13 ago → 25 ago | PROVISIONAL | 2,7 % bajó, 1,8 % subió | par base |
| Tumbes (Zarumilla), sin evento | 7 oct → 19 oct | PROVISIONAL | 0,4 % | 0 de 443 |
| Tumbes, exploratorio | 19 oct → 31 oct | mezclada | 0,5 % | 0 de 443 |
| Piura, exploratorio | 19 oct → 31 oct | mezclada | 2,3 % | 2 de 517 |

Tailandia contra OPERA (par 18 → 30 sep, OPERA a 1 día de cada escena): 239 de 305 celdas con más
de 25 % de cambio quedan corroboradas.

La evidencia de Tailandia está en `resultados/tailandia-01/`: documento, imágenes reducidas,
tablas por celda y fichas de cada par (3,4 MB, sin rásteres). No es un bundle. Las salidas de
trabajo siguen en `data-pipeline/out/validacion/`, que no entra al repositorio.

`resultados/peru-fuentes-nasa/` reúne lo explorado en Perú con otras fuentes de la NASA (agua
OPERA DSWx-S1, lluvia NASA POWER, humedad SMAP) y deja Sechura preparado para NISAR. Conclusión:
en Tumbes y Piura no hay agua extensa que mapear; NISAR registra ahí el suelo mojado (0,5 a 1 dB).

`06_cells.py` mide las vías por celda si en la carpeta de insumos hay `roads.geojson` (WGS84) y
`osm-metadata.json`, como los de `data-pipeline/inputs/<aoi>/`. Necesita `changed.tif` del mismo
par. `WOMBAT_CONTRACTS_DIR` permite validar contra otro `contracts/v1` sin cambiar de rama.

`make_exclude.py --water-raster` toma el agua permanente de otra fuente (p. ej. OPERA de antes
del evento) en lugar del umbral de brillo, que en desiertos marca arena seca como agua.

## Límites

- **Dos cambios de método respecto al §6 del plan**, sin aprobar aún: el promediado a 100 m y la
  máscara de exclusión. Si se adoptan, hay que declararlos en la pantalla Método (R3, R13); hoy
  el contrato no tiene un campo para llevar esos parámetros en el bundle.
- **Fuera de EPSG:32717 no hay bundle**: el contrato v1 fija esa zona. Tailandia (EPSG:32647) es
  solo validación.
- **`--exploratory`** (02, 03, 06) procesa un par no comparable solo para análisis: el manifest
  queda con `comparable: false` y nunca se arma bundle.
- `02_subset.py` leyó escenas reales de frecuencia A, 10 y 20 m, en cuatro zonas. No se probó
  con frecuencia B ni con un AOI en el borde de una escena.
- `make_synthetic.py` genera una escena SIMULADA, usada solo por las pruebas.
