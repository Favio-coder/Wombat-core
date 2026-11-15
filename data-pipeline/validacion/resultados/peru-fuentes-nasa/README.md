# Perú con otras fuentes de la NASA (sin NISAR)

**Fecha:** 14 nov 2026 · Exploración. Zonas: llanura del río Zarumilla (Tumbes) y Bajo Piura. Periodo: 10 oct – 13 nov 2026.

## En una frase

Las fuentes de la NASA confirman **cuándo llovió**, pero el mapa de agua OPERA **no muestra una inundación extensa** en Tumbes ni en Piura, ni siquiera un día después de las lluvias. Coincide con lo que dio NISAR: los eventos de Perú hasta ahora fueron localizados y de corta duración. **La excepción es el desierto de Sechura**, donde tras la lluvia del 1–2 de noviembre aparece un cambio grande y persistente, aún sin confirmar como agua abierta.

## Qué hay disponible

| Fuente NASA | Qué mide | Fechas útiles | Estado |
|---|---|---|---|
| OPERA DSWx-S1 | Agua superficial, 30 m (radar Sentinel-1) | Piura: 21 oct. Tumbes: 30 oct, 2 nov, 5 nov | **Procesado** |
| NASA POWER, `PRECTOTCORR` | Lluvia diaria, grilla de ~50 km (MERRA-2 corregido) | Todos los días hasta el 11 nov | **Consultado** |
| GPM IMERG diario | Lluvia, 10 km | Todos los días | Acceso denegado (403): la cuenta de Earthdata debe autorizar la aplicación "NASA GESDISC DATA ARCHIVE" |
| OPERA RTC-S1 | Radar Sentinel-1 corregido, 30 m | Piura: 9 oct → 21 oct, mismo satélite y órbita | Bloqueado: la cuenta debe aceptar el acuerdo de uso de ASF (EULA) |
| HLS (Landsat y Sentinel-2) | Imagen óptica, 30 m | 13 escenas sobre Piura | Descartado: 71 % a 100 % de nubes en todas |
| SMAP `SPL3SMP_E` | Humedad de suelo, 9 km | Cada 2 a 3 días | **Procesado** (2 oct – 11 nov, 50 archivos) |
| MODIS inundación NRT | Inundación diaria, 250 m | En el catálogo solo desde el 8 nov | Sin procesar |
| SWOT | Altura y extensión de agua | Ninguna encontrada | — |

## Lluvia (NASA POWER, mm por día)

Es un promedio sobre una celda de ~50 km, no un pluviómetro: los picos son menores que los de las estaciones (Senamhi reportó 104 mm en El Salto).

| Fechas | Zarumilla (Tumbes) | Piura |
|---|---|---|
| 11–12 oct | 35,9 y 28,8 | 2,7 y 1,3 |
| 19–20 oct | 24,3 y 31,4 | **21,1 y 30,0** |
| 25 oct | **60,9** | 10,4 |
| 26–31 oct | 20 a 31 cada día | 5 a 10 cada día |
| 1–2 nov | **45,3 y 58,9** | 17,6 y **43,7** |
| 4–5 nov | 23,2 y 24,2 | 4,5 y 15,6 |
| 8–11 nov | 3 a 7 | 1 a 3 |

Confirma las fechas tomadas de prensa: 20 oct en Piura; 25–27 oct y 1–2 nov en Tumbes. Muestra además que en Tumbes llovió casi todos los días desde el 11 de octubre.

## Agua superficial (OPERA DSWx-S1)

Agua = clases 1 (agua abierta) y 3 (vegetación inundada), sobre el área válida.

**Bajo Piura:** 0,42 % a 0,66 % en las diez pasadas. El 21 de octubre, un día después de las lluvias: 0,46 %. Sin aumento.

**Zarumilla:** 3,5 % a 4,0 % en todas las pasadas con cobertura completa (el fondo son pozas langostineras y manglar). El 2 de noviembre: 3,55 %.

Agua nueva entre pasadas de la misma geometría, en Zarumilla:

| Par | Lluvia encerrada | Agua nueva | Agua que desapareció |
|---|---|---|---|
| 12 oct → 24 oct | lluvias del 19–20 oct | 342 ha | 352 ha |
| 18 oct → 30 oct | 25–27 oct | 509 ha | 502 ha |
| 21 oct → 2 nov | 25 oct – 2 nov | 548 ha | 561 ha |
| 24 oct → 5 nov | 25 oct – 2 nov | 399 ha | 399 ha |

