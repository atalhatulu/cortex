# ⚛ CORTEX Engine

Cortex is an experimental, extremely fast lossless data compressor designed around a modern Burrows-Wheeler Transform (BWT) architecture. 

It aims to push the boundaries of BWT-based compression speed and ratio, proving that a pure BWT engine can still outperform some of the best modern dictionary-based algorithms (like ZSTD) on highly repetitive or text-based datasets like `enwik8`.

## 🚀 The Architecture

Cortex abandons traditional O(n) Cyclic BWT construction in favor of a **Suffix Array based BWT** combined with an implicit EOF approach. 

1. **Suffix Array Construction**: Uses `divsufsort` to extremely quickly build the suffix array of 16 MB data chunks in parallel.
2. **Implicit EOF & Custom LF-Mapping**: The original BWT bijection is maintained perfectly using a mathematically pristine rank-offset (`pidx`) technique, eliminating the need to append explicit EOF bytes to the chunks.
3. **Move-to-Front (MTF) & RLE2**: Standard BWT transformations to group identical bytes.
4. **Context Modeling**: An adaptive probability mixer (Order-1 and Order-2 contexts) tracks bit probabilities.
5. **Range Coder**: A highly optimized carryless arithmetic range coder squashes the predicted bits into their theoretical minimum sizes. 

## 📊 Benchmarks (The 24.99 MB Record)

Tested on the standard 100 MB `enwik8` dataset.

| Compressor | Size (MB) | Time (s) |
|------------|-----------|----------|
| GZIP / ZIP (-9) | ~ 35.0 MB | ~ 7.7 s |
| BZIP2 (-9) | ~ 28.3 MB | ~ 8.8 s |
| ZSTD (-19) | ~ 25.7 MB  | ~ 11.0 s |
| **Cortex** | **24.99 MB** | **~ 5.5 s** |

*Cortex compresses enwik8 smaller and nearly 2x faster than ZSTD at its maximum compression level!*

## ⚙️ Usage

Cortex is designed to be incredibly simple from the terminal. 

**Build and Install:**
```bash
cargo install --path core/ --force
```

**Compress a file:**
```bash
cortex compress enwik8
```
*(Automatically creates `enwik8.crx`)*

**Decompress a file:**
```bash
cortex decompress enwik8.crx
```
*(Automatically restores `enwik8`)*
