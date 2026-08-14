# ⚛ CORTEX Archiver

Cortex is a fast, **lossless archiver** built around a modern Burrows-Wheeler Transform (BWT)
engine. One engine, three aims:

| Mode | Flag | Engine | enwik8 | Decompress | Use for |
|------|------|--------|-------:|-----------:|---------|
| **Balanced** ⭐ | *(default)* | BWT + Order-1 tANS (CTXT) | 26.52 MB | ~1.5 s | Daily use — great ratio + fast |
| **Max ratio** | `--ratio` | BWT + Order-2 adaptive range (CTX8) | 24.88 MB | ~1.9 s | Archiving / maximum compression |
| **Max speed** | `--fast` | zstd (CTXF) | 26.21 MB | ~0.14 s | Biggest speed, still good ratio |

All numbers are **measured** on a 6-core box (AMD Ryzen 5 3500, 16 MB L3) — see
`docs/ARCHITECTURE.md` for the full measured table (incl. `enwik9`) and the audit trail.
No speculative figures; every claim traces to a byte-exact `cmp` + MD5 roundtrip.

## 🚀 The Architecture

1. **Suffix-Array BWT**: `divsufsort` builds suffix arrays of 16 MB blocks in parallel.
2. **Implicit EOF / LF-Mapping**: a rank-offset (`pidx`) keeps the BWT bijection exact,
   no EOF padding needed.
3. **MTF + RLE**: standard transforms that cluster identical bytes.
4. **Entropy layer**: order-1 static tANS (Balanced / CTXT) or order-2 adaptive range
   coder with bit-level context mixing (Max ratio / CTX8).
5. Content-aware classifier (`content.rs`): already-compressed blocks (JPEG/PNG/zip/PDF…)
   are stored raw instead of recompressed, and executables/text are classified.

## ⌨️ CLI Usage

```bash
# Build
cargo build --release --manifest-path core/Cargo.toml

# Balanced (default) — day-to-day
cortex compress <file> [--level 3]

# Maximum ratio
cortex compress --ratio <file>

# Maximum speed
cortex compress --fast <file>

# Decompress (auto-detects the mode from the archive header)
cortex decompress <file>.ctx
```

Optional: `--password <pw>` (encryption), `--split <MB>` (volume splitting),
`--threads N`, `--force`, `-q/--quiet`, `-v/--verbose`.

## 🖥️ Desktop GUI (Tauri)

A Tauri (React 19 + Rust) desktop frontend bundles the same library, with a folder
browser, multi-select, live progress, and a mode dropdown:

```bash
cd ui && npm install && npm run tauri dev
```

## 🧪 Status

**Beta.** The library + CLI are byte-exact on every tested corpus; the GUI compiles and
bundles the same engine. `--ratio` roundtrip is verified at level 3 (16 MB blocks).
Known caveats are tracked in `docs/ARCHITECTURE.md`.
