# WOMBAT Gateway

Servicio HTTP de solo lectura para exponer el último Risk WONB verificado por
AOI. Ejecuta con `cargo run -p wombat-gateway`; por defecto escucha en
`127.0.0.1:8080`.

Configura `WOMBAT_GATEWAY_BIND` para cambiar la dirección y
`WOMBAT_GATEWAY_ALLOWED_ORIGIN` para establecer el único origen CORS permitido.
El origen predeterminado es `http://localhost:3000`; `*` se rechaza.

## Endpoints

- `GET /health` — estado del servicio.
- `GET /api/v1/risks/latest` — Risk más reciente global.
- `GET /api/v1/risks/{aoi_id}` — último Risk para ese AOI.

Los endpoints de Risk responden `404` con JSON descriptivo hasta que el store
reciba un bloque. La capa futura debe llamar
`RiskStore::store_verified_risk(...)` solo después de validar el bloque y su
cadena; el gateway no verifica firmas ni calcula scores.

Esta primera parte aún no se suscribe a P2P/GossipSub y no persiste los Risks
tras reiniciar el proceso.
