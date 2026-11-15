"""01_search.py - busca granules NISAR GCOV y propone pares t1/t2 comparables (§6.3).

La busqueda es publica (no pide login). Parametros segun la guia de ASF (R11):
  https://nisar-docs.asf.alaska.edu/asf-search/
  asf.search(dataset='NISAR', processingLevel='GCOV', dataMaturity='PROVISIONAL')

Uso:
  python data-pipeline/validacion/01_search.py --bbox -80.80 -5.35 -80.55 -5.10 \
      --start 2026-09-01 --end 2026-11-14 --event 2026-10-20 \
      --out data-pipeline/out/validacion/piura-01/candidates.json
"""
import argparse
import json
import sys
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

try:  # usa los certificados de Windows si la red inspecciona HTTPS
    import truststore
    truststore.inject_into_ssl()
except ImportError:
    pass

import asf_search as asf
from shapely.geometry import box, shape

from wombat_nisar.gcov import parse_granule_name


def _coverage(result, bbox):
    """Fraccion del bbox que cubre la huella del granule (None si el resultado no trae huella)."""
    try:
        footprint = shape(result.geometry)
        area = box(*bbox)
        return round(footprint.intersection(area).area / area.area, 3)
    except Exception:
        return None


def search(bbox, start, end, maturity):
    west, south, east, north = bbox
    wkt = (f"POLYGON(({west} {south},{east} {south},{east} {north},"
           f"{west} {north},{west} {south}))")
    results = asf.search(dataset="NISAR", processingLevel="GCOV", dataMaturity=maturity,
                         intersectsWith=wkt, start=start, end=end)
    rows, skipped = [], []
    for r in results:
        props = r.properties
        url = props.get("url") or ""
        name = props.get("sceneName") or props.get("fileID") or Path(url).stem
        try:
            row = parse_granule_name(name)
        except ValueError as exc:
            skipped.append(f"{name}: {exc}")
            continue
        row.update(maturity=maturity, url=url, covers_bbox=_coverage(r, bbox))
        rows.append(row)
    rows.sort(key=lambda r: r["datetime"])
    return rows, skipped


def propose_pairs(rows, event_date):
    """Por cada grupo comparable (misma geometria, modo, polarizacion Y madurez, R6/R7):
    ultima escena antes del evento y primera despues."""
    groups = defaultdict(list)
    for r in rows:
        groups[(r["track"], r["frame"], r["orbit_direction"], r["mode"], r["pol_code"],
                r["maturity"])].append(r)
    pairs = []
    for key, scenes in groups.items():
        before = [s for s in scenes if s["datetime"][:10] < event_date]
        after = [s for s in scenes if s["datetime"][:10] > event_date]
        if before and after:
            pairs.append({
                "track": key[0], "frame": key[1], "orbit_direction": key[2],
                "mode": key[3], "pol_code": key[4], "maturity": key[5],
                "t1": before[-1]["granule"], "t1_datetime": before[-1]["datetime"],
                "t2": after[0]["granule"], "t2_datetime": after[0]["datetime"],
            })
    return pairs


def consecutive_pairs(rows):
    """Pares de pasadas seguidas con todo igual, madurez incluida, sin mirar el evento."""
    groups = defaultdict(list)
    for r in rows:
        groups[(r["track"], r["frame"], r["orbit_direction"], r["mode"], r["pol_code"],
                r["maturity"])].append(r)
    out = []
    for key, scenes in groups.items():
        for a, b in zip(scenes, scenes[1:]):
            out.append({"track": key[0], "frame": key[1], "mode": key[3], "pol_code": key[4],
                        "maturity": key[5], "t1": a["granule"], "t1_datetime": a["datetime"],
                        "t2": b["granule"], "t2_datetime": b["datetime"]})
    return sorted(out, key=lambda p: p["t1_datetime"])


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bbox", nargs=4, type=float, required=True,
                    metavar=("OESTE", "SUR", "ESTE", "NORTE"))
    ap.add_argument("--start", required=True)
    ap.add_argument("--end", required=True)
    ap.add_argument("--event", default="2026-10-20", help="fecha que separa t1 de t2")
    ap.add_argument("--maturity", default="ALL",
                    choices=("ALL", "PROVISIONAL", "VALIDATED", "BETA"),
                    help="ALL busca las tres colecciones; nunca se emparejan entre si (R7)")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    rows, skipped = [], []
    levels = ("PROVISIONAL", "VALIDATED", "BETA") if args.maturity == "ALL" else (args.maturity,)
    for level in levels:
        found, bad = search(args.bbox, args.start, args.end, level)
        rows += found
        skipped += bad
    rows.sort(key=lambda r: r["datetime"])
    pairs = propose_pairs(rows, args.event)
    consecutive = consecutive_pairs(rows)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({
        "query": {"dataset": "NISAR", "processingLevel": "GCOV", "dataMaturity": args.maturity,
                  "bbox_wgs84": args.bbox, "start": args.start, "end": args.end,
                  "event": args.event},
        "retrieved_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "candidates": rows, "pairs": pairs, "consecutive_pairs": consecutive,
        "skipped": skipped,
    }, indent=2), encoding="utf-8")

    print(f"{len(rows)} granules ({args.maturity}); {len(pairs)} pares comparables que cruzan "
          f"el {args.event} -> {out}")
    for r in rows:
        print(f"  {r['datetime']}  pista {r['track']} frame {r['frame']} "
              f"{r['orbit_direction']:<10} modo {r['mode']} {r['pol_code']}  {r['maturity']:<11}"
              f"  cubre {'?' if r['covers_bbox'] is None else format(r['covers_bbox'], '.0%')}")
    for p in pairs:
        print(f"PAR que cruza el evento  pista {p['track']} frame {p['frame']} {p['maturity']}: "
              f"{p['t1_datetime']} -> {p['t2_datetime']}")
    for p in consecutive:
        print(f"par consecutivo  pista {p['track']} frame {p['frame']} modo {p['mode']} "
              f"{p['maturity']}: {p['t1_datetime'][:10]} -> {p['t2_datetime'][:10]}")
    for s in skipped:
        print("OMITIDO:", s)
    return 0 if pairs else 1


if __name__ == "__main__":
    sys.exit(main())
