# Especificación WONB (Wombat Observation Notation Block)

## Formato

Un WONB es un objeto JSON con los siguientes campos:

| Campo | Tipo | Requerido | Descripción |
|-------|------|-----------|-------------|
| spec_version | string | Sí | `"wombat/0.1"` |
| id | string | Sí | `wonb:<tipo>:<uuid>` |
| type | string | Sí | `anomaly` \| `context` \| `risk` \| `tool` |
| schema | string | Sí | Esquema de dominio |
| source | objeto | Sí | `{provider, adapter, source_ref}` |
| payload | objeto | Sí | Datos del bloque |
| provenance | objeto | Sí | `{created_at, created_by, confidence, data_hash, rule_version}` |
| routing | objeto | Sí | `{topic, ttl_seconds, priority}` |
| evidence_refs | array | Sí | Referencias externas |
| signature | string | Sí | Firma Ed25519 (hex) |

## Reglas de validación

- `spec_version` debe ser `wombat/0.1`.
- `type` debe ser uno de los cuatro tipos permitidos.
- `topic` debe empezar con `wombat/`.
- `ttl_seconds` debe ser `> 0`.
- `created_at` debe ser ISO-8601 UTC parseable.
- `confidence` debe estar en `[0, 1]`.
- `data_hash` debe empezar con `sha256:`.
- El WONB se rechaza si expiró (`created_at + ttl_seconds < ahora`).
- El WONB se rechaza si su `id` ya fue visto (duplicado en memoria).

## Firma

La firma Ed25519 se calcula sobre los bytes del hash SHA-256 canónico
(sin el prefijo `sha256:`) y se codifica en hexadecimal.

## Avisos

- Los valores NISAR de los fixtures son **simulados para la demo**.
- Una anomalía de deformación superficial **no confirma ni predice por sí
  sola** un deslizamiento.
- El MVP usa una **regla demostrativa**, no un modelo científico operacional.