El agua nueva es casi igual a la que desaparece: es movimiento de pozas y manglar, no un aumento neto. Las celdas con más agua nueva están en el borde norte, en la zona de pozas.

## Barrido regional: dónde sí apareció agua

OPERA DSWx-S1 sobre toda la costa norte (`-81.35 -6.60 -79.60 -3.35`), comparando 5–13 oct con 4–12 nov. Imagen: `mapa_agua_nueva_costa_norte.png` (rojo = agua nueva, naranja = agua que desapareció, azul = agua en ambas épocas; 240 m por píxel).

- En casi toda la región no hay cambio. El borde norte (manglar y pozas de Tumbes y El Oro) muestra agua que aparece y desaparece por igual.
- **El único aumento neto grande está en el desierto de Sechura**, entre `-81.00 -6.30` y `-80.70 -6.00`, unos 100 km al sur de la ciudad de Piura: bloques de 3 km con 400 a 745 ha de agua nueva y casi nada que desaparezca. Es la zona donde en años de El Niño se forma la laguna La Niña; no se verificó que sea ese cuerpo de agua.

Serie sobre ese recuadro (incluye mar: ~22 % de agua de fondo), por geometría de pasada:

| Pasadas de la tarde (23:42 UTC) | Agua | Pasadas de la mañana (11:01 UTC) | Agua |
|---|---|---|---|
| 9 oct | 22,0 % | 12 oct | 21,9 % |
| 21 oct | 22,3 % | 18 oct | 21,6 % |
| **2 nov** | **32,0 %** | 24 oct | 22,2 % |
| **8 nov** | **31,9 %** | 30 oct | 23,3 % |
| | | 5 nov | 23,5 % |
| | | 11 nov | 22,9 % |

| Par de la misma geometría | Agua nueva | Desapareció | Celdas de 1 km con > 25 % |
|---|---|---|---|
| Tarde, 9 oct → 21 oct | 978 ha | 626 ha | 8 |
| **Tarde, 21 oct → 2 nov** | **11 061 ha** | 421 ha | **162** |
| Tarde, 2 nov → 8 nov | 1 704 ha | 1 832 ha | 6 |
| Mañana, 24 oct → 5 nov | 2 091 ha | 627 ha | 30 |
| Mañana, 30 oct → 11 nov | 538 ha | 911 ha | 4 |

Lectura prudente:

- **Algo cambió en esa superficie tras la lluvia del 1–2 de noviembre** (44 mm en Piura el 2 nov) y se mantuvo al menos hasta el 8.
- **Las dos geometrías no ven lo mismo.** La de la tarde ve ~11 000 ha de agua nueva; la de la mañana, ~2 000 ha. Agua abierta profunda debería verse igual desde ambas. Una explicación posible es una lámina muy somera o una planicie salina saturada, que el radar confunde con agua según el ángulo. No está resuelto.
- **Como mínimo hay ~1 500 a 2 000 ha** que ambas geometrías marcan como agua nueva.
- No hay imagen óptica que lo aclare: la última HLS disponible es del 4 nov, con 82 % de nubes. La del 5 de octubre (15 % de nubes) sirve de "antes".

NISAR sobre esa zona: pista 112, frame 092, modo 4005, cobertura 100 %. Como el cambio ocurre después del 1 de noviembre, el par que lo encierra es **31 oct → 12 nov (VALIDATED)**; la escena del 12 nov aún no está publicada. Los pares PROVISIONAL disponibles (hasta el 19 oct) son anteriores.

## Otras regiones: Lambayeque, Áncash, Lima e Ica

Lluvia NASA POWER del 1 oct al 11 nov (`lluvia-nasa-power-regiones.json`), reportes de prensa y pares NISAR de igual madurez que encierren el 2 de noviembre:

