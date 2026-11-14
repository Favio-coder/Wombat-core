#!/usr/bin/env python3
"""Verifica un bundle contra contracts/v1: esquemas + sha256 + firmas + cadena (7.4) + sin floats (R16).

Uso:  python3 contracts/v1/tools/verify_bundle.py <dir_bundle> [--as-of 2026-11-14T17:30:00Z]

--as-of: instante contra el cual se evalúa el TTL. Por defecto `generated_at` del bundle,
porque un bundle estático es una FOTO histórica (ver contracts/v1/README.md, 'TTL en bundles estáticos').
"""
import argparse, hashlib, json, sys
from datetime import datetime, timezone
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.exceptions import InvalidSignature

V1 = Path(__file__).resolve().parents[1]
def load(p): return json.loads(Path(p).read_text(encoding="utf-8"))
def ts(s): return datetime.fromisoformat(s.replace("Z", "+00:00"))
def jcs(o):
    def chk(x):
        if isinstance(x, float): raise ValueError("float en bloque firmado (R16)")
        if isinstance(x, dict): [chk(v) for v in x.values()]
        if isinstance(x, list): [chk(v) for v in x]
    chk(o); return json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()

def registry():
    reg = Registry()
    for f in V1.glob("*.schema.json"):
        doc = load(f); reg = reg.with_resource(doc["$id"], Resource.from_contents(doc, default_specification=DRAFT202012))
    return reg
def validator(name, reg):
    return Draft202012Validator(load(V1 / name), registry=reg, format_checker=FormatChecker())

def main():
    ap = argparse.ArgumentParser(); ap.add_argument("bundle"); ap.add_argument("--as-of")
    a = ap.parse_args(); B = Path(a.bundle); errs = []
    def err(m): errs.append(m); print("  ✗", m)
    reg = registry()
    idx = load(B / "index.json")
    for e in validator("bundle-index.schema.json", reg).iter_errors(idx): err(f"index.json: {e.message}")
    major = int(idx["contract_version"].split(".")[0]); want = int((V1 / "VERSION").read_text().split(".")[0])
    if major != want: err(f"contract_version {idx['contract_version']} incompatible con contratos v{want}")
    for f in idx["files"]:
        p = B / f["path"]
        if not p.exists(): err(f"falta {f['path']}")
        elif hashlib.sha256(p.read_bytes()).hexdigest() != f["sha256"]: err(f"sha256 distinto: {f['path']}")
    for name, schema in [("manifest.json","manifest"),("cells.geojson","cells"),("risk-config.json","risk-config")]:
        for e in validator(f"{schema}.schema.json", reg).iter_errors(load(B / name)): err(f"{name}: {e.message}")
    trusted = {i["issuer"]: i for i in load(B / "trusted-issuers.json")["issuers"]}
    as_of = ts(a.as_of or idx["generated_at"])
    blocks = {}
    for p in sorted((B / "wonb").rglob("*.json")):
        blk = load(p); blocks[blk["id"]] = (p, blk)
    nb = 0
    for bid, (p, blk) in blocks.items():
        nb += 1; rel = p.relative_to(B)
        for e in validator("wonb-envelope.schema.json", reg).iter_errors(blk): err(f"{rel}: {e.message}")
        body = {k: v for k, v in blk.items() if k not in ("id", "signature")}
        digest = hashlib.sha256(jcs(body)).digest()
        if "sha256:" + digest.hex() != blk["id"]: err(f"{rel}: hash_ok = false")
        iss = trusted.get(blk["issuer"])
        if not iss or blk["type"] not in iss["roles"]: err(f"{rel}: issuer_trusted = false")
        try: Ed25519PublicKey.from_public_bytes(bytes.fromhex(blk["issuer"].split(":")[1])).verify(bytes.fromhex(blk["signature"]), digest)
        except (InvalidSignature, ValueError): err(f"{rel}: signature_ok = false")
        if blk["valid_until"] and not (ts(blk["issued_at"]) <= as_of < ts(blk["valid_until"])): err(f"{rel}: expirado a {as_of.isoformat()} (valid_until {blk['valid_until']})")
        for par in blk["parents"]:
            if par not in blocks: err(f"{rel}: padre {par[:19]}… no existe (chain_ok = false)")
    ch = [b for _, b in blocks.values() if b["type"] == "change"]
    if len(ch) != 1: err(f"se esperaba 1 bloque change, hay {len(ch)}")
    elif (B / "overlays").exists():
        for k, h in ch[0]["payload"]["overlay_sha256"].items():
            if hashlib.sha256((B / "overlays" / k).read_bytes()).hexdigest() != h: err(f"overlay {k} no coincide con el bloque change")
        if hashlib.sha256((B / "cells.geojson").read_bytes()).hexdigest() != ch[0]["payload"]["cells_geojson_sha256"]: err("cells.geojson no coincide con el bloque change")
    print(f"{'OK ' if not errs else 'FALLA'}  {B}  |  {nb} bloques | as_of {as_of.isoformat()} | {len(errs)} error(es)")
    sys.exit(1 if errs else 0)
main()
