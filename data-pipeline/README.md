# WOMBAT — pipeline NISAR

Componente de Gustavo, Plan Maestro v2 §§6–7. Python prepara evidencia sin firmar;
Jelibeth obtiene y procesa OPERA/OSM; Rust calcula ranking, estados y firmas.

## Ejecutar desde la raíz (Windows / PowerShell)

Python 3.14; versiones probadas fijadas en `requirements.txt`:

Validación tras la limpieza: **25 pruebas aprobadas**, sin advertencias; `pip check` correcto.

```powershell
python -m venv data-pipeline/.venv
data-pipeline/.venv/Scripts/python.exe -m pip install -r data-pipeline/requirements.txt
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/create_grid.py
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/check_grid.py
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/01_search.py
data-pipeline/.venv/Scripts/python.exe -B -m pytest -c data-pipeline/pytest.ini -p no:cacheprovider -q
```

La búsqueda consulta ASF sin descargar productos y sale con código 2 si falta un par.
Cuando existan dos HDF5 reales comparables, conservar sus nombres de granule originales:

```powershell
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/02_subset.py --t1 'RUTA_T1.h5' --t2 'RUTA_T2.h5'
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/03_change.py
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/06_cells.py --roads data-pipeline/inputs/piura-01/roads.geojson --osm-metadata data-pipeline/inputs/piura-01/osm-metadata.json
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/assemble.py --validate-only
data-pipeline/.venv/Scripts/python.exe data-pipeline/nisar/assemble.py
```

