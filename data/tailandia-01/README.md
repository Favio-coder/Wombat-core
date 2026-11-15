# tailandia-01 — validación del método sobre un evento real

Este bundle **no es un producto para Piura**. Es la prueba de que el método
funciona de punta a punta sobre datos NISAR reales: cambio medido, contraste con
una fuente independiente, exposición vial por celda, priorización y firma.

| | |
|---|---|
| AOI | Llanura central de Tailandia, oeste de Ayutthaya |
| Par NISAR | 18 sep 2026 → 30 sep 2026, track 004, frame 081, descendente, modo 4005, frecuencia A, HHHH, 10 m |
| Madurez | **PROVISIONAL** |
| Origen | `nisar_real` |
| Contraste | OPERA DSWx-S1 (derivado de Sentinel-1), a 1 día de cada escena |
| Vías | OpenStreetMap vía Overpass, descargadas 2026-11-15 |
| Versión publicada | `v1/` — inmutable (R28). Una corrección se publica como `v2/`, nunca se sobrescribe |

## Qué significa PROVISIONAL

Es **priorización, no medición validada**. Los productos NISAR marcados
PROVISIONAL no han pasado la validación de calidad de la misión: sus valores
absolutos pueden cambiar. Lo que este bundle sostiene es el **orden relativo**
de las celdas — dónde mirar primero —, no la magnitud del cambio en decibelios.

Por eso el bloque `action` dice `field_verification_first` y no otra cosa.
Ninguna celda afirma que haya ocurrido una inundación: dicen cuánto bajó la
retrodispersión, si una fuente independiente ve agua nueva en esa misma zona, y
cuántos metros de vía hay cerca. La decisión sigue siendo de quien la toma.

## Sobre el evento

El par cubre un período de inundación reportado en Tailandia central en
septiembre de 2026. **Lo que sabemos del evento proviene de resúmenes de prensa
sin verificar**, no de una fuente oficial: no hay en este README cifras de
damnificados, hectáreas, pérdidas ni víctimas, y no se deben agregar sin una
fuente citable.

Lo que sí está verificado es lo que midió el pipeline, y está en los bloques
firmados. Nada más.

## Contenido

```
v1/
  manifest.json        procedencia: granules, madurez, CRS, OPERA, consulta OSM
  cells.geojson        600 celdas de 1 km con sus métricas crudas
  roads.geojson        5571 vías OSM, simplificadas a 5 m
  risk-config.json     los pesos y umbrales con los que se puntuó
  overlays/            t1.png, t2.png, delta.png y bounds.json
  wonb/                605 bloques firmados: change, context, 600 risk, 3 action
  index.json           sha256 de cada archivo
  trusted-issuers.json copia de la lista de confianza del repositorio
latest.json            apunta a la versión vigente
```

## Cómo comprobarlo

```bash
wombat verify data/tailandia-01/v1 --trusted config/trusted-issuers.json
python contracts/v1/tools/verify_bundle.py data/tailandia-01/v1
node contracts/v1/tools/verify-js/verify.mjs data/tailandia-01/v1
```

Las tres **no comprueban lo mismo**, y conviene saberlo: solo `wombat verify`
mide las firmas contra una lista de confianza externa al bundle. `verify_bundle.py`
lee la lista que viene dentro, y `verify.mjs` no lee ninguna: comprueba `id` y
firma contra la clave que el bloque mismo declara. Un bundle que trae su propia
lista de emisores se autoavala.

El `trusted-issuers.json` de dentro está para que un consumidor sepa con qué
clave se firmó, no para avalarse: compáralo con la lista del repositorio.

## Este README no está firmado

Vive **fuera** de `v1/` a propósito. `index.json` cubre los archivos del bundle
y nada más, así que un README dentro de la versión se podría editar sin que
ninguno de los tres verificadores se diera cuenta. Un archivo que parece
atestiguado sin estarlo es peor que no tenerlo.

Las afirmaciones de este README valen lo que vale quien lo escribió. Las de
`v1/` valen lo que vale la firma.
