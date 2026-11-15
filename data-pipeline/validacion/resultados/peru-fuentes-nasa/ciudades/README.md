# NISAR por ciudad: Áncash, Lambayeque, Lima, Ica, Tumbes y Piura

**Fecha:** 15 nov 2026 · Datos NISAR L2 GCOV reales, HHHH, leídos por partes desde ASF con `data-pipeline/validacion/`. Recuadros de unos 15 km. Exploración: nada de esto está firmado ni es un bundle.

Huaraz, Tumbes, Piura y Jequetepeque tienen el mismo contenido que el par completo de Tailandia (`../../tailandia-01/evento-18sep-30sep/`): cambio NISAR, contraste OPERA DSWx-S1 y vías de OSM por celda. Sus `manifest.json` y `cells.geojson` cumplen el contrato v1.1.0.

## En una frase

En casi todas las ciudades la señal de NISAR es estable. Hay dos excepciones: **Huaraz**, donde el 17 de octubre el 58,7 % del área subió 3 dB o más con un par de igual madurez (0,0 % en el par previo sin evento), y **Tumbes**, donde la señal sube 0,7 a 1,2 dB desde fines de septiembre. En ningún caso aparece la bajada extensa que indicaría agua abierta, y OPERA no ve agua nueva donde NISAR marca cambio.

## Serie por ciudad

Todas las fechas disponibles desde mediados de agosto. "Cambio" es la mediana de `delta_db` entre pasadas consecutivas, a 100 m; el porcentaje es el área con cambio ≥ 3 dB. Detalle en `nisar-series.json`.

| Ciudad | Pista / frame | Fechas | Pares de igual madurez | Lo más alto que se ve |
|---|---|---|---|---|
| **Áncash – Huaraz** | 083 / 093 | 7 (18 ago – 29 oct) | Estable (±0,1 dB) hasta el 5 oct | **5 → 17 oct: +3,33 dB, 58,6 % subió ≥ 3 dB** |
| Áncash – Huarmey (control) | 083 / 094 | 7 (18 ago – 29 oct) | Estable (±0,15 dB) | 5 → 17 oct: +0,14 dB, 0,2 % |
| Áncash – Chimbote | 011 / 093 | 7 (25 ago – 5 nov) | Estable (±0,1 dB), incluido 24 oct → 5 nov VALIDATED | 1 % hacia cada lado |
| Lambayeque – La Leche | 003 / 174 | 6 (25 ago – 24 oct) | Estable (−0,2 a 0,0 dB) | 0,4 % |
| Lambayeque – La Leche (segunda pasada) | 104 / 174 | 6 (20 ago – 31 oct) | Estable (−0,24 a −0,01 dB); solo ~40 % del recuadro con dato válido | 1,7 % |
| Lima – Rímac / Chosica | 075 / 172 | 6 (18 ago – 29 oct) | Estable (±0,35 dB) | 0,1 % |
| Ica – valle | 075 / 171 | 6 (18 ago – 29 oct) | Estable (±0,15 dB) | 0,9 % |
| **Tumbes – Zarumilla** | 112 / 090 | 7 (20 ago – 31 oct) | Estable hasta el 13 sep; +0,68 dB al 25 sep; **+1,21 dB al 19 oct** | 0,8 % subió ≥ 3 dB |
| **Tumbes – Zarumilla** | 003 / 176 | 6 (25 ago – 24 oct) | Estable hasta el 18 sep; **+1,20 dB al 30 sep**; +0,50 dB al 12 oct | 1,0 % subió ≥ 3 dB |
| Piura – ciudad y Bajo Piura | 112 / 091 | 7 (20 ago – 31 oct) | Estable (±0,1 dB), incluido 7 → 19 oct | 1,3 % |

## Huaraz, 5 → 17 de octubre

Par PROVISIONAL → PROVISIONAL, mismo frame, modo y polarización: `comparable: true`. Carpeta `huaraz-01/`.

