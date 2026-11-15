# Contrato de integración por AOI WOMBAT

## Alcance e invariantes

Cada ejecución procesa un solo `aoi_id`. El valor debe coincidir entre
`manifest.json`, `cells.geojson`, la grilla, las tablas por celda, las vías,
el directorio de salida y cada bloque WONB. No se combinan fuentes por nombre
de archivo ni se infiere que dos artefactos son del mismo AOI solo porque
compartan fechas.

El manifest es la autoridad para procedencia y CRS de cálculo. Las geometrías
GeoJSON se expresan en `EPSG:4326`; `crs_compute` declara el UTM WGS84 usado
para áreas, distancias y agregaciones. El CRS del mapa no sustituye al CRS de
cálculo.

## Contrato común de celda

El perfil integrado mantiene los valores científicos originales en
`cells.geojson`, que es una entrada no firmada de Python a Rust. Una colección
integrada declara:

```json
{
  "type": "FeatureCollection",
  "aoi_id": "tailandia-01",
  "crs_compute": "EPSG:32647",
  "features": [
    {
      "type": "Feature",
      "geometry": {"type": "Polygon", "coordinates": []},
      "properties": {
        "aoi_id": "tailandia-01",
        "cell_id": "tailandia-01-r00c00",
        "changed_fraction": 0.1,
        "valid_fraction": 0.9,
        "opera_status": "unavailable",
        "opera_overlap_ratio": null,
        "road_m_near_change": 0,
        "road_m_total": 0,
        "source_status": {
          "nisar": "available",
          "opera": "unavailable",
          "osm": "unavailable"
        },
        "provenance_refs": {
          "nisar": ["manifest.json#/nisar"],
          "opera": ["manifest.json#/opera"],
          "osm": []
        }
      }
    }
  ]
}
```

`opera_overlap_ratio` puede ser nulo cuando OPERA no está disponible. Los
estados de fuente son `available`, `unavailable` y, para NISAR sintético,
`simulated`. Una fuente marcada disponible debe tener al menos una referencia
de procedencia. `road_m_near_change` y `road_m_total` son metros enteros.

Las fracciones decimales pueden existir en esta entrada científica no firmada.
No se copian como floats a los payloads WONB: Rust transforma fracciones a
puntos básicos (enteros 0–10000), dB a centésimas (enteros) y conserva metros,
rank y scores como enteros. La serialización y firma siguen siendo exclusivas
de Rust; el algoritmo de ranking no cambia.

Los campos nuevos son aditivos respecto al GeoJSON anterior para que se puedan
leer artefactos históricos sin reescribirlos. El perfil integrado estricto
exige identidad, CRS, estado y referencias; se valida antes de crear o aceptar
un bundle.

Desde el contrato 1.2.0, `wombat build` y `wombat verify` exigen el perfil
integrado y el `crs_compute` de la política AOI. No se delega esta aceptación
al validador Python. Los AOI registrados son Piura/Sechura/Tumbes/Jequetepeque
(`EPSG:32717`), Huaraz (`EPSG:32718`) y Tailandia (`EPSG:32647`); un AOI sin
política registrada se rechaza hasta documentar y probar su CRS.

## Clasificación y emisión

`index.json.purpose` identifica el uso:

| `purpose` | Requisito | Bloques permitidos |
|---|---|---|
| `scientific` | `manifest.nisar.comparable=true`, los seis `comparable_checks=true` y t1/t2 distintos, seleccionados y ordenados | Change, Context, Risk y Action |
| `baseline` | Puede conservar un par no comparable; debe declararse explícitamente | Change y Context; nunca Risk/Action |
| `preparation` | Paquete previo o incompleto, incluso sin par comparable | Change y Context; nunca Risk/Action |

El valor predeterminado de `wombat build` es `scientific`. Para las otras dos
clases hay que pasar `--purpose baseline` o `--purpose preparation`. Esos modos
no calculan ranking ni crean bloques Risk/Action. No se debe presentar una
preparación como resultado científico ni como alerta. El verificador Rust
rechaza Risk/Action en bundles cuyo `purpose` no sea `scientific`, verifica
que Risk dependa de Change + Context y que Action dependa de su Risk, y exige
que celdas, padres, bloques y topics pertenezcan al AOI del índice. Una clave
de `scientific` requiere los roles Change, Context, Risk y Action; `baseline`
y `preparation` requieren solo Change y Context.

### Compatibilidad histórica

Un índice con `contract_version` anterior a 1.2.0 y sin `purpose` se trata
explícitamente como `legacy`, nunca como `scientific`. Rust conserva las
comprobaciones históricas de hash, firma, emisor de confianza, TTL y existencia
de padres, sin reinterpretar esos bundles con los nuevos campos AOI/CRS ni
reescribirlos. En contrato 1.2.0 `purpose` es obligatorio y se aplican los
controles nuevos. Un índice declarado como legacy no puede llevar el marcador
`cells.geojson.aoi_id` de 1.2.0, para detectar una bajada accidental de versión
sin tocar el bundle antiguo. Los bundles publicados antiguos permanecen
inmutables.

Una salida comparable no significa por sí sola que la magnitud observada sea
una inundación o que exista una alerta validada. Se mantiene la clase de
evidencia y la madurez explícitas en el manifest y la documentación del AOI.

## AOI presentes y CRS de cálculo

CRS comprobados en los manifests locales y en las grillas de los resultados:

