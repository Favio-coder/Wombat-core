#!/usr/bin/env python3
"""BOOTSTRAP: genera el bundle fixture firmado `piura-demo/v0` (datos SIMULADOS).

NO es el pipeline de producción. R14 dice que solo Rust firma: este script existe
únicamente para destrabar al dashboard y al CI hasta que `wombat-cli export-static`
exista. Cuando exista, el bundle se regenera con Rust y este script se archiva.

Qué hace (referencia de lo que Rust debe reproducir):
  fixtures/cells.geojson + fixtures/manifest.json + config/risk-config.json
    -> score/ranking (8.3) -> bloques WONB change/context/risk/action firmados
    -> overlays PNG simulados -> index.json con sha256

La clave privada Ed25519 se crea en memoria y se DESCARTA (R19): solo se escribe la pública.
Uso:  python3 contracts/v1/tools/gen_fixture_bundle.py
"""
import hashlib, json, math, os, sys
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "contracts/v1/fixtures/bundle/piura-demo/v0"
ISSUED = "2026-11-14T17:00:00Z"
RISK_VALID_UNTIL = "2026-11-15T17:00:00Z"
ACTION_VALID_UNTIL = "2026-11-14T18:00:00Z"
CONTRACT_VERSION = (ROOT / "contracts/v1/VERSION").read_text().strip()

def jcs(o):
    """RFC 8785 para nuestro subconjunto: sin floats (R16), claves ASCII."""
    def chk(x):
        if isinstance(x, float): raise ValueError("float en payload firmado (R16)")
        if isinstance(x, dict): [chk(v) for v in x.values()]
        if isinstance(x, list): [chk(v) for v in x]
    chk(o)
    return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")

def bp(x):  # fracción -> puntos básicos, redondeo half-up
    return int(math.floor(x * 10000 + 0.5))

