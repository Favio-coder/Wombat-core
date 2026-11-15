# WOMBAT Gateway HTTP

API HTTP de solo lectura para React/Vercel. El gateway consume únicamente las
líneas JSONL de eventos que `wombat-node` ya verificó; no valida firmas,
recalcula ranking ni transforma `Change` en `Risk`. Conserva `simulation: true`
y la procedencia en la respuesta.

## Configuración

```sh
wombat-gateway \
  --bind 127.0.0.1:3000 \
  --event-in /var/lib/wombat/events.jsonl
```

`--bind` también admite `WOMBAT_GATEWAY_BIND` y su default es
`127.0.0.1:3000`. `--event-in` también admite `WOMBAT_GATEWAY_EVENT_IN`. Si el
archivo todavía no existe, el gateway espera a que aparezca; sigue append,
truncado y rotación. Líneas inválidas, incompletas y eventos con `verified`
distinto de `true` no se exponen.

`WOMBAT_GATEWAY_ALLOWED_ORIGIN` configura CORS. El default es
`http://localhost:3000`; `*` habilita cualquier origen únicamente cuando se
configura explícitamente.

## Endpoints

- `GET /health`
- `GET /api/v1/events/latest`
- `GET /api/v1/events/{aoi_id}`
- `GET /api/v1/risks/latest` y `GET /api/v1/risks/{aoi_id}` (compatibilidad).

Si no hay evento verificado, los endpoints de eventos responden HTTP 404 con
`{"error":"no_verified_events", ...}`. Una respuesta de evento incluye
`wonb_id`, `aoi_id`, `type`, `topic`, `issued_at`, `simulation`, `data_origin`,
`verified`, `payload` y `provenance`.

## Servicio systemd

El ejemplo [wombat-gateway.service.example](release/wombat-gateway.service.example)
usa exactamente este comando:

```ini
ExecStart=/opt/wombat/wombat-gateway \
  --bind 127.0.0.1:3000 \
  --event-in /var/lib/wombat/events.jsonl
```

## Instalación en EC2 (Linux x86_64)

Descarga el archivo desde el Release elegido, verifica sus checksums e instala
el binario y la unidad:

```sh
release_tag="vX.Y.Z"
package="wombat-gateway-linux-x86_64.tar.gz"
curl -fL "https://github.com/Favio-coder/Wombat-core/releases/download/${release_tag}/${package}" -o "/tmp/${package}"
tar -xzf "/tmp/${package}" -C /tmp
cd /tmp/wombat-gateway-linux-x86_64
sha256sum -c SHA256SUMS
sudo install -d -m 0755 /opt/wombat
sudo install -m 0755 wombat-gateway /opt/wombat/wombat-gateway
sudo install -m 0644 wombat-gateway.service.example /etc/systemd/system/wombat-gateway.service
sudo systemctl daemon-reload
sudo systemctl enable --now wombat-gateway
```

El usuario configurado en la unidad debe poder leer
`/var/lib/wombat/events.jsonl`. Verifica el servicio en EC2:

```sh
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3000/api/v1/events/latest
```

Para acceso desde React/Vercel, conserva el bind local y coloca un proxy HTTPS
delante; establece `WOMBAT_GATEWAY_ALLOWED_ORIGIN` al origen exacto del
frontend. No expongas el puerto de aplicación directamente a Internet.