| | Base 11 → 23 sep (sin evento) | Evento 5 → 17 oct |
|---|---|---|
| Mediana del cambio | −0,10 dB (p02 −1,07, p98 +0,81) | **+3,33 dB** (p02 +0,83, p98 +6,95) |
| Área que subió ≥ 3 dB | 0,0 % | **58,7 %** |
| Área que bajó ≥ 3 dB | 0,0 % | 0,0 % |
| Celdas de 1 km con > 25 % de cambio | — | **216 de 240** |
| Celdas con > 50 % de cambio | — | 152 de 240 |
| Pasada | 23:24 UTC | 17 oct, 23:24 UTC (18:24 hora local) |

Máscara de exclusión, igual que en Tailandia: construida con el par base (cambio habitual) y el agua de OPERA del 4 de octubre. Excluye el 0,8 % del recuadro. Sin máscara el resultado es 58,6 % y 217 celdas.

Lo que respalda que es un cambio real del terreno:

- **El mismo recuadro, un mes antes, no cambia nada** (par base, misma pista y frame): 0,0 % frente a 58,7 %.
- **Huarmey, en la misma pasada del satélite y el frame vecino, no cambia** (+0,14 dB). No es un problema de esa adquisición.
- **El patrón sigue el terreno:** sube en laderas y pastizales y casi no cambia en la ciudad (`overlays/delta.png`). Un fallo de calibración sería uniforme; una interferencia, en franjas.
- **Doce días después vuelve a bajar** (−2,56 dB al 29 oct, medido entre madureces distintas).

Lo que no está resuelto:

- **La causa.** Es compatible con suelo y vegetación mojados por lluvia reciente. NASA POWER da solo 4 mm ese día (es un promedio de ~50 km, mal indicador de tormentas de sierra). No se encontró un reporte de prensa del 17 de octubre; sí del 25 de septiembre y del 6–9 de noviembre (lluvia con granizada y calles anegadas). El aviso rojo de Senamhi para la zona empieza el 20 de octubre.
- **Es subida, no bajada.** El plan asocia la bajada con agua abierta. Esto no es una inundación: es "cambio observado" sobre casi toda la zona.
- **OPERA no lo corrobora, y es lo esperable.** OPERA mapea agua en superficie, no suelo mojado: entre el 4 y el 16 de octubre no ve agua nueva (0 de 240 celdas corroboradas). Además su segunda fecha es un día anterior a la pasada de NISAR.
- **Es sierra.** El relieve complica el radar, aunque el producto ya viene corregido por terreno.
- **Zona UTM 18S** (`EPSG:32718`), válida desde el contrato v1.1.0.

## Tumbes: la señal sigue a la lluvia

En las dos pistas la señal es plana hasta mediados de septiembre y sube desde fines de septiembre, cuando empiezan las lluvias. Coincide con la humedad de suelo de SMAP (ver `../README.md`). Son cambios de 0,7 a 1,2 dB sobre toda el área, por debajo del umbral de 3 dB: el control `tumbes-01/control-07oct-19oct/` da 0 celdas con más de 25 % de cambio.

## Sesgo entre madureces (R7), medido

En las cinco zonas secas, donde nada más cambia, el paso de una escena PROVISIONAL a una VALIDATED baja la señal:

| Zona | PROVISIONAL → VALIDATED | Pares de igual madurez |
|---|---|---|
| Lambayeque – La Leche | −0,42 dB | −0,22 a +0,04 dB |
| Áncash – Chimbote | −0,38 dB | −0,12 a +0,06 dB |
| Áncash – Huarmey | −0,49 dB | −0,15 a +0,14 dB |
| Lima – Rímac / Chosica | −0,38 dB | −0,35 a +0,11 dB |
| Ica – valle | −0,56 dB | −0,15 a +0,04 dB |

Es un desplazamiento sistemático de unos 0,4 a 0,6 dB debido al procesamiento. Es pequeño frente al umbral de 3 dB, pero real: es la razón de ser de R7, y hay que descontarlo si alguna vez se comparan madureces.

## Las cuatro ciudades con pipeline completo

