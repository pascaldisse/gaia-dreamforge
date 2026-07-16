# GAIA DreamForge — DreamForge

Not a rewrite. A new engine.

Rust-native, Terry-shaped core (`crates/gaia-core`: ECS + schema + ops +
scheduler + package loader — nothing else), everything as packages
(`packages/`). One traced lighting system, one cluster geometry pipeline,
one solver, universe scale with zero loading, volumetric everything,
AI agents as primary users.

READ FIRST: `HANDOFF.md` (anchor) → `DREAMFORGE.md` (the charter, all
laws + amendments) → per-system rulings: `RENDER.md` · `GEOMETRY.md` ·
`PHYSICS.md` · `NEURAL.md` · `CREATE.md` · `VISIONFLOW.md` · `RAIN.md` ·
`FEATURES.md` (parity contract) · `NARUKO.md` (proof-of-concept world).
Evidence: `research/`. Canon images: `reference/naruko/`.

Lineage: founded 2026-07-16 from GAIA-World-Engine branch `rust-port`
@ f13f8668 — all commit hashes cited in the law docs resolve THERE.
The web engine remains the live reference implementation; the server,
protocol, scenes and ops are shared truth.

Gate: `cargo test --workspace`. Proof of life: `proof/`.
