# pi-musubi-core

Web PoC focused on Pi Network sign-in and a single `10 Pi deposit` action.

## Foundation alignment

This repo is the canonical implementation repo for MUSUBI Day 1.
The constitutional and design source of truth is `mt4110/musubi-foundation`.

Pinned foundation reference:
See `docs/foundation_lock.md`.

Current implementation milestone:
M1 - Core Truth and Orchestration Baseline

See `docs/foundation_lock.md` for the pinned design corpus.

Implementation agents must treat `docs/foundation_lock.md` as the mandatory reading gateway before changing code.

## Local verification

From the repository root, run the Day 1 HTTP smoke suite with:

```bash
make http-day1-smoke
```

For the full local backend verification plus the HTTP suite, run:

```bash
make verify-local-http
```

These targets delegate to `apps/backend`. The Rust workspace root remains
`apps/backend`.
