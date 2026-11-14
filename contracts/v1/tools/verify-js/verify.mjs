// Uso: node verify.mjs <dir_bundle>
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import * as ed from "@noble/ed25519";
import { sha256 } from "@noble/hashes/sha256";
import { sha512 } from "@noble/hashes/sha512";
import { bytesToHex, hexToBytes } from "@noble/hashes/utils";
import canonicalize from "canonicalize";

ed.etc.sha512Sync = (...m) => sha512(ed.etc.concatBytes(...m));

const walk = (d) => readdirSync(d).flatMap((n) => { const p = join(d, n); return statSync(p).isDirectory() ? walk(p) : p.endsWith(".json") ? [p] : []; });

export function verifyBlock(blk) {
  const { id, signature, ...body } = blk;
  const digest = sha256(new TextEncoder().encode(canonicalize(body)));
  const hash_ok = "sha256:" + bytesToHex(digest) === id;
  const signature_ok = ed.verify(hexToBytes(signature), digest, hexToBytes(blk.issuer.replace("ed25519:", "")));
  return { hash_ok, signature_ok };
}

if (process.argv[1].endsWith("verify.mjs")) {
  const files = walk(join(process.argv[2], "wonb")).sort();
  let bad = 0;
  for (const f of files) {
    const r = verifyBlock(JSON.parse(readFileSync(f, "utf8")));
    if (!r.hash_ok || !r.signature_ok) { bad++; console.log("✗", f, r); }
  }
  console.log(`${files.length} bloques: ${files.length - bad} ok, ${bad} con error`);
  process.exit(bad ? 1 : 0);
}
