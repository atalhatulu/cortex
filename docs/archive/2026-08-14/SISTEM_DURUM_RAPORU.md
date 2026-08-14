# Cortex — Sistem / Algoritma / Problem Durum Raporu (nesnel, 2026-08-13)

Bu rapor, kod tabanındaki gerçeklere dayanır. Aİ modellere soracaksan bu dosyayı olduğu gibi verebilirsin.
Rapor tarafsızdır; kendi kararını/yorumunu içermez, sadece mevcut durumu tarif eder.

---

## 1. Proje Tanımı

- **Repo:** `/home/teha/Documents/GitHub/cortex` — sıfırdan yazılmış **Rust** kayıpsız sıkıştırıcı / arşivci.
- **Hedef:** zip/RAR tarzı "tam arşivci" olmak. Bir "standart" (varsayılan) dengeli mod + max-ratio + max-hız modları.
- **Kod:** ~3.000 satır Rust (core/src). Girdi: CLI + Tauri masaüstü GUI (ui/). Çok çekirdekli (rayon) blok-paralel.
- **Ölçüm donanımı:** 6 fiziksel çekirdek, SMT yok (AMD Ryzen 5 3500), L2 3MB, L3 16MB (2×8MB).

---

## 2. Mimari — Üç Ayrı Format (Mod)

Her mod kendine özgü 4-bayt sihirli imza ile başlar. "Mod" = üretim anında seçilen, tamamen farklı algoritma zinciri.

| Mod | Sihir | Algoritma zinciri | Karakter |
|-----|-------|-------------------|----------|
| **CTX8** | `b"CTX8"` | BWT → MTF → RLE → **order-2 aritmetik (range coder)** | Max ratio |
| **CTXT** | `b"CTXT"` | BWT → MTF → RLE → **order-1 tANS** entropy | Dengeli (varsayılan aday) |
| **CTXF** | `b"CTXF"` | **zstd** (hazır kütüphane, `-l` seviye) | Max hız |
| **CDXL** | `b"CDXL"` | **LZ77** → literaller order-1 tANS + zstd meta | (ölü deneme, ratio kötü) |

Her mod kendi decoder'ını gerektirir; imzaya bakılarak seçilir. **Decompress, arşivin üretildiği modla yapılır** — modlar birbirini açamaz.

---

## 3. Sıkıştırma Pipeline'ı (paralel)

Girdi eşit boyutta **bloklara** bölünür (`block_size_for_level`: level1=1MB, level3=16MB varsayılan).
Bloklar rayon havuzunda paralel işlenir, sonuçlar **zincirleme iç içe geçmiş başka bir blok yapısıyla** yazılır.
Her bloğun önüne `u32` bayt uzunluğu konur. Batch büyüklüğü bellek bütçesine göre ayarlanır (RSS taşmasın).

