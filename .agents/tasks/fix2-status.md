# Fix 2 Status — WONB 0.1 Legacy Crypto Path

**Date:** 2025  
**Commit:** `8939f35`  
**Branch:** `feat/verify-all`

---

## What was found

### F-02 — Non-JCS canonicalization in WONB 0.1

`crates/wombat-core/src/wonb.rs`, method `Wonb::to_canonical_json()`, called
`serde_json::to_string(self)` for signature computation. This does not guarantee
deterministic key ordering across runtimes (Rust/Python/JS), making cross-runtime
signature verification unreliable.

The WONB 0.2 path in `v02/envelope.rs` already uses `serde_jcs::to_vec(self)`,
which is RFC 8785 (JCS) — deterministic by spec.

### F-03 — `confidence: f64` violates R16

`WonbProvenance` in `wonb.rs` defined `confidence: f64`. This field was included
in the signed content hash (not stripped by `compute_content_hash`), violating R16
(no floats in signed content). It was also validated at runtime in `validation.rs`
with a floating-point range check `0.0..=1.0`.

Callers affected:
- `crates/wombat-core/src/wonb.rs` — struct definition and test helpers
- `crates/wombat-core/src/validation.rs` — range validation and test
- `crates/wombat-node/src/behaviour.rs` — test constructing a `WonbProvenance`
- `contracts/fixtures/*.json` — all 6 fixture files
- `contracts/schemas/wonb-*.schema.json` — all 4 schema files

### Workspace membership

`crates/wombat-risk` was already listed in `[workspace.members]` in `Cargo.toml`
at the time of this fix. No change needed.

### Is the 0.1 path dead code?

**No.** The 0.1 path is actively used:
- `wombat-node/src/main.rs` uses `validate_wonb` and `Wonb` for `publish` and
  `validate` subcommands.
- `wombat-node/src/behaviour.rs` uses `Wonb` and `WonbProvenance` for P2P
  message processing.
- `ci.yml` runs `wombat-node validate` against `contracts/fixtures/*-valid.json`
  and expects `contracts/fixtures/*-invalid.json` to fail.

Migration rather than removal was required.

---

## What was changed

### `crates/wombat-core/src/wonb.rs`

1. **`WonbProvenance`**: renamed `confidence: f64` → `confidence_bp: u16`.
   Added doc comment explaining basis-points convention (0–10000 = 0–100.00%).
   Added `Eq` derive (possible now that `f64` is gone).
2. **`Wonb`**: added `Eq` derive (all fields now implement `Eq`).
3. **`Wonb::to_canonical_json`**: replaced `serde_json::to_string(self)` with
   `serde_jcs::to_vec(self)` + UTF-8 conversion. The comment and doc string were
   updated to reflect JCS (RFC 8785).
4. **`tests::sample_provenance`**: updated `confidence: 0.89` → `confidence_bp: 8900`.

### `crates/wombat-core/src/validation.rs`

5. **`validate_timestamp`**: replaced float range check
   `!(0.0..=1.0).contains(&wonb.provenance.confidence)` with integer check
   `wonb.provenance.confidence_bp > 10000`.
6. **`tests::confidence_out_of_range_rejected`**: set `confidence_bp = 10001`
   instead of `confidence = 1.5`.

### `crates/wombat-node/src/behaviour.rs`

7. **`tests::process_incoming_wonb_valid_accepted`**: updated `confidence: 0.89`
   → `confidence_bp: 8900` in the `WonbProvenance` constructor.

### `contracts/fixtures/*.json` (6 files)

8. All six WONB 0.1 fixture files updated: `"confidence": <float>` →
   `"confidence_bp": <integer>` with the following conversions:

   | Old value | New value | File |
   |---|---|---|
   | `0.89` | `8900` | `anomaly-valid.json`, `risk-valid.json` |
   | `0.95` | `9500` | `context-valid.json` |
   | `1.0` | `10000` | `tool-valid.json` |
   | `0.5` | `5000` | `expired-invalid.json`, `malformed-invalid.json` |

### `contracts/schemas/wonb-*.schema.json` (4 files)

9. All four JSON Schema files updated: `"confidence"` required field renamed to
   `"confidence_bp"`, type changed from `{"type": "number", "minimum": 0, "maximum": 1}`
   to `{"type": "integer", "minimum": 0, "maximum": 10000}`.

---

## What was removed vs migrated

**Migrated** — the 0.1 path was not removed because it is live code (wombat-node
still uses it, CI validates 0.1 fixtures). The two specific vulnerabilities were
patched in-place:

| Vulnerability | Fix type | Location |
|---|---|---|
| F-02: `serde_json::to_string` for signing | Replaced with JCS | `wonb.rs::to_canonical_json` |
| F-03: `confidence: f64` in signed content | Type changed to `confidence_bp: u16` | `wonb.rs`, `validation.rs`, callers |

---

## Build and test results

```
cargo build --workspace
  Finished `dev` profile in 34.32s   ✓

cargo test --workspace
  wombat-core:   48 passed, 0 failed
  wombat-core (fixture integration): 11 passed, 0 failed
  wombat-cli:    31 passed, 0 failed
  wombat-cli (e2e): 13 passed, 0 failed
  wombat-node:    4 passed, 0 failed
  wombat-risk:   42 passed, 0 failed
  wombat-risk (golden): 7 passed, 0 failed
  Doc-tests:      1 passed, 0 failed
  TOTAL:        157 passed, 0 failed
```