Mismo método en todas: promediado a 100 m, umbral de ±3 dB, máscara de exclusión, contraste con OPERA y vías a menos de 250 m del cambio. "Con calidad" son las celdas con al menos 60 % de área válida.

### Cambio NISAR

| Ciudad | Par | Madurez | Mediana | Bajó / subió ≥ 3 dB | Celdas con > 25 % de cambio | Máscara |
|---|---|---|---|---|---|---|
| **Huaraz** | 5 → 17 oct | PROVISIONAL | +3,33 dB | 0,0 % / **58,7 %** | **216 de 240** | 0,8 %: par base 11 → 23 sep y agua OPERA |
| Tumbes – Zarumilla | 7 → 19 oct | PROVISIONAL | +1,02 dB | 0,0 % / 0,4 % | 0 de 443 | 9,9 %: par base de la pista 003 |
| Piura – Bajo Piura | 7 → 19 oct | PROVISIONAL | +0,84 dB | 0,2 % / 3,8 % | 29 de 808 | 2,0 %: solo agua OPERA del 6 oct |
| Jequetepeque | 24 oct → 5 nov | VALIDATED | −0,15 dB | 1,3 % / 0,5 % | 4 de 518 | 22,4 %: mar y agua OPERA del 25 oct |

El par de Piura es de la pista 104 (11:09 UTC, 19 oct) y termina un día antes de las lluvias del 20 de octubre: es el estado previo, no el evento. En Jequetepeque 149 celdas caen en el mar y quedan sin calidad.

### Contraste con OPERA DSWx-S1 (Sentinel-1)

Capa WTR; agua = clases 1 y 3; "agua nueva" = agua en la fecha posterior y no en la anterior (§6.6). En todos los casos OPERA queda a 1 día de cada escena NISAR (máximo permitido: 6) y las dos fechas son del mismo satélite y la misma órbita.

| Ciudad | OPERA usado | Cobertura válida | Área con agua nueva OPERA | Del cambio NISAR, OPERA ve agua nueva | Celdas corroboradas |
|---|---|---|---|---|---|
| Huaraz | 4 → 16 oct, 23:33 UTC | 94,9 % | 0,0 % | 0,0 % | 0 de 240 |
| Tumbes – Zarumilla | 6 → 18 oct, 11:01 UTC | 100 % | 0,7 % | 1,8 % | 1 de 493 |
| Piura – Bajo Piura | 6 → 18 oct, 11:01 UTC | 100 % | 0,1 % | < 0,1 % | 0 de 812 |
| Jequetepeque | 25 oct → 6 nov, 10:53 UTC | 99,4 % | 0,1 % | 4,2 % | 5 de 667 |

Lectura: en ninguna ciudad hay agua nueva extensa, y lo que NISAR marca no es agua según OPERA. Es coherente con el resto de esta carpeta: lo que NISAR registra en Perú es terreno mojado, no agua en superficie. En Jequetepeque, 2 de las 4 celdas con más de 25 % de cambio quedan corroboradas; el valle es arrocero, así que puede ser riego de cultivo y no inundación (no verificado; R13).

Dos pasadas de Sentinel-1 del mismo día que NISAR (Huaraz, 5 y 17 oct a las 23:25; Jequetepeque, 24 oct y 5 nov a las 11:01) aparecen en la búsqueda pero no traen dato sobre el recuadro. Se usó la pasada vecina.

### Vías de OpenStreetMap

Descarga de Overpass del 15 nov 2026, © OpenStreetMap contributors, con las mismas clases de vía que usó el equipo en `piura-01` y `tailandia-01`. Los archivos están en `data-pipeline/inputs/<aoi>/` (`roads.geojson` y `osm-metadata.json`); los de Piura son los del equipo.

Los metros por celda (`road_cells.json`) salen de `05_roads.py`, el paso de vías del equipo (commit `770055b` de `main`). El cálculo propio de `06_cells.py` da lo mismo: el total coincide y la parte cercana al cambio difiere menos de 0,2 %.

