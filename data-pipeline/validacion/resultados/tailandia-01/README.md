# Prueba del método con una inundación real: llanura central de Tailandia

**Fecha:** 14 nov 2026 · Datos del evento tomados de resúmenes de prensa y agencias, sin verificar en los reportes originales. Contrastado con OPERA DSWx-S1.

Aquí "validación" significa prueba del método. Los resultados se describen como "cambio observado, corroborado por OPERA DSWx-S1 (Sentinel-1)"; no como "inundación detectada" ni "validado" (plan maestro R2 y §17.4).

## En una frase

Con las herramientas de `data-pipeline/validacion/`, un par NISAR real de la misma madurez muestra en Tailandia un cambio compatible con agua abierta que coincide con la inundación reportada en septiembre de 2026: **la mitad del área válida bajó 3 dB o más**, frente a 2,7 % en el par previo sin inundación.

## Por qué Tailandia

- Inundación extensa y prolongada desde el 16 de septiembre de 2026, agravada desde el 25; 53 provincias afectadas a fines de octubre. El agua seguía ahí cuando pasó el satélite.
- Todo ocurre antes del 21 de octubre: las escenas son todas PROVISIONAL y ya están publicadas. No hay que mezclar madureces (R7).
- En Perú los pares disponibles no sirven para esta prueba: el de Piura que cruza el 20 de octubre mezcla madureces, y en Tumbes y Piura el satélite pasó 4 y 11 días después de la lluvia.

## Datos usados

Recuadro: `100.30 14.30 100.52 14.52` (oeste de Ayutthaya), 600 celdas de 1 km en `EPSG:32647`. NISAR L2 GCOV, pista 004, frame 081, descendente, modo 4005, frecuencia A, HHHH, 10 m, cobertura 100 %.

| Par | Fechas | Uso |
|---|---|---|
| Base | 13 ago → 25 ago | Sin inundación: máscara de agua permanente y cambio habitual |
| Evento | 6 sep → 30 sep | Antes del inicio (16 sep) y después del agravamiento (25 sep) |
| Evento, para OPERA | 18 sep → 30 sep | Encierra el agravamiento; OPERA queda a 1 día de cada escena |

Los 6 chequeos de comparabilidad pasan en los tres pares (`comparable: true`). Los nombres exactos de los granules están en cada `manifest.json` y en `candidates.json`.

## Resultado

| | Base (sin inundación) | Evento 6 → 30 sep | Evento 18 → 30 sep |
|---|---|---|---|
| Área válida que bajó ≥ 3 dB | 2,7 % | **48,6 %** | 36,2 % |
| Área válida que subió ≥ 3 dB | 1,8 % | 0,5 % | 0,6 % |
| Mediana del cambio | −0,01 dB | −2,80 dB | −1,48 dB |
| Celdas con > 25 % de cambio | — | **369 de 503** | 305 de 503 |
| Celdas con > 50 % de cambio | — | 251 de 503 | 150 de 503 |

De las 600 celdas, 97 quedan con menos de 60 % de área válida por la máscara y no cuentan.

El cambio es casi todo bajada, compatible con agua abierta (reflexión especular). En `imagenes/despues_30sep.png` los campos aparecen negros y solo quedan claros los diques, caminos y poblados a lo largo de los ríos.

Método: promediado de gamma0 a 100 m, umbral de ±3 dB y máscara de exclusión construida con el par base (22,5 % del recuadro: 2,5 % agua permanente, 4,5 % cambio habitual, más un bloque de margen). Ruido de speckle esperado tras promediar: 0,29 dB.

## Contraste independiente con OPERA DSWx-S1 (Sentinel-1)

OPERA es un producto NASA derivado de Sentinel-1, no de NISAR (R4). Capa WTR; agua = clases 1 (agua abierta) y 3 (vegetación inundada). "Agua nueva" = agua en la fecha posterior y no en la anterior, válida en ambas (§6.6).

| | Dentro de la regla | Complementario |
|---|---|---|
| Par NISAR | 18 sep → 30 sep | 6 sep → 30 sep |
| OPERA | 19 sep → 1 oct (1 y 1 días) | 15 sep → 1 oct (9 y 1 días; fuera de regla) |
| Área con cambio NISAR | 36,8 % | 49,2 % |
| Área con agua nueva OPERA | 27,1 % | 27,6 % |
| Del cambio NISAR, OPERA ve agua nueva | **53,6 %** | 49,5 % |
| Del agua nueva OPERA, NISAR marcó cambio | **73,0 %** | 88,3 % |
| Donde NISAR no ve cambio, OPERA ve agua nueva | 11,6 % | 6,3 % |

Por celda (par dentro de la regla, umbral de corroboración 20 %): de las 305 celdas con más de 25 % de cambio, **239 quedan corroboradas** por OPERA.

Las dos fuentes coinciden en lo principal. NISAR marca más área que OPERA; con estos datos no se puede decir cuál acierta en la diferencia (banda L y banda C responden distinto bajo vegetación, y en una llanura arrocera parte del agua puede ser de cultivo).

## Cómo se comprobó

