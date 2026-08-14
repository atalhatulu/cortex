# Cortex — Mimari & Durum Raporu (Authority, 2026-08-14)

> Bu dosya, kod tabanındaki **gerçeklere** dayanır. Aİ modellere (AGY / fcc-claude / Gemini vb.)
> değerlendirme için verilecek nesnel referanstır. Sayılar kendi makinende ölçüldü (byte-exact,
> 3-run ort), tahmin değildir. Eski `SISTEM_DURUM_RAPORU.md` / `BENCHMARKS.md` bu dosyanın yerini alır.
> **Güncel tutmak için** sayılar/modlar değiştiğinde `git log --oneline`, `wc -l core/src/*.rs`,
> `grep -n "pub fn"` ile yeniden üret; hafızaya güvenme (oranlar/iş parçacığı dağılımları kayar).

---

## 1. Proje Tanımı

- **Repo:** `~/Documents/GitHub/cortex` (github.com/atalhatulu/cortex) — sıfırdan yazılmış **Rust**
  kayıpsız sıkıştırıcı / arşivci. ~3.000 satır (`core/src`).
- **Ürün hedefi:** zip/RAR tarzı **tam arşivci**. Bir **standart (varsayılan) dengeli mod** + max-ratio
  + max-hız modları. Girdi: CLI (`cli.rs`) + Tauri masaüstü GUI (`ui/`).
- **Paralellik:** blok-paralel (rayon), iki ayrı decompress havuzu + kanal.
- **Ölçüm donanımı:** AMD Ryzen 5 3500 — **6 fiziksel çekirdek, SMT yok**, L2 3MB, L3 16MB (2×8MB).

---

## 2. Mod Yapısı (her biri ayrı 4-bayt sihir + ayrı decoder)

