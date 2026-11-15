# Sechura: candidato de Perú, preparado para NISAR

**Fecha:** 15 nov 2026, 00:10 (Lima) · **Para:** Alex y Gustavo · Exploración; nada de esto es entregable todavía.

## Por qué Sechura

Un barrido de la costa norte con OPERA DSWx-S1 (NASA) muestra un solo lugar con aumento neto grande de agua entre inicios de octubre e inicios de noviembre: el sur del desierto de Sechura, unos 100 km al sur de la ciudad de Piura. Detalle en `../README.md`.

- El cambio aparece tras la lluvia del 1–2 de noviembre y sigue el 8 de noviembre: no depende de que el satélite pase el mismo día.
- Las pasadas de la tarde de Sentinel-1 ven ~11 000 ha de agua nueva; las de la mañana, ~2 000 ha. **No está confirmado que sea agua abierta.**

## Qué quedó preparado

Recuadro `-80.98 -6.28 -80.72 -6.02`: 870 celdas de 1 km, `EPSG:32717`. Los rásteres de trabajo quedan en `data-pipeline/out/validacion/sechura-01/`, fuera del repositorio; aquí van las fichas.

| Pieza | Archivo en esta carpeta | Estado |
|---|---|---|
| Grilla | `grid.geojson` | 870 celdas |
| Escenas encontradas | `candidates.json` | Con URL, cobertura y madurez |
| Par previo NISAR | `control-07oct-19oct/manifest.json` | 7 oct → 19 oct, pista 104, frame 174, modo 2005, 20 m, PROVISIONAL, `comparable: true`, cobertura 100 % |
| Máscara (R13) | `exclude.json` (parámetros; el ráster se regenera) | 25 % excluido. El agua permanente sale de OPERA (9 y 12 oct), no del brillo del radar |
| Control sin evento | `control-07oct-19oct/cells.geojson`, `change_summary.json`, `bounds.json` | Par previo con la máscara: **0 celdas** sobre 10 % de cambio, de 660 con calidad |
| Imágenes | `imagenes/antes_07oct.png`, `control_19oct.png`, `control_cambio_07oct_19oct.png` | γ⁰ HHHH y `delta_db` del par previo, reducidas a la mitad |

Misma estructura que `../../tailandia-01/` (README, `candidates.json`, `grid.geojson`, `exclude.json`, una carpeta por par con `manifest.json`, `change_summary.json`, `cells.geojson` y `bounds.json`, e `imagenes/`). **Falta lo que en Tailandia sí hay**: el par del evento y el contraste OPERA por celda, que dependen de la escena NISAR del 12 nov.

El manifest del control no lleva `osm` y sus celdas tienen OPERA `unavailable` y vías en 0: es un control del método, no un bundle.

## Par del evento

**31 oct → 12 nov**, pista 104, frame 174, las dos VALIDATED. La del 31 oct está publicada; la del 12 nov no (el dato crudo sí está en el archivo de ASF). Alternativa: pista 112, frame 092, modo 4005, mismas fechas.

## Advertencia: puede volver a salir plano

En la escena NISAR del 7 de octubre, con el terreno seco, la zona que OPERA marca después como agua nueva ya es muy oscura:

| Zona | γ⁰ HHHH mediano, 7 oct |
|---|---|
| Mar y agua previa | −18,1 dB |
| Zona que OPERA marca como agua nueva el 2 nov | **−21,4 dB** |
| Resto del terreno | −19,1 dB |

En banda L la arena seca y lisa refleja tan poco como el agua. Si esa planicie se cubre de agua, la señal apenas puede bajar; el umbral de −3 dB es difícil de alcanzar desde −21 dB. Es posible ver subidas donde el suelo se moja y se vuelve más rugoso, o no ver nada. Solo se sabrá al correrlo.

En el norte del recuadro hay además una instalación industrial con pozas rectangulares; lo que cambie ahí no es inundación.

## Comandos para cuando aparezca la escena del 12 nov

Desde la raíz del repo, con el entorno de Python activo.

```bash
python data-pipeline/validacion/01_search.py --bbox -80.98 -6.28 -80.72 -6.02 --start 2026-10-25 --end 2026-11-20 --event 2026-11-01 --out data-pipeline/out/validacion/sechura-01/candidates.json
```

Si lista un "PAR que cruza el evento" para la pista 104, frame 174:

```bash
python data-pipeline/validacion/02_subset.py --aoi sechura-01 --aoi-name "Desierto de Sechura, sur" --grid data-pipeline/out/validacion/sechura-01/base-07oct-19oct/inputs-provisional/grid.geojson --candidates data-pipeline/out/validacion/sechura-01/candidates.json --out data-pipeline/out/validacion/sechura-01/evento-31oct-12nov --t1 NISAR_L2_PR_GCOV_034_104_A_174_2005_DHDH_A_20261031T110840_20261031T110901_P05036_N_P_J_001 --t2 <GRANULE_DEL_12_NOV>
python data-pipeline/validacion/03_change.py --in data-pipeline/out/validacion/sechura-01/evento-31oct-12nov --exclude data-pipeline/out/validacion/sechura-01/exclude.tif
python data-pipeline/validacion/06_cells.py --in data-pipeline/out/validacion/sechura-01/evento-31oct-12nov --inputs data-pipeline/out/validacion/sechura-01/base-07oct-19oct/inputs-provisional
```

Contraste con OPERA: las pasadas de la mañana del 30 oct y del 11 nov quedan a 1 día de cada escena NISAR (dentro de la regla de 6 días) y son las que ven menos agua; la de la tarde del 8 nov queda a 4 días. Conviene reportar las dos.

## Lo que falta para que sea un caso del proyecto

- La escena NISAR del 12 nov.
- Confirmar con otra fuente qué es el cambio: una imagen óptica sin nubes (la última HLS es del 4 nov, 82 % de nubes) o el radar Sentinel-1 directo (OPERA RTC-S1), que requiere aceptar el acuerdo de uso de ASF en la cuenta de Earthdata.
- Decisión de Alex: es una zona desértica, casi sin población ni vías; como "qué celda verificar primero" tiene menos sentido que Tumbes o Piura.