| Región y punto | Lluvia oct | Lluvia 1–11 nov | Días ≥ 10 mm | Reportes encontrados | Par NISAR |
|---|---|---|---|---|---|
| Tumbes (Zarumilla) | 443 mm (solo 10–31 oct) | 218 mm | 24 | Inundaciones 26–27 oct y 1–2 nov | Pendiente (escena del 5 o 12 nov) |
| Piura (ciudad) | 139 mm (solo 10–31 oct) | 108 mm | 7 | Inundaciones 20 oct; alerta roja en nov | Pendiente (12 nov) |
| Lambayeque (Mórrope) | 47 mm | 47 mm | 1 | Ninguno en nov | El par 24 oct → 5 nov cubre solo 1–2 % de la zona |
| Lambayeque (Olmos) | 103 mm | 76 mm | 2 | Ninguno en nov | — |
| Áncash (Chimbote) | 15 mm | 7 mm | 0 | Ninguno en la costa | 24 oct → 5 nov, VALIDATED, cobertura 100 % |
| Áncash (Huaraz, sierra) | — | — | — | 6 nov: calles inundadas y un huaico | Ninguno |
| Lima (Chosica) | 10 mm | 13 mm | 0 | Aniegos por lloviznas | Ninguno |
| Ica (ciudad) | 0,6 mm | 2 mm | 0 | Solo pronósticos para la sierra | Ninguno |

Lectura:

- **Hasta el 11 de noviembre, la lluvia fuerte está en Tumbes y Piura.** En la costa de Áncash, Lima e Ica prácticamente no ha llovido; en Lambayeque, poco.
- **El estado de emergencia del 3 de noviembre (D.S. 149-2026-PCM) es preventivo**: declara 1 380 distritos de 25 regiones por riesgo ante El Niño 2026-2027, no por daños ya ocurridos. Que una región esté en la lista no significa que ya tenga un evento que mapear.
- Lo reportado en Áncash es en Huaraz: zona urbana de sierra, donde el radar falla por el relieve.
- Lima, Ica y Huaraz están en la zona UTM 18S (`EPSG:32718`), válida desde el contrato v1.1.0.
- La temporada de lluvias de El Niño suele ir de diciembre a marzo: es probable que estas regiones tengan eventos más adelante, no ahora.

El barrido OPERA de la costa norte llega hasta la latitud −6,6 y cubre la mayor parte del llano de Lambayeque: ahí no hay aumento neto de agua. No se barrió Áncash, Lima ni Ica con OPERA.

Con NISAR sí se leyeron todas: ver `ciudades/README.md`. Lambayeque, Chimbote, Lima e Ica son estables. **Huaraz muestra un cambio grande el 17 de octubre** (58,7 % del área subió ≥ 3 dB, par de igual madurez; 0,0 % en el par previo sin evento), compatible con terreno mojado y no con agua abierta: OPERA no ve agua nueva ahí.

## Humedad de suelo (SMAP `SPL3SMP_E`, 9 km) frente a NISAR

Humedad en m³/m³, media de las celdas de cada zona. SMAP pasa cada 2 a 3 días; no hay dato del 17 al 20 de octubre. El brillo NISAR es el γ⁰ HHHH medio del recuadro, sin agua oscura.

**Bajo Piura** (20 celdas SMAP, casi todas de calidad recomendada):

| Fecha | NISAR (06:09 local) | SMAP | Lluvia POWER (mm) |
|---|---|---|---|
| 2–16 oct | 7 oct: −7,26 dB | 0,09 a 0,12, estable | 0,4 a 3 por día |
| 19 oct | **−6,76 dB** (+0,50) | sin pasada | **21,1** |
| 20 oct | — | sin pasada | **30,0** |
| 21 oct | — | **0,164** (mañana), 0,142 (tarde) | 9,6 |
| 23–24 oct | — | 0,119 y 0,113 | 0,7 y 2,6 |
| 26–29 oct | — | 0,116 a 0,143 | 7 a 10 por día |
| 31 oct – 1 nov | 31 oct: **−7,54 dB** (−0,78) | 0,106 y 0,105 | 4,9 y 17,6 |
| 3 nov | — | **0,156** (mañana) | 2 nov: **43,7** |
| 6 nov | — | **0,174** (mañana) | 5 nov: 15,6 |
| 8–11 nov | — | 0,106 a 0,113 | 1 a 3 por día |

Las tres fuentes coinciden: suelo seco y estable hasta el 16 de octubre, lluvia el 19–20, suelo mojado el 21, seco otra vez el 31. NISAR está alto el 19 y bajo el 7 y el 31, que son los dos días en que SMAP marca suelo seco. El suelo se seca en dos o tres días (0,164 → 0,119 del 21 al 23).

**Tumbes–Zarumilla** (15 celdas; muchas sin calidad recomendada, por la costa):