"Mod" = üretim anında seçilen **tamamen farklı algoritma zinciri**. Decompress, arşivin üretildiği
modla yapılır; modlar birbirini **açamaz** (arşiv, üreten encoder'ın modelini kodlar).

| Mod | Sihir | Algoritma zinciri | Karakter |
|-----|-------|-------------------|----------|
| **CTX8** | `CTX8` | BWT → MTF → RLE → **order-2 aritmetik (range coder)** | Max ratio (arşiv) |
| **CTXT** | `CTXT` | BWT → MTF → RLE → **order-1 tANS** entropy | Dengeli (standart aday) |
| **CTXF** | `CTXF` | **zstd** (hazır kütüphane, `-l` seviye) | Max hız |
| **CDXL** | `CDXL` | LZ77 → order-1 tANS + zstd meta | **Ölü deneme — kodda YOK** |

- CTXF'te content/store yoktur (zstd hazır). classify/STORE **yalnız BWT modlarında** (CTX8 + CTXT) aktiftir.
- Sihir 4-byte; imzaya bakılarak decoder seçilir (lib.rs `is_ctx8 / is_fast / is_tans`).

---

## 3. Content-Aware Sınıflandırma + STORE (content.rs) — "Hibrit motor" muadili

Kullanıcının/önerilerin "Hibrit: Data Detector → BWT veya LZ" dediği kavram cortex'te **tek motor +
akıllı atlama** olarak devrede. Zaten sıkıştırılmış bloklar (JPEG/PNG/zip/gzip/zstd/xz/7z/PDF/FLAC/
OGG/MP4/WebM/WAV…) full pipeline'a sokulmaz, **raw STORE** edilir → boşa zaman + ratio kaybı önlenir.

- **`ContentKind`**: `Text | Binary | Executable | AlreadyCompressed`.
- **`classify(data)`** iki aşamalı:
  1. **Magic-byte tablosu** (`magic_kind`): PNG/JPEG/GIF/ZIP/gzip/zstd/xz/bzip2/7z/PDF/FLAC/OggS/
     RIFF(WAV/AVI/WebP)/Matroska/ID3 — en uzun imza önce.
  2. **Exec başlık** (`is_executable`): MZ(PE) / ELF / Mach-O → `Executable` (BWT **+ E8/E9** ön-geçit).
  3. **Shannon entropisi** ≥ `7.0` bits/byte → `AlreadyCompressed` (metin ~4-4.5, gerçek sıkıştırılmış ~7.5-8).
  4. **Printable-ratio** > %90 → `Text`, değilse `Binary`. (`SAMPLE_BYTES=64KB`, `MIN_SAMPLE_SIZE=1024`.)
- **Pitfall (fixed):** `MIN_SAMPLE_SIZE` kapısı magic + exec kontrollerden **SONRA** gelir; küçük
  already-compressed bloğun yanlışlıkla Binary→BWT'ye gitmesi önlendi.
- **STORE bir 4. mod değil, BWT modlarında bir bayraktır:** `num_tokens` bit 30 = stored
  (bit 31 = `is_exec`). `store_block()` → `pidx[8]=0` + `num_tokens`(bit30|size) + ham bayt; decoder
  bit30 görürse `Stored(vec)` döner. Eski arşivler okunur (bit30=0).
- **Ratio tarafı** metinde byte-identical (enwik9 CTX8 çıktısı eski commit'le aynı 202,837,581 bayt).

---

## 4. Sıkıştırma Pipeline'ı (blok-paralel)

1. Girdi eşit bloklara bölünür (`block_size_for_level`: lvl1=1MB … lvl3=16MB varsayılan). Her blok
   başına `u32` boyut öneki; bloklar zincirleme/yapılandırılmış iç içe yazılır. Batch boyutu RSS'ye göre.
2. Her blok `classify()` → STORE edilecekse ham yaz; değilse aşağıdaki BWT iskeleti.
3. **BWT iskeleti** (`mtf::bwt_mtf_rle`):
   - `divsufsort::sort` suffix array inşa (BWT ratio'nun kalbi).
   - BWT dönüşümü; blok **8 lane** (`LANES=8`), her lane start'ı `pidx[8]`.
   - **MTF** (move-to-front; `0` dev pik → uzun sıfır koşuları) → **RLE** (sıfır koşuları binary).
4. **Entropy (moda göre):**
   - **CTX8:** `MtfModel` order-2 + `rangecoder` (LZMA-tarzı carryless, 32-bit range, `PROB_BITS=12`).
   - **CTXT:** order-1 2D histogram (256 ctx × 257 sembol) → her ctx için tANS tablosu
     (`TABLE_BITS=11`); histogram (~263KB) zstd ile sıkıştırılıp **gömülür**.
   - **CTXF:** `zstd::encode_all(chunk, level)` doğrudan.
5. Exec blok → E8/E9 filter (x86 call/jump göreli). `filters.rs`.

---

## 5. Decompress Pipeline'ı (iki ayrı rayon havuzu + kanal, örtüşmeli)

- **Stage1** (entropy ağırlıklı): memory-map girdiden blok oku → imzaya göre zstd / tANS+hist / range
  decode → `Normal{pidx,tokens,is_exec,size}` veya `Stored(vec)` → bounded kanala gönder.
- **Stage2** (ters dönüşüm): kanaldan `par_bridge` ile tüket → `Normal` için **inverse-BWT** (8 lane) →
  RLE çöz → MTF ters → E8/E9. **Yazıcı thread** blokları `BTreeMap` + `global_chunk_idx` ile sıralayıp
  orijinal sırada yazar.
- **Thread dağılım:** `total_threads=6`, `stage1=(6*2)/3=4`, `stage2=2`. `channel_cap=max(24, threads*3)`.
- **İki AYRI havuz şart** — tek ortak global havuz kanal backpressure + rayon theft'ten **deadlock** yapar.

---

## 6. Ölçülmüş Sonuçlar (byte-exact, 3-run ort)

Disiplin: `rm -f` çıktı önce (Cortex var olan çıktıda işi atlar → sahte 0.0s), python `perf_counter` 3-run.

### enwik9 (1GB)
| Codec | Ratio (MB) | Compress | Decompress |
|-------|-----------:|---------:|-----------:|
| Cortex **CTX8** | 206.4 | ~38s | 15.75s |
| Cortex **CTXT-O1** | 219.7 | ~31s | ~11.5s (idle 11.46) |
| Cortex **CTXF** | 229.8 | ~116s | **1.45s** |
| xz -9 | 205.7 | 367.5s | 2.53s |
| zstd -19 | 224.4 | 675.6s | 2.23s |
| gzip -9 | 307.6 | 45.1s | 3.32s |
| bzip2 -9 | 242.2 | 69.1s | 32.99s |

### enwik8 (100MB) — CTXT ratio düzeltildi (2026-08-14 A/B ölçümü: 26.52MB gerçek, skill kaydı 25.29 drift etmiş)
| Codec | Ratio (MB) | Compress | Decompress |
|-------|-----------:|---------:|-----------:|
| Cortex **CTX8** | 24.88 | ~6.0s | ~1.9s |
| Cortex **CTXT-O1** | 26.52 | ~4.4s | ~1.5s |
| Cortex **CTXF** | 26.21 | 15.4s | **0.14s** |
| xz -9 | 23.71 | 71.14s | 0.26s |
| zstd -19 | 25.70 | 51.55s | 0.16s |
| bzip2 -9 | 27.66 | 6.65s | 3.30s |
| gzip -9 | 34.76 | 5.34s | 0.33s |

> CTXT enwik8 26.52MB üç commit'te A/B doğrulandı (55b5201 / b056e8a / HEAD = aynı 26,522,369 bayt) — pool optimizasyonu ratio'yu bozmadı. enwik9'da CTXT ratio %21.97 (219.7MB); oran blok sayısıyla iyileşir (6 blok → 60 blok).

**Özet (enwik9 skorlama: compress %25 / ratio %40 / decompress %35):**
1. **CTXF 0.872** 🥇 — en hızlı decompress (tüm rakiplerden; zstd 2.23s'ten bile hızlı), ratio gzip'ten çok iyi
2. xz 0.858 — ratio kralı
3. **CTX8 0.836** — CTX8 ≈ xz ratio'sunda, compress hızında 9× önde
4. **CTXT-O1 0.813** — **compress en hızlı (31s vs zstd 675s = 22×)**, ratio zstd'den iyi

---

## 7. Denenen Yaklaşımlar ve Ölçülmüş Sonuçlar (dead-end — TEKRAR KOŞMA)

### Decompress hızlandırma (ratio koruyarak)
- **u32 t_arr paketleme** (u64→u32): enwik9 ~%3, beklenen %30-40 DEĞİL → bottleneck bandwidth değil,
  **serial LF pointer-chase latency**.
- **thread-split 3/3→4/2** (entropy-ağırlıklı): 14.7→13.7s. Ratio sabit. ✅ Çalışan, 6 çekirdek tavanına yakın.
- **order-1 tANS** (256 ctx): 13.7→11.46s, ratio 219.7MB (+0.4 enwik8). ✅ Kabul edilen en büyük speedup.
- **Paralel inverse-BWT** (lane→thread): **ZARAR** 18.97→24.26s. 8-lane unrolled loop zaten MLP (load-store
  queue) doyuruyor; her thread tek pointer-chase olunca cache-latency'ye yenik.
- **Paralel t_arr build**: ~0 (hafif kötü). **Software prefetch** (write-prefetch ET0): ~%1, geri alındı.
- **5s decompress hedefi bu 6 çekirdekte ratio-sabit FİZİKSEL DEĞİL** (min ~10s; ~64s CPU / 6 çekirdek).

### Ratio girişimleri
- **MTF yüksek mertebe (order-3)**: 26.35→26.64% (kötüleşti), geri alındı.
- **LZ77 modu (CDXL)**: window 32KB enwik8 35.27MB, 8MB window 34.26MB **/0.44s açar, compress 5.57s**.
  Pencere 256× büyüyünce oran ~1MB iyileşti ama compress 6× yavaşladı. **LZ77 temelde BWT ratio'suna
  ulaşamaz** (gzip-sınıfı). "Aralığı doldurma" = iki AYRIK mimari, tuning yeri değil. `lz77.rs` tamamen silindi.

### Not: elindeki öneriler bu ölçüleri bilmiyorsa hayal üretir
Katışıksız "LZ+Range Coder = LZMA gibi 25MB" tahmini, cortex'te ölçülüp reddedilmiştir (34.26MB).
Bir modele cortex mimarisini verirken bu dosyayı ver; tahmin tablolarına güvenme.

---

## 8. Çekirdek Açık Problem

**BWT tabanlı arşivcide, ratio kaybetmeden decompress'i rakipler seviyesine (zstd ~2.2s) çekmek MÜMKÜN MÜ?**
Bilinen engel: inverse-BWT traversal serial bellek-gecikme zinciri; çekirdekle ölçeklenmez, SIMD'leşmez.

1. "Compress mod A / decompress mod B" mantıken nasıl kurulur? (Format encoder'ın modelini kodlar.)
2. **Modal / adaptive** — tek arşivde blok-başına mod (BWT+tANS bazı bloklar, LZ/zstd diğerleri; zstd/brotli/7zip
   gibi). Cortex formatına per-block mod bayrağı ile nasıl girer?
3. Ratio-sabit BWT decompress için gerçekçi kod/format kaldıraçları (test edilmiş dead-end'ler §7'de).

---

## 9. Kod Haritası

| Dosya | Satır | Rol |
|-------|------:|-----|
| `core/src/lib.rs` | 741 | mod seçimi, blok yapısı, compress/decompress pipeline, thread split, `HIST2D_POOL` |
| `core/src/mtf.rs` | 411 | `LANES=8`, BWT(divsufsort), MTF, RLE, inverse-BWT traversal |
| `core/src/tans.rs` | 448 | order-1 tANS, 256 ctx, `TABLE_BITS=11`, `TANS_POOL_ENCODE/DECODE` |
| `core/src/content.rs` | 180 | content-aware sınıflandırma + STORE (ContentKind, classify, magic, entropy) |
| `core/src/rangecoder.rs` | 241 | order-2 aritmetik (CTX8) |
| `core/src/cli.rs` | 78 | CLI argümanları |
| `core/src/main.rs` | 390 | çalıştırılabilir giriş |
| `core/src/crypto.rs` | 68 | AES-GCM |
| `core/src/filters.rs` | 43 | E8/E9 exec filter |
| `core/src/split_io.rs` | 154 | volume splitting |
| `core/src/tui.rs` | 162 | TUI |

Bağımlılıklar: serde_json, clap, divsufsort 2.0, rayon, aes-gcm, zstd 0.13, crossbeam-channel,
lz4_flex (atıl olabilir), num_cpus. Release: `opt-level=3, lto=true`.

---

## 10. Çalışma Ağacı Durumu (2026-08-14) — commitlenmemiş optimizasyon

`git status`'ta **commitlenmemiş** iki dosya değişikliği var (dice verelim, benchmark/commit sonra):

- **`HIST2D_POOL` thread_local** (lib.rs): order-1 tANS 2D histogramı (256×257×4 ≈ 263KB) her blokta
  tekrar allocate/atma yerine thread başına havuzdan yeniden kullanılıyor (`.fill(0)` ile sıfırlama).
- **`TANS_POOL_ENCODE/DECODE` thread_local** (tans.rs): order-1 tablolar (257 × ~2KB) her blokta rebuild
  yerine havuzdan; `tables: Vec<TansTable>` → `Box<[TansTable]>`, `build()` gömme + **boş ctx'ler (sum==0)
  tablo kurmaz**.

Bunlar byte-exact olmalı (hafıza düzeni optimizasyonu, çıktı değişmez) ama **henüz MD5 doğrulanmadı ve
commitlenmedi**. Benchmark/roundtrip onayı + `cargo test` sonrası commit edilecek. Yeni rapor bu yüzden
"~11.46s" notunu uncommitted olarak taşıyor.

---

## 11. Dokümantasyon Arşiv Haritası

`docs/` artık **tek yetkili kaynak** = yukarıdaki ARCHITECTURE.md. Eski raporlar içerik karşılıklarıyla
`docs/archive/2026-08-14/` altına alındı (git history'de + diskte korunur):

| Eski dosya | Kader | İçerik karşılığı |
|---|---|---|
| `BENCHMARKS_ESKI_gecersiz_8_core.md` | 🗑 **GEÇERSİZ** — eski 8-çekirdek makineden, yeni 6C ölçümleriyle çelişir | §6 (yeni ölçümler) |
| `decompress_hiz_analizi_karar_kaydi.md` | 📦 karar tarihçesi (1s fiziksel imkânsızlık + Strateji 1/2 gerekçesi) | §6-7 özeti |
| `enwik9_karsilastirma.md` | 📦 benchmark + skorlama | §6 |
| `SISTEM_DURUM_RAPORU.md` | 📦 önceki otorite rapor | §1-9 |
| `TASKS_enwik8.md` / `TASKS_bwt_cache.md` | `docs/` kökünde kalır — açık görev kartları | — |