def sha_file(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()

def write_json(p, o):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(o, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

# ---------- 1. cuadrícula simulada 4x4 (si no existe fixtures/cells.geojson) ----------
AOI = "piura-demo"
LON0, LAT0, STEP = -80.720, -5.200, 0.009045   # esquina NO; ~1 km por celda (solo fixture)
# (valid, dec, inc, opera_status, overlap, road_near, road_total)
TBL = [
 [(.95,.02,.01,"nc",.05,300,2100),(.93,.05,.02,"nc",.08,800,3000),(.94,.03,.03,"nc",.04,0,1500),(.90,.01,.01,"nc",.00,0,900)],
 [(.92,.10,.03,"co",.24,1500,4200),(.91,.18,.04,"co",.31,2400,5100),(.90,.12,.06,"co",.22,1900,4700),(.55,.30,.02,"nc",.10,200,800)],
 [(.94,.08,.02,"nc",.12,700,2600),(.89,.21,.05,"co",.38,2800,5600),(.92,.15,.04,"co",.27,3100,6000),(.93,.06,.02,"un",None,1200,3500)],
 [(.96,.01,.00,"nc",.02,100,1800),(.95,.04,.01,"nc",.09,500,2400),(.91,.07,.07,"co",.20,900,3300),(.94,.02,.01,"nc",.03,0,1000)],
]
ST = {"co":"corroborated","nc":"not_corroborated","un":"unavailable"}
cells_path = ROOT / "fixtures/cells.geojson"
feats = []
for r, row in enumerate(TBL):
    for c, (v, d, i, st, ov, rn, rt) in enumerate(row):
        x0, y0 = LON0 + c * STEP, LAT0 - r * STEP
        ring = [[round(x0,6),round(y0,6)],[round(x0+STEP,6),round(y0,6)],[round(x0+STEP,6),round(y0-STEP,6)],[round(x0,6),round(y0-STEP,6)],[round(x0,6),round(y0,6)]]
        ch = round(d + i, 2)
        feats.append({"type":"Feature","geometry":{"type":"Polygon","coordinates":[ring]},"properties":{
            "cell_id": f"{AOI}-r{r:02d}c{c:02d}","valid_fraction": v,"decrease_fraction": d,"increase_fraction": i,
            "changed_fraction": ch,"p90_abs_delta_db": round(3.0 + ch * 10, 1),"opera_status": ST[st],
            "opera_overlap_ratio": ov,"road_m_near_change": rn,"road_m_total": rt}})
write_json(cells_path, {"type":"FeatureCollection","crs_note":"SIMULADO. geometry en EPSG:4326; en producción los cálculos se hacen en EPSG:32717","features":feats})
cells = json.loads(cells_path.read_text())["features"]
BBOX = [round(LON0,6), round(LAT0-4*STEP,6), round(LON0+4*STEP,6), round(LAT0,6)]

manifest = {
  "aoi_id": AOI, "aoi_name": "SIMULATED — fixture de desarrollo (no es un AOI real)", "data_origin": "simulated",
  "crs_compute": "EPSG:32717", "bbox_wgs84": BBOX,
  "nisar": {"product":"NISAR_L2_GCOV","maturity":"PROVISIONAL","frame":"SIMULATED","orbit_direction":"ascending","mode":"SIMULATED",
    "frequency":"A","polarization":"HHHH","pixel_spacing_m":20,
    "t1":{"granule":"SIMULATED-T1","datetime":"2026-10-14T23:00:00Z"},"t2":{"granule":"SIMULATED-T2","datetime":"2026-10-26T23:00:00Z"},
    "comparable": True,
    "comparable_checks":{k:True for k in ["same_frame","same_orbit_direction","same_mode","same_frequency","same_polarization","same_maturity"]}},
  "opera": {"product":"OPERA_L3_DSWx-S1","derived_from":"Sentinel-1","status":"available",
    "t1":{"granules":["SIMULATED-OPERA-T1"],"gap_days":2},"t2":{"granules":["SIMULATED-OPERA-T2"],"gap_days":4}},
  "osm": {"query":"highway=*","retrieved_at":"2026-11-14T15:00:00Z","attribution":"© OpenStreetMap contributors"},
  "pipeline": {"commit":"fixture","risk_config_version":"risk-config/0.1"}}
write_json(ROOT / "fixtures/manifest.json", manifest)

cfg = json.loads((ROOT / "config/risk-config.json").read_text())

# ---------- 2. score y ranking de referencia (spec 8.3) ----------
props = [f["properties"] for f in cells]
ok = [p for p in props if p["valid_fraction"] >= cfg["quality_min_valid_fraction"]]
max_road = max((p["road_m_near_change"] for p in ok), default=0)
w = cfg["weights"]
res = {}
for p in props:
    cid = p["cell_id"]
    if p["valid_fraction"] < cfg["quality_min_valid_fraction"]:
        res[cid] = {"insufficient": True}; continue
    ch = min(p["changed_fraction"] / cfg["change_saturation"], 1.0)
    op = p["opera_overlap_ratio"] if p["opera_status"] != "unavailable" else 0.0
    rd = (p["road_m_near_change"] / max_road) if max_road else 0.0
    sc = w["change"]*ch + w["opera"]*op + w["road"]*rd
    res[cid] = {"change_bp": bp(ch), "opera_bp": bp(op), "road_bp": bp(rd), "score_bp": bp(sc)}
ranked = sorted([c for c in res if "insufficient" not in res[c]], key=lambda c: (-res[c]["score_bp"], -res[c]["change_bp"], c))
hi, med = bp(cfg["status_thresholds"]["review_high"]), bp(cfg["status_thresholds"]["review_medium"])
for n, cid in enumerate(ranked, 1):
    res[cid]["rank"] = n
    s = res[cid]["score_bp"]
    res[cid]["status"] = "review_high" if s >= hi else "review_medium" if s >= med else "review_low"

def pct(b): return (b + 50) // 100
def explain(p):
    out = [f"{pct(bp(p['decrease_fraction']))}% of valid pixels decreased ≥{cfg['delta_db_threshold']:g} dB; {pct(bp(p['increase_fraction']))}% increased ≥{cfg['delta_db_threshold']:g} dB."]
    if p["opera_status"] == "unavailable":
        out.append(f"No OPERA DSWx-S1 (Sentinel-1) observation within {cfg['opera_max_gap_days']} days; no independent contrast available.")
    else:
        thr = pct(bp(cfg["corroboration_overlap_threshold"]))
        if p["opera_status"] == "corroborated":
            out.append(f"OPERA DSWx-S1 (Sentinel-1) shows new water on {pct(bp(p['opera_overlap_ratio']))}% of changed area (threshold {thr}%).")
        else:
            out.append(f"OPERA DSWx-S1 (Sentinel-1) shows new water on only {pct(bp(p['opera_overlap_ratio']))}% of changed area (below the {thr}% threshold).")
    out.append(f"{p['road_m_near_change']/1000:.1f} km of road lie within {cfg['road_buffer_meters']} m of changed pixels.")
    return out

# ---------- 3. roads simulados + overlays simulados ----------
W, H = 400, 400
def lonlat_to_px(lon, lat):
    return ((lon - BBOX[0]) / (BBOX[2]-BBOX[0]) * W, (BBOX[3] - lat) / (BBOX[3]-BBOX[1]) * H)
roads_ll = [
 [[BBOX[0],LAT0-1.5*STEP],[BBOX[0]+2*STEP,LAT0-1.6*STEP],[BBOX[2],LAT0-2.4*STEP]],
 [[LON0+1.5*STEP,BBOX[3]],[LON0+1.6*STEP,LAT0-2*STEP],[LON0+2.2*STEP,BBOX[1]]],
 [[BBOX[0],LAT0-3.4*STEP],[BBOX[2],LAT0-3.2*STEP]],
 [[LON0+3.3*STEP,BBOX[3]],[LON0+3.4*STEP,BBOX[1]]]]
def seglen_m(a, b):
    dx = (b[0]-a[0]) * 110400; dy = (b[1]-a[1]) * 110570
    return math.hypot(dx, dy)
roads_total = int(sum(seglen_m(l[i], l[i+1]) for l in roads_ll for i in range(len(l)-1)))
roads = {"type":"FeatureCollection","features":[{"type":"Feature","properties":{"highway":"track","simulated":True},"geometry":{"type":"LineString","coordinates":[[round(x,6),round(y,6)] for x,y in l]}} for l in roads_ll]}

rng = np.random.default_rng(42)
yy, xx = np.mgrid[0:H, 0:W]
base = 0.5 + 0.12*np.sin(xx/37.0) + 0.10*np.cos(yy/29.0)
speck1 = rng.gamma(8, 1/8, (H, W)); speck2 = rng.gamma(8, 1/8, (H, W))
hot = np.zeros((H, W))
for r, row in enumerate(TBL):
    for c, (v, d, i, *_) in enumerate(row):
        hot[r*100:(r+1)*100, c*100:(c+1)*100] = d - i
g1 = base * speck1
g2 = base * speck2 * np.exp(-6.0 * hot)       # bajada de retrodispersión donde hay "decrease"
db1, db2 = 10*np.log10(g1), 10*np.log10(g2)
lo, hi_ = np.percentile(np.concatenate([db1.ravel(), db2.ravel()]), [2, 98])    # R9: MISMO estiramiento
def gray(db):
    a = np.clip((db - lo) / (hi_ - lo), 0, 1)
    return (a * 255).astype(np.uint8)
def stamp(img):
    d = ImageDraw.Draw(img); d.rectangle([0, 0, W, 16], fill=(0,0,0,170) if img.mode == "RGBA" else 0)
    d.text((4, 2), "SIMULATED DATA - NOT NISAR", fill=(255,255,255,255) if img.mode == "RGBA" else 255)
    return img
OV = OUT / "overlays"; OV.mkdir(parents=True, exist_ok=True)
stamp(Image.fromarray(gray(db1), "L").convert("RGBA")).save(OV/"t1.png")
stamp(Image.fromarray(gray(db2), "L").convert("RGBA")).save(OV/"t2.png")
delta = np.clip((db2 - db1) / 6.0, -1, 1)           # ±6 dB fijo
rgba = np.zeros((H, W, 4), np.uint8)
neg, pos = np.clip(-delta, 0, 1), np.clip(delta, 0, 1)
rgba[..., 2] = (255 * neg).astype(np.uint8); rgba[..., 0] = (255 * pos).astype(np.uint8)
rgba[..., 3] = (np.maximum(neg, pos) * 200).astype(np.uint8)
rgba[np.abs(delta) < 0.5] = 0                         # |delta|<3 dB -> transparente
stamp(Image.fromarray(rgba, "RGBA")).save(OV/"delta.png")
write_json(OV/"bounds.json", {"crs":"EPSG:4326","coordinates":[[BBOX[0],BBOX[3]],[BBOX[2],BBOX[3]],[BBOX[2],BBOX[1]],[BBOX[0],BBOX[1]]],"order":"top-left, top-right, bottom-right, bottom-left"})

# ---------- 4. firma ----------
sk = Ed25519PrivateKey.generate()
pk_hex = sk.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()
ISSUER = f"ed25519:{pk_hex}"
def seal(btype, topic_leaf, parents, payload, valid_until, sources):
    body = {"wonb_version":"0.2","type":btype,"topic":f"wombat/pe/piura/{AOI}/{topic_leaf}","issued_at":ISSUED,"valid_until":valid_until,
            "issuer":ISSUER,"parents":parents,"payload":payload,
            "provenance":{"sources":sources,"method":"risk-config/0.1","code_commit":"fixture"}}
    digest = hashlib.sha256(jcs(body)).digest()
    return {"wonb_version":"0.2","id":"sha256:"+digest.hex(),"type":body["type"],"topic":body["topic"],"issued_at":ISSUED,"valid_until":valid_until,
            "issuer":ISSUER,"parents":parents,"payload":payload,"provenance":body["provenance"],"signature":sk.sign(digest).hex()}

write_json(OUT/"cells.geojson", json.loads(cells_path.read_text()))
write_json(OUT/"roads.geojson", roads)
write_json(OUT/"manifest.json", manifest)
write_json(OUT/"risk-config.json", cfg)
src_nisar = {"product":"NISAR_L2_GCOV","granule":"SIMULATED-T1/T2","maturity":"PROVISIONAL"}
src_opera = {"product":"OPERA_L3_DSWx-S1","derived_from":"Sentinel-1","granule":"SIMULATED-OPERA"}
src_osm = {"product":"OpenStreetMap","retrieved_at":"2026-11-14"}
n = manifest["nisar"]
aoi_dec = bp(sum(p["decrease_fraction"] for p in props) / len(props)); aoi_inc = bp(sum(p["increase_fraction"] for p in props) / len(props))
aoi_valid = bp(sum(p["valid_fraction"] for p in props) / len(props))
change = seal("change","change",[],{"aoi_id":AOI,"data_origin":"simulated","t1":n["t1"]["datetime"],"t2":n["t2"]["datetime"],"polarization":n["polarization"],
  "frequency":n["frequency"],"pixel_spacing_m":n["pixel_spacing_m"],"delta_db_threshold_cdb":int(round(cfg["delta_db_threshold"]*100)),
  "aoi_valid_bp":aoi_valid,"aoi_decrease_bp":aoi_dec,"aoi_increase_bp":aoi_inc,
  "overlay_sha256":{"t1.png":sha_file(OV/"t1.png"),"t2.png":sha_file(OV/"t2.png"),"delta.png":sha_file(OV/"delta.png")},
  "cells_geojson_sha256":sha_file(OUT/"cells.geojson")}, None, [src_nisar])
context = seal("context","context",[],{"aoi_id":AOI,"roads_total_m":roads_total,"road_buffer_m":cfg["road_buffer_meters"],
  "osm_retrieved_at":manifest["osm"]["retrieved_at"],"roads_geojson_sha256":sha_file(OUT/"roads.geojson")}, None, [src_osm])
write_json(OUT/"wonb/change.json", change); write_json(OUT/"wonb/context.json", context)
risk_blocks = {}
for p in props:
    cid = p["cell_id"]; r = res[cid]
    metrics = {"valid_bp":bp(p["valid_fraction"]),"decrease_bp":bp(p["decrease_fraction"]),"increase_bp":bp(p["increase_fraction"]),"changed_bp":bp(p["changed_fraction"]),
      "p90_abs_delta_cdb":None if p["p90_abs_delta_db"] is None else bp(p["p90_abs_delta_db"]/100),
      "opera_status":p["opera_status"],"opera_overlap_bp":None if p["opera_overlap_ratio"] is None else bp(p["opera_overlap_ratio"]),
      "road_m_near_change":p["road_m_near_change"],"road_m_total":p["road_m_total"]}
    if "insufficient" in r:
        pay = {"cell_id":cid,"status":"insufficient_quality","rank":None,"score_bp":None,"components_bp":None,"metrics":metrics,
               "explanation":[f"Only {pct(metrics['valid_bp'])}% of pixels are valid (minimum {pct(bp(cfg['quality_min_valid_fraction']))}%); excluded from the ranking."]}
    else:
        pay = {"cell_id":cid,"status":r["status"],"rank":r["rank"],"score_bp":r["score_bp"],
               "components_bp":{"change":r["change_bp"],"opera":r["opera_bp"],"road":r["road_bp"]},"metrics":metrics,"explanation":explain(p)}
    blk = seal("risk","risk",[change["id"],context["id"]],pay,RISK_VALID_UNTIL,[src_nisar,src_opera,src_osm])
    risk_blocks[cid] = blk; write_json(OUT/f"wonb/risk/{cid}.json", blk)
for cid in ranked[:cfg["top_n_actions"]]:
    rk = res[cid]["rank"]
    write_json(OUT/f"wonb/action/{rk}.json", seal("action","action",[risk_blocks[cid]["id"]],
      {"cell_id":cid,"rank":rk,"action":"field_verification_first","audience":"GORE Piura risk analyst"},ACTION_VALID_UNTIL,[src_nisar,src_opera,src_osm]))
write_json(OUT/"trusted-issuers.json", {"version":1,"issuers":[{"issuer":ISSUER,"name":"wombat-fixture-demo (clave privada descartada)","roles":["change","context","risk","action"]}]})
write_json(ROOT/"config/trusted-issuers.json", {"version":1,"issuers":[]})  if not (ROOT/"config/trusted-issuers.json").exists() else None

# ranking dorado (para los tests de Rust)
write_json(ROOT/"contracts/v1/fixtures/expected/ranking.json", {"note":"Referencia de wombat-risk sobre fixtures/cells.geojson + config/risk-config.json. Regla de redondeo: floor(x*10000+0.5). Desempate: score_bp desc, change_bp desc, cell_id asc.",
  "ranking":[{"rank":res[c]["rank"],"cell_id":c,"status":res[c]["status"],"score_bp":res[c]["score_bp"],"components_bp":{"change":res[c]["change_bp"],"opera":res[c]["opera_bp"],"road":res[c]["road_bp"]}} for c in ranked],
  "insufficient_quality":[c for c in res if "insufficient" in res[c]]})

# ---------- 5. index.json ----------
files = sorted(str(p.relative_to(OUT)) for p in OUT.rglob("*") if p.is_file() and p.name != "index.json")
write_json(OUT/"index.json", {"contract_version":CONTRACT_VERSION,"bundle_version":"v0","aoi_id":AOI,"data_origin":"simulated","generated_at":ISSUED,"core_commit":"fixture",
  "files":[{"path":f,"sha256":sha_file(OUT/f)} for f in files]})
print("bundle:", OUT, "| archivos:", len(files)+1, "| top3:", ranked[:3], "| issuer:", ISSUER[:20]+"…")