Antes de descargar, revisar `size_bytes` y las alternativas de recorte autenticado
[ASF/Earthdata](https://nisar-docs.asf.alaska.edu/earthaccess/). El lector recibe archivos locales y
lee ventanas HDF5. `02 --exclusion-mask RASTER` acepta CRS declarado y valores 0=conservar,
1=excluir, 255=desconocido. Solo conserva 0. Falta revisar arrozales activos y núcleo urbano (R13).

## Archivos y convenciones

| Archivo | Responsabilidad |
|---|---|
| `inputs/piura-01/aoi.geojson`, `grid.geojson` | AOI WGS84 y grilla interna UTM |
| `nisar/create_grid.py`, `check_grid.py` | Generar y validar grilla |
| `01_search.py`, `02_subset.py`, `03_change.py`, `06_cells.py` | Entradas CLI, sin lógica duplicada |
| `search.py`, `subset.py`, `change.py`, `cells.py` | Funciones importables de esas cuatro etapas |
| `common.py`, `contracts.py` | Proyección/JSON y lectura/validación de contratos centrales |
| `integration.py`, `overlays.py`, `assemble.py` | Consumir contexto, exportar PNG y preparar evidencia para Rust |
| `config/risk-config.json` | Única configuración de umbrales del contrato; `06 --config` permite elegirla |
| `tests/`, `pytest.ini` | Pruebas numéricas, geoespaciales, de contratos y recorrido integral |

- Bbox provisional `[-80.80,-5.35,-80.55,-5.10]`. **812 cuadrados completos de 1000 × 1000 m**,
  alineados a 1000 m en **EPSG:32717**. Filas desde el norte, columnas desde el oeste;
  propiedades solo `cell_id`. La grilla declara CRS proyectado: no es GeoJSON RFC 7946 lon/lat.
- Comparabilidad estricta: track/frame, órbita, modo, frecuencia, polarización y madurez conocidas e iguales.
  Se prioriza PROVISIONAL, luego VALIDATED, luego BETA, sin mezclarlas. Se excluye el día UTC 20 oct;
  se elige T1 más reciente y luego T2 más temprano entre pares comparables. Una consulta parcial se bloquea.
- Gamma-cero diagonal, `mask` y `numberOfLooks` se leen de
  `/science/LSAR/GCOV/grids/frequency[A|B]/`. Se comprueban identificación HDF5, coordenadas de centro
  y resolución nativa cuadrada idéntica. Malla común UTM por vecino más cercano; fuera del AOI, nodata.
- Quality gate: máscara 1..`numberOfSubSwaths`, looks finitos >0 y gamma finito >0 en ambas fechas.
  Máscara 0=inválido, 255=fuera de adquisición. Se excluyen anomalías de `inputDataExceptionMask` si existe.
  `delta_db=10*log10((gamma_t2+1e-10)/(gamma_t1+1e-10))`, con signo y umbrales inclusivos ±3 dB.
- `valid_fraction` usa todos los píxeles de la celda; decrease/increase/changed_fraction usan válidos;
  p90 del cambio absoluto es nulo sin válidos. Rust aplica calidad mínima .60 y decide estados/ranking.
- PNG reproyectados a WGS84: T1/T2 usan percentiles 2–98 conjuntos; delta fija ±6 dB, azul disminución,
  rojo aumento e inválidos transparentes. `bounds.json`: TL, TR, BR, BL; `scales.json`: escalas empleadas.

## Entradas de Jelibeth y salidas

`roads.geojson`: FeatureCollection WGS84 con LineString/MultiLineString sin segmentos duplicados.
`osm-metadata.json`: `query`, `retrieved_at` ISO8601 con zona y
`attribution: "© OpenStreetMap contributors"`. Sin ambos se bloquea la exportación: no se inventan
metros viales. Se calcula longitud UTM total y dentro del cambio dilatado 250 m, por celda.

OPERA opcional: `06 --opera RUTA/opera-context.json`, con resultados **ya calculados por Jelibeth**:
`status`, `product: OPERA_L3_DSWx-S1`, `derived_from: Sentinel-1`, `specification_url`, `class_mapping`,
`nisar: {t1: ID_NISAR_T1, t2: ID_NISAR_T2}`, `delta_db_threshold`,
`t1` y `t2` con `granules` y `datetime`, y `cells` como mapa cell_id → overlap_ratio (0..1 o null).
Jelibeth calcula agua nueva y solapamiento en malla UTM de 30 m según §6.6; este consumidor comprueba
par, umbral y desfase ≤6 días. Fracción ≥.20 implica corroboración. Sin contexto, celda ausente,
null o desfase excesivo: `opera_status: unavailable` y overlap null. No se procesan productos OPERA aquí.

`out/piura-01/` recibe TIFF intermedios, metadatos, `cells.geojson` WGS84, `manifest.json`, vías,
configuración y overlays cuando existan datos. `provenance.json` conserva URLs y tiempos de extracción
que el contrato del manifiesto no admite; no inventa fechas de descarga. Los esquemas se leen de
`contracts/v1/` o mediante `git show origin/main:contracts/v1/...`, sin modificarlos.
`assemble.py` prepara evidencia sin firmar en `staging/data/piura-01/v1/` y rechaza sobrescribir versiones.
Rust completa el bundle público `data/piura-01/v1/`: índice, hashes, emisores y cadena WONB.
Validar ciencia no equivale a superar el verificador oficial del bundle.

## Identidad y aislamiento por AOI

`data-pipeline/nisar/` es el flujo oficial actualmente configurado para `piura-01`
(`EPSG:32717`). El flujo exploratorio multi-AOI vive en `data-pipeline/validacion/`;
no se debe reutilizar la grilla ni el CRS de Piura para otra región. Usa los CRS
de `docs/aoi-integration-contract.md`: Tailandia `EPSG:32647`, Huaraz
`EPSG:32718`, y los AOI peruanos restantes `EPSG:32717`.

Antes de pasar un paquete al exportador Rust, valida su aislamiento y propósito:

```bash
python data-pipeline/validacion/validate_aoi_integration.py \
  --in RUTA/DEL/PAQUETE \
  --purpose scientific \
  --inputs data-pipeline/inputs/ID_DEL_AOI \
  --roads data-pipeline/inputs/ID_DEL_AOI/roads.geojson
```

`scientific` requiere t1/t2 seleccionados y comparables; `data_origin` sigue
determinando si el contenido es real o un fixture simulado, que nunca debe
presentarse como evidencia real. `baseline` y `preparation` son explícitos y no
pueden generar Risk ni Action. El validador
rechaza AOI/CRS inconsistentes, identidad de celda o ruta cruzada, fuentes sin
procedencia, y cadenas WONB con padres de otro AOI. Las fracciones en
`cells.geojson` son evidencia no firmada; Rust conserva la serialización y
firma de enteros en los payloads WONB.

## Estado y referencias

Una consulta ASF registrada el 14 nov 2026 para la ventana 1 oct–10 nov encontró 4 escenas
PROVISIONAL (7 y 19 oct), 2 VALIDATED (31 oct) y 0 BETA; no encontró un par de igual madurez
a ambos lados del evento. Las respuestas y preflight de ASF quedan en `out/piura-01/`, salida
generada, ignorada y no versionada. El flujo oficial no tiene un bundle de evento verificable:
Piura sigue bloqueado para una alerta científica. Hay una línea base comparable 7–19 oct en
`data-pipeline/validacion/resultados/peru-fuentes-nasa/ciudades/piura-01/`, pero no representa
el evento; el par exploratorio 19–31 oct tiene madurez mixta. Las vías OSM locales están en
`inputs/piura-01/`; su presencia no desbloquea el par NISAR. El bundle local
`data/tailandia-01/v1/` es provisional e inmutable, y sus rutas/audiencias de Action y nota CRS
son contradictorias: no reutilizarlo para Piura ni enrutar sus acciones. Los HDF5 fabricados
por tests permanecen solo en temporales.

[GCOV ASF](https://nisar-docs.asf.alaska.edu/gcov/) y
[JPL D-102274 Rev F, v1.5.0](https://cumulus.asf.earthdatacloud.nasa.gov/PUBLIC/DATA/NISAR/NISAR_D-102274_RevF_NASA_SDS_Product_Specification_L2_GCOV_w-esigs.pdf):
§5.2 identificación, §5.3 datasets, pp.36/44 máscaras A/B. Se eliminó el PDF temporal.
[ASF Search](https://nisar-docs.asf.alaska.edu/asf-search/) y
[colecciones oficiales](https://nisar-docs.asf.alaska.edu/earthaccess/): asf_search 10.3.0 usa `shortName`
porque no admite `dataMaturity`: `NISAR_L2_GCOV_PROVISIONAL_V1`, `NISAR_L2_GCOV_V1` y `NISAR_L2_GCOV_BETA_V1`.
