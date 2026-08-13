# Cortex: Üç Modlu Decompress İyileştirme (ratio + hız) — Orkestratör Planı

**Hedef:** Mevcut BWT ratio modu aynen kalsın (CTX8). Ona iki hız modu ekle:
- **CTXT — tANS modu** (Strateji A): BWT+MTF+RLE korunur, binary range coder yerine **tANS** entropy, blok 16MB→4MB. Hedef enwik9 ~4-5s, enwik8 ratio ~25.5-27MB.
- **CTXF — zstd fast-mod** (Strateji B): zstd level CLI'dan kontrol edilebilir (varsayılan -19). Hedef enwik9 ~2s, enwik8 ratio ~26.9MB (zstd -19).

> Not: CLI'daki mevcut `--fast` zaten CTXF (zstd) yoluna basıyor ama zstd level'i `3`'e sabitlenmiş (kötü ratio ~35MB). Bu düzeltilecek: zstd level'i `-l`'den gelecek. Yeni `--tans` bayrağı CTXT'i seçer. `--fast` geriye dönük uyum için CTXF'ye eşlenir.

---

## GÖREV 1 — (agy) CTXF zstd level kontrolü + CTXT scaffolding [lib.rs, cli.rs, main.rs]

**Mevcut durum:** `lib.rs::compress_file_with_progress` `fast=true` ise `zstd::encode_all(chunk, 3)` sabit level 3 kullanıyor (`lib.rs` line ~183). `--fast` CLI bayrağı `main.rs::52,59` üzerinden geçiriliyor.

**Yapılacaklar:**
1. `compress_file_with_progress`'e zstd level'ını parametre olarak ekle (veya fast modda `zstd::encode_all(chunk, level)` — level default -19). Fonksiyon imzası: `_level: u8` zaten mevcut; onu zstd level'i olarak fast-branch'te kullan. Ama dikkat: `_level` şu an BWT blok boyutunu belirliyor (1→1MB, 3→16MB). Fast modda zstd level'i olarak yorumla; normal modda blok boyutu olarak koru. (`-l 19` fast modda zstd -19 demek.)
2. `cli.rs`'ye `--tans` bayrağı ekle (`#[arg(long, default_value_t = false)] tans: bool`), CTXT modunu seçer.
3. `main.rs`'te `tans` flag'ini `compress_file_with_progress`'e yeni bir parametre olarak geçir (ör. `fast` yerine bir `mode: Mode` enum: `Normal / Tans / Zstd`). En temiz: `mode` parametresi.
4. `lib.rs::decompress_file_with_progress` CTXT deşifresi için scaffolding: `MAGIC_FAST` (CTXF) `is_fast` branch'ine ek bir `is_tans` (CTXT, 4-byte `b"CTXT"`) branch'i ekle — şimdilik tANS decode yok, bu yüzden `Err("CTXT not yet implemented")` döndür. (Görev 2'de doldurulur.)

**Format korumaları:**
- CTX8 (`rangecoder` path) **DEĞİŞMEZ** — geriye dönük uyumlu.
- CTXF (`is_fast` / zstd) byte-exact roundtrip korunur.
- CTXT `decompress` şimdilik açıkça `Err` döndürür; roundtrip testi CTXT'i ele almamalı.

**Milestone:** `cargo build --release --manifest-path core/Cargo.toml` temiz. `cargo test --manifest-path core/Cargo.toml --test roundtrip` 12 test YEŞİL (CTX8 değişmediği için). `cortex compress data/enwik8 --fast -l 19` → CTXF dosya üretir, 26.9MB civarı, decompress byte-exact. `--tans` compress'i henüz `Err` verebilir (scaffolding) veya mim validation yapabilir.

---

## GÖREV 2 — (agy) CTXT: tANS entropy + blok 4MB [tans.rs YENİ, mtf.rs, lib.rs]

**Bu, Strateji A'nın tam uygulaması.** Görev 1'deki `--tans` scaffolding'ini doldurur.

**Yapılacaklar:**
1. Yeni `core/src/tans.rs`: tANS (rANS-family) decoder/encoder. Token histogramı → normalized tANS state table → decode/encode. Sembol uzayı: MTF token = 0..256 (u16). Histogram: 257×u16 (514B) her entropy bloğunun başında serialize.
2. `mtf.rs`: MTF+RLE → token stream aynı kalır (BWT bijection korunur). Encoding path'e: token histogramı çıkar, tANS build et, token'ları tANS ile kodla. Decode path: histogramı oku, aynı tANS tablosunu build et, token'ları çöz → `decode_rle_mtf_bwt` aynı kalır.
3. `lib.rs`: CTXT modunda entropy aşaması `rangecoder::decode_tokens` yerine tANS kullanır. Yeni magic `b"CTXT"`. `block_size_for_level`: CTXT modunda level 3 → 4MB blok (`lib.rs:60`). `LANES=8`, `pidx[8]` formatı **dokunulmaz** — chunk header aynı kalır, sadece entropy akışının başına histogram eklenir.
4. `MtfModel::new()` blok başına 8MB memset — pooled `Vec`'e çevir (`lib.rs:435`; 4MB bloklarda enwik9'da ~1000 alloc).

**Byte-exact kuralı:** histogram encode tarafında token stream'inden deterministik çıkarılmalı; decode'ta aynı build algoritması → tANS tablosu birebir aynı. `tests/roundtrip.rs` + `cmp` + md5 ile doğrula.

**Risk/focus (fcc-claude):** MTF token dağılımında 0'da dev pik (RLE) → tANS normalizasyonu dikkat; hatalı histogram = bozuk stream; blok küçültme ratio kaybı.

**Milestone:** `cargo build` temiz. `cargo test --manifest-path core/Cargo.toml --test roundtrip` YEŞİL. `cortex compress data/enwik8 --tans` → CTXT dosya, decompress byte-exact. enwik8 ratio ~25.5-27MB. enwik9 CTXT decompress ~4-5s (temiz yük).

---

## Görev sırası ve sahiplik
- **GÖREV 1 (CTXF zstd level + scaffolding):** agy → fcc-claude review → Hermes verify/commit.
- **GÖREV 2 (CTXT tANS):** agy → fcc-claude review → Hermes verify/commit.
- Sıralı: Her iki görev de `lib.rs`+`mtf.rs`'e dokunuyor → **paralel race olmaz, sıralı**. (Görev 2, Görev 1'den sonra.)
- fcc-claude sadece inceleme (read-only); kod yazımı agy.

## Milestone özeti (kabul kriteri)
| Mod | Magic | Ratio (enwik8) | Decompress enwik9 | Roundtrip |
|-----|-------|----------------|-------------------|-----------|
| BWT ratio | CTX8 | ~24.88MB | ~14.7s | byte-exact |
| tANS | CTXT | ~25.5-27MB | ~4-5s | byte-exact |
| zstd fast | CTXF | ~26.9MB (-19) | ~2s | byte-exact |
