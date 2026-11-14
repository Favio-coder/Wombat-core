# Arquitectura WOMBAT

## Visión general

WOMBAT es un protocolo P2P para intercambiar bloques JSON de evidencia
(WONB) entre nodos heterogéneos. El MVP se centra en el núcleo Rust, los
contratos compartidos, las pruebas y la estructura de integración.

## Capas

```
+-------------------------------------------------------------+
|  Apps (futuro): dashboard, agente de riesgo, landing        |
+-------------------------------------------------------------+
|  Adapters (futuro): geoespacial Python, edge Raspberry Pi   |
+-------------------------------------------------------------+
|  wombat-node: libp2p + GossipSub, CLI run/publish/validate  |
+-------------------------------------------------------------+
|  wombat-core: modelo WONB, validación, SHA-256, Ed25519     |
+-------------------------------------------------------------+
|  contracts: schemas JSON y fixtures (contratos compartidos) |
+-------------------------------------------------------------+
```

## Flujo de datos

1. Un adaptador local (futuro Python) genera un WONB.
2. El WONB se firma con Ed25519 sobre su hash canónico SHA-256.
3. El nodo Rust lo valida (versión, tipo, topic, TTL, timestamp,
   duplicados) y lo publica por GossipSub en su topic.
4. Otros nodos reciben, validan y procesan el WONB.

## P2P como transporte

- libp2p con TCP, DNS, Noise, Yamux, Identify, Ping y GossipSub.
- Topics por defecto: `wombat/lima/costa-verde/{anomaly,context,risk,action}`.
- El nodo MVP arranca en modo demo local sin red P2P hasta que la infra
  esté lista.

## A2A semántica

La coordinación semántica se hace mediante WONBs (tipo anomaly, context,
risk, tool). MCP y A2A como protocolos externos no están implementados
todavía.

## Sin base de datos todavía

Todo el estado vive en memoria (deduplicación por `id`). No hay persistencia
en el MVP.

## Responsables sugeridos

| Capa                       | Responsable  |
|----------------------------|--------------|
| Core Rust / P2P            | Richard      |
| Adaptador geoespacial      | Gustavo      |
| Nodo edge Raspberry Pi     | Jelibeth     |
| Agente de riesgo / dashboard | Mireya     |
| Landing / pitch / UI       | Ximena       |
| QA, integración, CI, demo  | Jayo         |