| Ciudad | Tramos | Vías en la grilla | A menos de 250 m del cambio | Celdas con > 25 % de cambio y vías cerca |
|---|---|---|---|---|
| Huaraz | 1 527 | 595 km en 198 celdas | 544 km (91 %) | 174 de 216 |
| Tumbes – Zarumilla | 3 093 | 1 395 km en 380 celdas | 288 km (21 %) | no hay celdas con > 25 % |
| Piura – Bajo Piura | 14 572 | 4 012 km en 648 celdas | 758 km (19 %) | 16 de 29 |
| Jequetepeque | 3 641 | 1 260 km en 390 celdas | 299 km (24 %) | 4 de 4 |

## Carpetas

Cada ciudad sigue la estructura de `../../tailandia-01/`: `candidates.json`, `grid.geojson`, `exclude.json` y una carpeta por par con `manifest.json` (con `opera` y `osm`), `change_summary.json`, `cells.geojson`, `opera_cells.json`, `road_cells.json` y `overlays/` (`t1.png`, `t2.png`, `delta.png`, `bounds.json`, reducidos).

| Carpeta | Qué es |
|---|---|
| `huaraz-01/evento-05oct-17oct/` | El par del cambio |
| `huaraz-01/base-11sep-23sep/` | Par sin evento: `manifest.json`, `change_summary.json` y `candidates.json` |
| `tumbes-01/control-07oct-19oct/` | Par sin evento en Zarumilla |
| `piura-01/previo-07oct-19oct/` | Estado previo a las lluvias del 20 oct |
| `jequetepeque-01/par-24oct-05nov/` | Único par VALIDATED |
| `nisar-series.json` | Serie de todas las ciudades de la primera tabla |
| `nisar-serie-huarmey-control.json` | Control de la pasada del 17 oct |

Lambayeque, Chimbote, Lima e Ica solo tienen la serie; no se corrió el pipeline por celdas porque no hay cambio que ordenar.

## Límites

- Es evidencia sin firmar, **no un bundle**: el ranking y la firma los hace Rust en `data/`.
- `pipeline.commit` de los manifests es `b9782df`, el `HEAD` al correr. La regla de cobertura de `validate_opera.py` (celda con menos de la mitad cubierta por OPERA queda `unavailable`) se agregó en el mismo cambio que sube estos archivos; no altera ninguna celda de estas ciudades ni de Tailandia. En el mismo cambio, `crs_note` de `cells.geojson` pasa a decir la zona UTM real de cada grilla; el `cells.geojson` de Tailandia ya publicado todavía dice `EPSG:32717` en esa nota (sus cálculos son en `EPSG:32647`, como indica su manifest).
- El par base de Huaraz es solo referencia: su manifest no lleva `osm` ni `opera` y no cumple el contrato, igual que los pares base de Tailandia.
- Las máscaras no son iguales: Huaraz y Tumbes tienen par base; Piura y Jequetepeque solo excluyen agua, porque no se procesó un par previo sin evento.
- `tumbes-01` y `piura-01` son recuadros de validación. El AOI oficial del proyecto y su pipeline son los de `data-pipeline/nisar/`.
- OPERA es radar de banda C: detecta mal el agua en zona urbana y bajo vegetación, y no ve suelo mojado.
- Las longitudes de vías se miden en la zona UTM de cada grilla; Rust calcula su total con otra fórmula y puede diferir en torno al 2 %.

## Fuentes

- NISAR L2 GCOV (ASF DAAC): https://nisar-docs.asf.alaska.edu/gcov/
- Lluvia con granizada en Huaraz, 25 sep 2026 (El Comercio): https://elcomercio.pe/peru/ancash/huaraz-soporta-intensa-lluvia-con-truenos-y-granizada-calles-quedaron-anegadas-fenomeno-el-nino-apuesplan-ultimas-noticia/
- Lluvias y granizada en Huaraz, 6–9 nov 2026 (Andina): https://andina.pe/agencia/noticia-ancash-lluvias-intensas-inundan-calles-y-afectan-viviendas-y-locales-publicos-huaraz-1098984.aspx
- Avisos de Senamhi Áncash: https://www.senamhi.gob.pe/main.php?dp=ancash&p=aviso-24H
