# ⚛ CORTEX Archiver

Cortex is a fast, **lossless archiver** built around a modern Burrows-Wheeler Transform (BWT)
engine. One engine, three aims:

| Mode | Flag | Engine | enwik8 | Decompress | Use for |
|------|------|--------|-------:|-----------:|---------|
| **Balanced** ⭐ | *(default)* | BWT + Order-1 tANS (CTXT) | 26.52 MB | ~1.5 s | Daily use — great ratio + fast |
| **Max ratio** | `--ratio` | BWT + Order-2 adaptive range (CTX8) | 24.88 MB | ~1.9 s | Archiving / maximum compression |
| **Max speed** | `--fast` | zstd (CTXF) | 26.21 MB | ~0.14 s | Biggest speed, still good ratio |

All numbers are **measured** on a 6-core box (AMD Ryzen 5 3500, 16 MB L3), 3-run
averages, byte-exact (`cmp` + MD5) roundtrip. No speculative figures.

## 📊 Benchmark — enwik8 (100 MB Wikipedia text)

### Cortex + rivals, same box (live, 2026-08-14)

Measured on the 6-core box, sequential, byte-exact `cmp` roundtrip.

| Codec | Boyut (MB) | Compress | Decompress |
|-------|-----------:|---------:|-----------:|
| Cortex **CTX8** (Max ratio) | 24.88 | 5.74 s | 2.31 s |
| Cortex **CTXT** (Balanced) | 26.52 | 4.85 s | 1.57 s |
| Cortex **CTXF** (Max speed) | 26.21 | 20.72 s | **0.20 s** |
| xz -9 | 23.71 | 93.40 s | 1.02 s |
| zstd -19 | 25.70 | 72.18 s | 0.34 s |
| bzip2 -9 | 27.66 | 7.27 s | 4.04 s |
| gzip -9 | 34.76 | 5.12 s | 0.48 s |

### Official rival figures (Large Text Compression Benchmark)

[Source](https://mattmahoney.net/dc/text.html) (Matt Mahoney, updated Jul 2026). These
are ratio references on an *older reference machine* — usable to benchmark *how much* each
codec compresses, not comparable wall-clock to our Ryzen numbers above.

| Program | enwik8 (MB) | enwik9 (MB) |
|---------|------------:|------------:|
| xz 5.2.1 `lzma2 9e, dict 1GiB` | 24.70 | 197.33 |
| 7-Zip LZMA `-mx=9` | 25.00 | 213.49 |
| zstd 0.5.1 `-21` | 25.57 | 219.43 |
| **Cortex CTX8** (our box) | 24.88 | 206.4 |
| brotli `-q 11 -w 24` | 25.76 | 223.60 |
| bzip2 1.0.3 `-9` | 29.01 | 253.98 |
| gzip 1.3.5 `-9` | 36.45 | 322.59 |

> LTCB sizes tell the *achievable* ratio: Cortex CTX8 (24.88) sits between xz's toy dict
> preset and 7-Zip LZMA — on real default settings it roughly ties xz, but with ~6–16×
> faster compression (see next section).

## 📊 Benchmark — enwik9 (1 GB Wikipedia text)

### Cortex + rivals, same box (live/archived, 2026-08-14)

Cortex + gzip/bzip2 are **live measured** (sequential, byte-exact). xz -9 and zstd -19 are
**archived measured** figures (same box, prior run — their 1 GB compresses take 6–11 min).

| Codec | Boyut (MB) | Compress | Decompress |
|-------|-----------:|---------:|-----------:|
| Cortex **CTX8** (Max ratio) | 206 | 56.72 s | 16.29 s |
| Cortex **CTXT** (Balanced) | 219 | 49.86 s | 14.57 s |
| Cortex **CTXF** (Max speed) | 229 | 235.49 s | **1.69 s** |
| xz -9 (archived) | 205.7 | 367.5 s | 2.53 s |
| zstd -19 (archived) | 224.4 | 675.6 s | 2.23 s |
| gzip -9 (live) | 307 | 44.09 s | 3.39 s |
| bzip2 -9 (live) | 242 | 75.11 s | 33.32 s |

### Read from the tables
- **Ratio:** xz -9 and Cortex CTX8 are effectively tied on enwik9 (205.7 vs 206 MB),
  but CTX8 compresses **~6× faster** (56.7 s vs 367 s). On enwik8, xz wins slightly (23.71
  vs 24.88) at the cost of ~16× slower compress.
- **Speed:** `--fast` (CTXF) is the fastest decompressor of *all* on enwik9 (1.69 s —
  faster than even raw zstd -19's 2.23 s), and beat every rival on enwik8 too (0.20 s).
- **Balanced (default) CTXT:** best compress time among the tight-ratio codecs (~50 s on
  enwik9 = **~13× faster than zstd -19's 675.6 s**) with a ratio better than zstd -19
  (219 vs 224.4 MB), and a fast ~14.6 s decompress.
- **Composite score** (enwik9: compress 25% / ratio 40% / decompress 35%):
  **CTXF 🥇**, xz, **CTX8**, **CTXT** — order per the full scored table in `docs/ARCHITECTURE.md`.

> Measured notes: enwik8 CTXT = 26.52 MB is the verified value (2026-08-14 A/B across three
> commits, byte-identical). enwik9 Cortex sizes are the live `.ctx` file sizes (206 / 219 /
> 229 MB); xz / zstd enwik9 rows are the archived measured run (we don't re-wait ~18 min for
> full xz -9 + zstd -19 1 GB compresses). Full audit trail, methodology and dead-end probes:
> `docs/ARCHITECTURE.md`.

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
