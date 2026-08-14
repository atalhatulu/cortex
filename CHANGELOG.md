# Changelog

All notable changes to **CORTEX Archiver**.

## [0.1.0-beta.1] — 2026-08-14

### Added
- **Desktop GUI (Tauri)** bundles the engine; mode dropdown added
  (Balanced / Max ratio / Max speed) wired to the library mode flags.
- `docs/ARCHITECTURE.md` established as the single architectural authority
  (superseded reports moved to `docs/archive/2026-08-14/`).

### Changed
- **Default compression mode is now Balanced (CTXT, BWT + Order-1 tANS).**
  Previous default was Max ratio (CTX8). Explicit modes via flags:
  `--ratio` (CTX8) and `--fast` (CTXF). CLI `--tans` flag removed in favour
  of the now-default Balanced mode.
- README rewritten with measured 6-core numbers and the three-mode table.
- GUI Rust `compress_cmd` signature fixed (was passing 3 booleans to a
  2-boolean library API — broke compilation).

### Fixed
- **GUI compilation (E0061)**: stale 3-arg call to `compress_file_with_progress`
  corrected.

### Removed
- `--tans` CLI flag (superseded by default Balanced mode).

### Performance (measured, unchanged engine)
- Order-2 tANS / adaptive-order-1 tANS / read-prefetch on inverse-BWT were each
  probed and found to be dead-ends; documented in `docs/ARCHITECTURE.md`.
  No speculative ratio claims — every number is a byte-exact `cmp`/MD5 roundtrip
  on the 6-core dev box.

## [Unreleased]
