# data-pipeline (Python) — dueño: Gustavo

Scripts `01_search.py` … `06_cells.py` del plan maestro (§6). **Python no firma (R14).**
Entrada/salida y criterios de "hecho": ver §6.2 del plan. La salida debe cumplir `contracts/v1/`
(`manifest.schema.json`, `cells.schema.json`). Las salidas pesadas van en `data-pipeline/out/` (ignorado por git).

Mientras no haya datos reales, `fixtures/cells.geojson` y `fixtures/manifest.json` sirven como entrada de prueba para Rust.
