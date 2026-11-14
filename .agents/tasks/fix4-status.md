# Fix 4 — Pin GitHub Actions to Commit SHAs

**Status:** COMPLETE  
**Date:** 2025

---

## Files Changed

### `.github/workflows/ci.yml`
| Action | Before | After |
|---|---|---|
| `actions/checkout` | `@v4` | `@11bd71901bbe5b1630ceea73d27597364c9af683 # v4` |
| `dtolnay/rust-toolchain` | `@stable` | `@4f366e621dc797ca1b35e7bd81a7f017cc10e7c0 # stable` |
| `actions/cache` | `@v4` | left at `@v4` with TODO comment (SHA not provided) |

Both occurrences of `checkout` and `rust-toolchain` (jobs `quality` and `fixtures`) were updated.

### `.github/workflows/ci-contracts.yml`
| Action | Before | After |
|---|---|---|
| `actions/checkout` (×4 jobs) | `@v4` | `@11bd71901bbe5b1630ceea73d27597364c9af683 # v4` |
| `dtolnay/rust-toolchain` | `@stable` | `@4f366e621dc797ca1b35e7bd81a7f017cc10e7c0 # stable` |
| `actions/cache` | `@v4` | left at `@v4` with TODO comment |
| `actions/setup-python` | `@v5` | left at `@v5` with TODO comment |
| `actions/setup-node` | `@v4` | left at `@v4` with TODO comment |

### `.github/workflows/publish-data.yml`
| Action | Before | After |
|---|---|---|
| `actions/checkout` | `@v4` | `@11bd71901bbe5b1630ceea73d27597364c9af683 # v4` |
| `actions/setup-python` | `@v5` | left at `@v5` with TODO comment |
| `actions/upload-pages-artifact` | `@v3` | left at `@v3` with TODO comment |
| `actions/deploy-pages` | `@v4` | left at `@v4` with TODO comment |

---

## TODOs Left (SHAs not provided in task spec)

The following actions were not in the provided SHA list and were left with `# TODO: pin to SHA — currently uses floating tag <tag>` comments:

| Action | Tag | Files |
|---|---|---|
| `actions/cache` | `@v4` | `ci.yml`, `ci-contracts.yml` |
| `actions/setup-python` | `@v5` | `ci-contracts.yml`, `publish-data.yml` |
| `actions/setup-node` | `@v4` | `ci-contracts.yml` |
| `actions/upload-pages-artifact` | `@v3` | `publish-data.yml` |
| `actions/deploy-pages` | `@v4` | `publish-data.yml` |

To resolve these TODOs, look up the current SHA for each action tag on GitHub (e.g., `https://github.com/actions/cache/commits/v4`) and replace the floating tag with the full 40-character commit SHA.

---

## No Logic Changes

Only `uses:` version references were modified. All `run:` steps, `with:` blocks, job conditions, triggers, permissions, and concurrency settings are unchanged.