| Fecha | NISAR (18:39 local) | SMAP, pasada de la tarde de la misma geometría |
|---|---|---|
| 7 oct | −6,80 dB | 0,184 |
| 15 oct | — | 0,197 |
| 19 oct | −5,90 dB (+0,90) | sin pasada |
| 23 oct | — | 0,239 |
| 31 oct | −5,70 dB (+0,20) | 0,261 |

SMAP sube de forma sostenida y NISAR también. Las demás pasadas dan valores más altos y dispersos (0,25 a 0,41), con tendencia creciente de inicios de octubre a noviembre.

Lectura: el brillo de NISAR en banda L sigue a la humedad del suelo en ambas zonas, con cambios de 0,5 a 1 dB. Eso está por debajo del umbral de ±3 dB del plan, que busca agua abierta. No es una calibración formal: son tres fechas NISAR por zona, y el 19 de octubre no tiene dato SMAP del mismo día.

## Qué significa

- **No hay inundación extensa que mapear en Perú** con productos de agua de 30 m, para estos eventos. Los daños reportados (asentamientos inundados, quebradas activadas) son urbanos, pequeños frente a un recuadro de 16 × 28 km, y drenan en horas.
- **Los resultados planos de NISAR en Tumbes y Piura quedan respaldados** por una fuente independiente: no era falta de sensibilidad.
- **La subida general que vio NISAR entre el 7 y el 19 de octubre coincide con lluvia y con suelo húmedo según SMAP.** Lo que el satélite sí registró en Perú es el suelo mojado, no agua en superficie.
- **Hay una señal real de NISAR en Perú, pero más débil que la que busca el plan.** Un indicador de humedad (cambio de 0,5 a 1 dB sobre áreas amplias) sería otro producto, con otro umbral; no sirve para ordenar celdas de 1 km por inundación.

## Límites

- NASA POWER es un producto de modelo, de baja resolución. GPM IMERG sería mejor (10 km), pero requiere la autorización indicada arriba.
- OPERA DSWx-S1 es radar: detecta mal el agua en zonas urbanas y bajo vegetación densa.
- El reto y el plan piden NISAR como dato principal (R1). Estas fuentes sirven de contexto y contraste; reemplazar NISAR es decisión de Alex.

## Contenido de esta carpeta

| Archivo | Qué es |
|---|---|
| `mapa_agua_nueva_costa_norte.png` | Barrido OPERA DSWx-S1 de la costa norte, 5–13 oct frente a 4–12 nov |
| `opera/barrido-costa-norte.json` | Hectáreas de agua nueva y bloques de 3 km con mayor aumento |
| `opera/serie-*.json` | Agua OPERA por pasada sobre Tumbes–Zarumilla, Bajo Piura y Sechura |
| `lluvia-nasa-power.json` | Lluvia diaria NASA POWER en Zarumilla y Piura (10 oct – 11 nov) |
| `lluvia-nasa-power-regiones.json` | Lo mismo para Lambayeque, Áncash, Lima e Ica (1 oct – 11 nov) |
| `humedad-smap.json` | Humedad de suelo SMAP por fecha, pasada y celda |
| `nisar-gamma0-medio.json` | γ⁰ HHHH medio de NISAR por fecha en Piura y Tumbes |
| `sechura-01/` | Sechura preparado para NISAR: plan, grilla, par previo y control |
| `ciudades/` | **NISAR por ciudad**: serie de fechas en Áncash, Lambayeque, Lima, Ica, Tumbes y Piura, y carpetas de Huaraz, Tumbes, Piura y Jequetepeque con el mismo contenido que el par completo de Tailandia (cambio NISAR, OPERA y vías por celda; cumplen el contrato v1.1.0) |

Es material de exploración, no un bundle. No se incluyen los rásteres ni los mapas OPERA descargados: se regeneran. Las salidas de trabajo están en `data-pipeline/out/validacion/`, que no entra al repositorio.

## Fuentes

- OPERA DSWx-S1 (PO.DAAC), colección `OPERA_L3_DSWX-S1_V1`: https://www.jpl.nasa.gov/go/opera/products
- NASA POWER, API diaria: https://power.larc.nasa.gov/
- GPM IMERG diario (GES DISC), colección `GPM_3IMERGDL`: https://gpm.nasa.gov/data/imerg
- SMAP L3 Enhanced, colección `SPL3SMP_E` versión 006 (NSIDC): https://nsidc.org/data/spl3smp_e
- HLS, colecciones `HLSS30` y `HLSL30` (LP DAAC): https://www.earthdata.nasa.gov/data/projects/hls