**CTX8 / CTXT ortak BWT iskeleti (`mtf::bwt_mtf_rle`):**
1. `divsufsort::sort` ile suffix array (SA) inşa edilir (BWT kalbi — ratio'nun kaynağı).
2. BWT dönüşümü. Blok **8 lane'a** bölünür (`mtf::LANES = 8`), her lane'in BWT içindeki başlangıç pozisyonu `pidx[8]`'e yazılır.
   - Lane bölme, decompress'te inverse-BWT'yi paralelize etmek içindir (her lane bağımsız çözülebilir).
3. **MTF** (move-to-front): her sembol listedeki indeksine dönüştürülür; `0` sembolü listede en önde → uzun sıfır koşuları oluşur.
4. **RLE**: uzun sıfır koşuları binary (bit base-2) kodlanır, geri kalan indeksler `u16` token olarak kalır.

**Entropy aşaması (moda göre değişir):**
- **CTX8:** `MtfModel` (order-2 bağlam modeli) + `rangecoder` (LZMA tarzı carryless aritmetik, 32-bit range, PROB_BITS=12). Uyarlanabilir olasılıklar.
- **CTXT:** token histogramı alınır; **order-1** 2D histogram (256 ctx × 257 sembol) çıkarılır; her ctx için tANS tablosu kurulur; token akışı tANS ile kodlanır. 2D histogramın kendisi (~263KB) açıkta kalmaması için zstd ile sıkıştırılıp arşive gömülür.
- **CTXF:** doğrudan `zstd::encode_all(chunk, level)`.

**Opsiyonel ön-işlem:** çalıştırılabilir (executable) tespit edilirse E8/E9 filter uygulanır (x86 call/jump adreslerini göreli yapar → BWT/entropi daha iyi çalışır).

---

## 4. Decompress Pipeline'ı (iki ayrı rayon havuzu + kanal)

Decompress iki aşamaya ayrılmış, **iki ayrı rayon havuzu** + bounded `crossbeam-channel` ile **örtüşmeli** çalışır:
- **Stage1 (entropy çözme):** memory-mapped girdiden blokları oku, imzaya göre düşer (zstd decode / tANS decode+hist / range coder decode). `DecompressStage1::Normal{pidx,tokens,is_exec,size}` veya `Fast(block)` üretir. Bloklar kanala gönderilir.
- **Stage2 (ters dönüşüm):** kanaldan `par_bridge` ile tüketir. `Normal` için **inverse-BWT** (8 lane çözülür) → RLE çöz → MTF ters çevir → E8/E9 (gerekirse) uygula. **yazıcı thread** iç içe geçmiş blokları `BTreeMap` ile global sıraya koyup orijinal sırayla dosyaya yazar.
- **Thread dağılımı:** `total_threads=6`, `stage1 = (6*2)/3 = 4`, `stage2 = 2` (Stage1 entropy CPU-ağırlıklı). `channel_cap = max(24, threads*3)`.
- İç içe geçmiş/dağınık yazımı doğru sıraya döndürmek için `global_chunk_idx` + BTreeMap.

---

## 5. Ölçülmüş Sonuçlar (byte-exact, 3-run ort, idle)

**enwik8 (100MB) — 6 çekirdek:**

| Mod | Ratio | Decompress | Kompresyon |
|-----|-------|-----------|-----------|
| CTX8 | 24.88MB | ~1.9s | — |
| CTXT-O1 | 25.29MB | ~1.27s | — |
| CTXF (-19) | 26.21MB | ~0.14s | — |

**enwik9 (1GB) — tablo (benchmark sırasında load varken ölçüldü; bağıl sıralama geçerli):**

| Codec | Ratio (MB) | Compress | Decompress |
|-------|-----------|----------|-----------|
| Cortex CTX8 | 206.4 | ~38s | 15.75s |
| Cortex CTXT-O1 | 219.7 | ~31s | 13.37s |
| Cortex CTXF | 229.8 | ~116s | **1.45s** |
| xz -9 | 205.7 | 367.5s | 2.53s |
| zstd -19 | 224.4 | 675.6s | 2.23s |
| gzip -9 | 307.6 | 45.1s | 3.32s |
| bzip2 -9 | 242.2 | 69.1s | 32.99s |

**CTXT için daha güncel (idle) rakamlar:** enwik9 decompress ~11.46s, ratio 219.7MB; order-1 tANS geldikten sonra.

---

## 6. Gözlemlenen Temel Ölçeklenme / Başarı/Engel Noktaları

- **ratio tarafı iyi:** CTX8 ≈ xz ratio'da, CTXT zstd'den iyi. **Kompresyon hızı büyük üstünlük:** CTXT 31s vs zstd 675s vs xz 367s (1GB).
- **CTXF decompress (1.45s) tüm rakipleri geçiyor** (zstd 2.23s, xz 2.53s, gzip 3.32s, bzip2 33s).
- **Tek zayıf nokta:** BWT modlarının (CTX8 ~15.7s, CTXT ~11.5s) decompress'i. Rakipler (xz 2.5s, zstd 2.2s, gzip 3.3s) burada önde.
- Decompress CPU profili (enwik9, CTXT): entropy ~%60, inverse-BWT traversal ~%25-37, kalan MTF+RLE+t_arr build.

---

## 7. Denenen Yaklaşımlar ve Ölçülmüş Sonuçlar (öğrenilmiş dersler)

**Decompress hızlandırma (ratio koruyarak):**
- `u32 t_arr` paketleme (u64→u32, 24-bit indeks): enwik9 13.7→13.31s (~%3), beklenen %30-40 DEĞİL. → traversal bottleneck: memory-bandwidth değil, **serial LF pointer-chase latency**.
- `thread-split 3/3 → 4/2` (entropy ağırlıklı): enwik9 14.7→13.7s. Ratio sabit. Çalışır; 6 çekirdekte CPU tavanına yakın.
- `order-1 tANS` (256 ctx): enwik9 13.7→11.46s, ratio 219.7MB (+0.4MB enwik8). **Şu ana kadar kabul edilen en büyük speedup.** (order-0 tANS daha az kazanmıştı.)
- Paralel inverse-BWT (lane→thread): **ZARAR VERDİ** 18.97→24.26s. 8-lane unrolled loop zaten load-store queue'yu MLP için doyuruyor; her thread tek pointer-chase olunca cache-latency'ye yenik düştü.
- Paralel t_arr build: ~0 kazanç (11.46→11.545s, hafif kötü). Cortex zaten 60 blokta thread pool'u %100 doyuruyor.
- Software write-prefetch (traversal): ~%1 kazanç, wall-time değişmedi. Geri alındı.

**Ratio girişimleri:**
- MTF yüksek mertebe (order-3) context: 26.35→26.64% (kötüleşti), geri alındı.
- LZ77 modu (CDXL): window 32KB / 8MB denendi. LZ77 temelde BWT ratio'suna ulaşamıyor (enwik8 34-35MB vs CTXT 25.29). "Aralığı doldurma" yaklaşımı başarısız — iki mimari değil, ayrık.

**Blok boyutu / LANES denemeleri (sadece marjinal kazanç beklenen, denenmemiş):**
- Blok küçültme her 4×'te ~1.8-2MB ratio kaybettiriyor.
- LANES artırma: denendi/yayınlanmadı.

---

## 8. Mevcut Açık Problem / Sorulacak Soru

**Çekirdek problem:** BWT tabanlı bir arşivci ile, **ratio'yu kaybetmeden** decompress hızını rakiplerin (zstd ~2.2s, xz ~2.5s) seviyesine çekmek MÜMKÜN MÜ?
Bilinen engel: inverse-BWT traversal, serial bellek-gecikme zinciri; çekirdek sayısıyla ölçeklenmez, SIMD ile vektörleşmez.

İstenen mimari / karar soruları:
1. "Compress'te mod A, decompress'te mod B kullan" — **tek arşivde iki farklı decoder** kavramı mantıken nasıl kurulur? (Format fizik olarak encoder'ın modelini gerektirir; "aynı bi akışı farklı decoder" = aynı formatın farklı *uygulaması*, farklı format değil.)
2. **Modal / adaptive (blok-başına mod seçimi)** — tek dosyanın içinde bazı blokları BWT+tANS, bazılarını LZ/zstd yapmak mümkün ve yaygın (zstd, brotli, 7-Zip). Bu Cortex formatına nasıl entegre edilir, dekoder blok başlığından ilgili modu okuyup çalıştırır.
3. Ratio korunarak BWT decompress'i hızlandırmak için gerçekçi kod/format seviyesi kaldıraçları neler? (Test edilmiş dead-end'ler yukarıda; paralel inverse-BWT, prefetch, bandwidth packing işe yaramadı.)

---

## 9. Kod Haritası (soru sorarken referans)

- `core/src/lib.rs` (850 satır): mod seçimi, blok yapısı, compress/decompress pipeline, thread split.
- `core/src/mtf.rs`: `LANES=8`, BWT (divsufsort), MTF, RLE, inverse-BWT traversal.
- `core/src/rangecoder.rs`: order-2 aritmetik (CTX8).
- `core/src/tans.rs`: order-1 tANS, 256 ctx tablo, `TABLE_BITS=11` (2048 giriş).
- `core/src/lz77.rs`: LZ77 denemesi (CDXL, ölü).
- `core/src/filters.rs`: E8/E9 exec filter; `crypto.rs`: AES-GCM şifreleme; `split_io.rs`: volume splitting.

*Bağımlılıklar:* serde_json, clap, divsufsort 2.0, rayon, aes-gcm, zstd 0.13, crossbeam-channel, lz4_flex (kullanılmıyor olabilir), num_cpus.
*Release profile:* opt-level=3, lto=true.
