//! Uso: cargo run --manifest-path contracts/v1/tools/verify-rs/Cargo.toml -- <dir_bundle>
//! Verifica por cada bloque WONB: hash (JCS RFC 8785) y firma Ed25519 sobre los 32 bytes del hash.
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::exit};

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() { walk(&p, out) } else if p.extension().is_some_and(|x| x == "json") { out.push(p) }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("falta <dir_bundle>");
    let mut files = vec![];
    walk(&Path::new(&root).join("wonb"), &mut files);
    files.sort();
    let (mut ok, mut bad) = (0, 0);
    for f in &files {
        let mut v: Value = serde_json::from_str(&fs::read_to_string(f).unwrap()).unwrap();
        let obj = v.as_object_mut().unwrap();
        let id = obj.remove("id").unwrap().as_str().unwrap().to_string();
        let sig = obj.remove("signature").unwrap().as_str().unwrap().to_string();
        let issuer = obj["issuer"].as_str().unwrap().trim_start_matches("ed25519:").to_string();
        let digest = Sha256::digest(serde_jcs::to_vec(&v).unwrap());
        let hash_ok = format!("sha256:{}", hex::encode(digest)) == id;
        let pk: [u8; 32] = hex::decode(issuer).unwrap().try_into().unwrap();
        let sg: [u8; 64] = hex::decode(sig).unwrap().try_into().unwrap();
        let sig_ok = VerifyingKey::from_bytes(&pk).unwrap().verify(&digest, &Signature::from_bytes(&sg)).is_ok();
        if hash_ok && sig_ok { ok += 1 } else { bad += 1; println!("✗ {} hash_ok={hash_ok} signature_ok={sig_ok}", f.display()) }
    }
    println!("{} bloques: {ok} ok, {bad} con error", files.len());
    exit(if bad == 0 { 0 } else { 1 });
}