| Qué | Cómo | Resultado |
|---|---|---|
| El código calcula bien | 13 pruebas automáticas con datos fabricados de respuesta conocida | Pasan las 13 |
| Lee bien las escenas reales | El nombre del granule se contrasta con sus metadatos internos; la estructura coincide con la especificación JPL D-102274 | Coincide en todas las escenas usadas |
| El ruido está bien estimado | Desviación de `delta_db` esperada por speckle frente a la medida (Piura, 20 m, sin promediar) | 2,08 dB esperada, 2,11 dB medida |
| Distingue evento de no evento | Mismo recuadro, par sin inundación frente a par con inundación | 2,7 % frente a 48,6 % |
| No marca cambio donde no hay evento | Par sin evento en Tumbes (Zarumilla), 7 → 19 oct | 0 de 443 celdas sobre 25 % |
| Lo respalda otra fuente | OPERA DSWx-S1 a 1 día de cada escena | 239 de 305 celdas corroboradas |

No comprobado: exactitud frente a datos de campo o a un mapa oficial de la emergencia (Sentinel Asia, GISTDA); por qué NISAR marca más área que OPERA; cuánta agua es de cultivo; el ranking y la firma de Rust con estos datos.

## Límites

- No dice nada sobre Perú. Es una prueba del método, no el caso del proyecto; no aporta a Local Impact.
- El recuadro está en `EPSG:32647` y el contrato v1 fija `EPSG:32717`: estos `manifest.json` no cumplen el esquema y **esto no es un bundle**. Por eso está aquí y no en `data/`.
- Los `manifest.json` no llevan `osm` (no se calcularon vías) y sus `cells.geojson` tienen `road_m_*` en 0.
- `pipeline.commit` en los manifest es el `HEAD` del repositorio al momento de correr (`ce7ae33`); las herramientas se versionaron después, en `c4570ed`.
- OPERA también es radar: es independiente de NISAR, pero comparte algunas limitaciones.
- Los umbrales (±3 dB, 20 %) son los demostrativos del plan.

## Contenido de esta carpeta

| Archivo | Qué es |
|---|---|
| `imagenes/antes_06sep.png`, `despues_30sep.png` | Gamma0 HHHH en dB, mismo estiramiento, reducidas a un tercio |
| `imagenes/cambio_06sep_30sep.png`, `cambio_18sep_30sep.png` | `delta_db` a 100 m: azul bajó, rojo subió, transparente enmascarado |
| `grid.geojson` | Las 600 celdas, `EPSG:32647` |
| `candidates.json` | Granules NISAR encontrados sobre la zona, con URL y cobertura |
| `exclude.json` | Parámetros y fracciones de la máscara de exclusión |
| `base-13ago-25ago/` | `manifest.json` y `change_summary.json` del par sin inundación |
| `evento-06sep-30sep/` | `manifest.json`, `change_summary.json`, `cells.geojson` (OPERA `unavailable`: queda a 9 días), `bounds.json` y el contraste OPERA complementario |
| `evento-18sep-30sep/` | Lo mismo, con `opera_cells.json` y `cells.geojson` con OPERA dentro de la regla |
| `opera/granules.json` | Granules OPERA DSWx-S1 consultados |

No se incluyen los rásteres (`t1.tif`, `delta_db.tif`, la máscara) ni las imágenes a tamaño completo: pesan cientos de megas y se regeneran.

## Cómo repetirlo

Cada persona necesita su login de Earthdata (`python data-pipeline/validacion/earthdata_login.py`). Desde la raíz del repo:

```bash
python data-pipeline/validacion/01_search.py --bbox 100.25 14.20 100.70 14.80 --start 2026-08-10 --end 2026-10-22 --event 2026-09-20 --out data-pipeline/out/validacion/tailandia-01/candidates.json
python data-pipeline/validacion/00_grid.py --aoi tailandia-01 --crs EPSG:32647 --bbox 100.30 14.30 100.52 14.52 --out <carpeta>/inputs-provisional/grid.geojson
```

Después, con el par base: `02_subset.py`, `03_change.py` y `make_exclude.py`. Con cada par del evento: `02_subset.py`, `03_change.py --exclude`, `validate_opera.py` (solo 18 → 30 sep) y `06_cells.py`.

## Fuentes

- Resumen de la inundación (NASA Earth Observatory): https://science.nasa.gov/earth/earth-observatory/floodwaters-overwhelm-thailand/
- Activación de Sentinel Asia, Bangkok, 26 sep 2026: https://sentinel-asia.org/EO/2026/article20260926TH.html
- AHA Centre, reporte semanal 19–25 oct 2026: https://ahacentre.org/weekly-disaster-update/weekly-disaster-update-19-25-october-2026/
- AHA Centre, reporte semanal 5–11 oct 2026: https://ahacentre.org/weekly-disaster-update/weekly-disaster-update-5-11-october-2026/
- Disponibilidad y madurez de NISAR: https://nisar-docs.asf.alaska.edu/availability-overview/
- Especificación NISAR L2 GCOV, JPL D-102274 Rev F: https://cumulus.asf.earthdatacloud.nasa.gov/PUBLIC/DATA/NISAR/NISAR_D-102274_RevF_NASA_SDS_Product_Specification_L2_GCOV_w-esigs.pdf
- Clases de OPERA DSWx-S1: https://developers.google.com/earth-engine/datasets/catalog/OPERA_DSWX_L3_V1_S1 y especificación JPL D-108761 Rev A: https://archive.podaac.earthdata.nasa.gov/podaac-ops-cumulus-docs/web-misc/opera/D-108761_OPERA_DSWx_S1AB_ProductSpec_Rev_A.pdf