| AOI | CRS de cálculo correcto | Estado y uso permitido |
|---|---|---|
| `piura-demo` | `EPSG:32717` | Demo técnica totalmente simulada. Solo fixture/CI; nunca evidencia de Piura. |
| `piura-01` | `EPSG:32717` | Existe una línea base comparable 7–19 oct 2026, anterior al evento. El par de evento sigue bloqueado; no emitir alerta científica ni usar 19–31 oct (madurez mixta). |
| `sechura-01` | `EPSG:32717` | Control NISAR real 7–19 oct, no el evento; integración OPERA/OSM incompleta. |
| `tumbes-01` | `EPSG:32717` | Control NISAR real 7–19 oct, integrado hasta celdas; no es el evento. |
| `huaraz-01` | `EPSG:32718` | Cambio comparable real 5–17 oct, integrado hasta celdas; la causa del aumento de backscatter no está determinada. |
| `jequetepeque-01` | `EPSG:32717` | Par comparable real 24 oct–5 nov, integrado hasta celdas; cambios ambiguos y no tiene bundle de ranking. |
| `tailandia-01` | `EPSG:32647` | Validación de método PROVISIONAL. La versión local `v1` es inmutable y sus acciones/topic nombran Piura y el `crs_note` de celdas declara erróneamente `EPSG:32717`; no es destino operativo. Corregir insumos futuros y emitir otra versión Rust, nunca editar `v1`. |

Un manifest con CRS ausente, o un `cells.geojson` cuyo CRS declarado no
coincida con el manifest, se rechaza. No se cambia el CRS de un resultado
existente por edición manual: se revisa el productor y, si corresponde, se
genera una salida nueva con su propia procedencia.

## Aislamiento de fuentes y cadena de bloques

1. Buscar candidatos NISAR solo para el AOI y seleccionar t1/t2 que pasan los
   seis chequeos de comparabilidad.
2. Procesar/subsetear usando los archivos locales ya disponibles. La búsqueda
   o el preflight no son evidencia de que se descargó o procesó un granule.
3. Calcular cambio y agregar por la grilla del mismo AOI.
4. Asociar OPERA por granules/fechas y `cell_id`; la ausencia queda como
   `unavailable`, nunca como cero observado.
5. Asociar OSM por bbox/metadata y grilla del mismo AOI. `roads.geojson`
   declara el mismo `aoi_id` y cada geometría debe estar recortada al bbox del
   AOI; una vía ajena no se acepta porque otra vía haga intersectar el bbox
   agregado. La ausencia de `road_m_near_change` medido no demuestra que no
   existan vías.
6. Escribir celdas con identidad, CRS, estado por fuente y referencias de
   procedencia; validar con `validate_aoi_integration.py`.
7. Solo para `purpose=scientific`, Rust calcula ranking y emite Change,
   Context, Risk y Action. Para baseline/preparation emite Change/Context sin
   ranking ni acciones.
8. Rust firma y crea el índice. La carpeta de bundle/version es inmutable; una
   corrección se escribe en una versión/directorio nuevo.

Risk cuelga de Change y Context del mismo AOI. Action cuelga de su Risk, con el
mismo `cell_id`; la cadena completa debe resolver al Change/Context del AOI
solicitado. `aoi_id` se deriva también del prefijo de cada `cell_id`. Topic,
audience, rutas de archivos e índice no pueden reintroducir otro AOI.

## Comprobación local

El validador es de solo lectura y usa la biblioteca estándar de Python:

```bash
python data-pipeline/validacion/validate_aoi_integration.py \
  --in data-pipeline/validacion/resultados/peru-fuentes-nasa/ciudades/piura-01/previo-07oct-19oct \
  --purpose baseline \
  --inputs data-pipeline/inputs/piura-01 \
  --roads data-pipeline/inputs/piura-01/roads.geojson
```

Para un paquete no comparable debe indicarse explícitamente `--purpose
baseline` o `--purpose preparation`; el validador falla si encuentra Risk o
Action. El modo científico falla si `comparable` o cualquiera de los seis
chequeos no es verdadero. También compara identidades, IDs de celda, bbox,
CRS, estado/procedencia de fuentes, topics, audience, padres, índice y valores
float dentro de payloads firmados. La autoridad de `wombat verify` es Rust;
este script es una validación local adicional, no un sustituto.

El resultado histórico de Piura del ejemplo antecede al perfil integrado: hoy
debe rechazarse porque sus celdas no declaran los campos AOI/CRS/estado/
procedencia requeridos. El comando no transforma ni reescribe ese artefacto;
solo sirve para comprobar que no se publique como baseline hasta reprocesarlo
con el productor actualizado.

La compatibilidad con los formatos previos no reescribe los datos persistidos.
Si el validador encuentra metadatos de integración ausentes o contradictorios,
la salida existente no se publica como 1.2.0 hasta que un reprocesamiento
autorizado produzca una versión nueva.

## Verificación CI de Python

El workflow `.github/workflows/ci-pipeline.yml` configura Python 3.12 e instala
las dependencias declaradas antes de ejecutar las pruebas:

```bash
python -m pip install -r data-pipeline/validacion/requirements.txt
python -m pytest data-pipeline/validacion -q
```

No ejecutar `pip install` localmente como parte de esta validación si las
dependencias no están disponibles; reportar el bloqueo de entorno. La lista de
requirements es la fuente de dependencias de CI y actualmente no fija versiones
exactas, por lo que un lockfile de Python queda como mejora de reproducibilidad.
