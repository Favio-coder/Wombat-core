# Arquitectura WOMBAT

WOMBAT es **El Niño Evidence Mesh**: prepara evidencia auditable para ayudar al analista del GORE Piura a priorizar qué celda de 1 km² verificar durante la preparación ante lluvias. El producto describe **cambio observado**, no una predicción ni una alerta.

## Repositorios y estado actual

El plan separa el sistema en dos repositorios:

| Repositorio | Responsabilidad | Estado en este checkout |
|---|---|---|
| `wombat-core` (este repositorio) | Contratos, Rust y el pipeline de datos. | Tiene `contracts/v1/`, WONB 0.2 en `wombat-core`, `wombat-risk` y un bundle fixture simulado. `data-pipeline/` aún no contiene los scripts del plan. |
| `wombat-dashboard` (repositorio aparte) | Aplicación web React/MapLibre; consume bundles y contratos, no importa código Rust. | Planificado; la aplicación no está en este checkout. |

El edge Raspberry/ESP32 es P2 y está planificado; no forma parte del flujo actual.

## Componentes

```text
Fuentes [PLANIFICADO]                 wombat-core
NISAR L2 GCOV ─┐                  ┌─ data-pipeline/ (Python; futuro)
OPERA DSWx-S1 ─┼─> cambio ─> celdas ─> wombat-risk (Rust; ranking determinista)
OSM roads ─────┘                  ├─ wombat-core::v02 (WONB 0.2; JCS, hash,
                                  │  firma y verificación Ed25519)
                                  ├─ contracts/v1/ (contratos + bundle simulado)
                                  ├─ wombat-node (WONB 0.1 legado; demo local)
                                  └─ wombat-cli / wombat-gateway [PLANIFICADO]
                                               │
                              bundle static/remote ─┴─ gateway live [PLANIFICADO]
                                               │
                             wombat-dashboard [PLANIFICADO, repo aparte]
```

### Contratos

WONB 0.1 y 0.2 son contratos distintos y conviven mientras dure la compatibilidad:

- **0.1 (legado):** `anomaly/context/risk/tool`, con fixtures y esquemas heredados. `wombat-node` todavía lo usa.
- **0.2 (vigente para el trabajo nuevo):** `change/context/risk/action`; está definido por `contracts/v1/` y lo usa `wombat-risk` para producir bloques del contrato nuevo.

En 0.2, los bloques se canonicalizan con JCS, se hashean y se firman con Ed25519. El ranking se calcula solo en `wombat-risk`; usa configuración explícita, aritmética exacta y produce resultados deterministas. El test dorado comprueba el ranking del bundle simulado `piura-demo/v0`.

Los valores del fixture son **SIMULADOS**. Ningún fixture actual demuestra procesamiento real de NISAR, OPERA u OSM.

## Flujo objetivo de evidencia

1. **[PLANIFICADO] Preparar datos:** el pipeline lee escenas NISAR GCOV comparables, calcula el cambio con signo, contrasta con OPERA DSWx-S1 y mide exposición de vías OSM por celda de 1 km.
2. **Implementado parcialmente en Rust:** `wombat-risk` puntúa y ordena celdas a partir de sus entradas y `risk-config.json`. Los payloads WONB 0.2 evitan floats.
3. **Implementado en el core:** WONB 0.2 permite construir, firmar y verificar bloques y cadenas de evidencia.
4. **[PLANIFICADO] Orquestar y publicar:** `wombat-cli` generará los bloques y exportará un bundle inmutable versionado; CI verificará y publicará el bundle.
5. **[PLANIFICADO] Presentar la evidencia:** el dashboard separado mostrará el mapa, el ranking y la cadena verificable. El navegador no se conectará directamente a libp2p.

## Dos repositorios y tres modos del dashboard

Los modos son parte del diseño del dashboard y **no están implementados en este checkout**:

| Modo | Fuente de datos prevista | Estado |
|---|---|---|
| `static` | Copia fijada del bundle, incluida en el build web. Debe funcionar sin backend y es el modo obligatorio del producto. | **[PLANIFICADO]** No hay dashboard que la consuma. |
| `remote` | Bundle publicado por `wombat-core` en GitHub Pages; el dashboard lo carga remotamente. | **[PLANIFICADO]** Existe workflow de publicación del fixture, pero no loader de dashboard. |
| `live` | REST/WS de `wombat-gateway` para bloques y estado recientes. | **[PLANIFICADO]** Gateway y transporte no están implementados. |

El plan propone caer a la copia estática fijada si `remote` o `live` falla. Eso también es trabajo del dashboard.

## Transporte y persistencia

`wombat-node` es una herramienta legada de demo, no una red activa. No hay transporte libp2p real: `publish` valida e imprime el JSON, y el comando `run --demo` finaliza sin conectar peers. No se debe interpretar como comunicación entre nodos.

El almacenamiento de bundles es estático y versionado por contrato; no hay una base de datos ni un gateway implementados en el core actual.
