# Adaptador geoespacial Python (futuro)

Este directorio contendrá el adaptador geoespacial en Python que leerá
fuentes locales (por ejemplo, metadatos NISAR simulados) y publicará WONBs
de tipo `anomaly` y `context`.

## Alcance MVP

- No implementado todavía.
- Futuro: leer observaciones, construir WONBs según `contracts/schemas/`,
  calcular `data_hash` y firmar con Ed25519.
- Los valores NISAR de los fixtures son **simulados para la demo**.

## Cómo correr (futuro)

```bash
cd adapters/geospatial-python
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python adapter.py
```

## Responsable sugerido

Gustavo.
