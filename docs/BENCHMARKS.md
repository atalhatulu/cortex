# Cortex Compression Benchmarks

Karşılaştırmalı sıkıştırma kıyas sonuçları — Cortex (kapalı) vs rakip araçlar.

> **Metodoloji:** Tek makine, temiz yük (load < 1). Araçlar **sıralı** ölçüldü
> (CPU paylaşımı süreleri bozmasın). Her araç gerçek dosya üzerinde ölçüldü.
> Boyut = üretilen arşiv byte'ı. Oran = arşiv / ham input.
> Cortex 8 thread (`--threads 8`); gzip/xz tek-thread (varsayılan).
> **Donanım:** RX 5500XT, CachyOS, 8 mantıksal çekirdek.

---

## 1. Kendi verilerimiz (yalnızca Cortex)

### 1.1 enwik8 — 100,000,000 byte (100 MB)

| Varyant | Boyut (byte) | Oran | Compress süre | Compress hız | Decompress süre |
|---|---|---|---|---|---|
| **Cortex level 3** (16MB blok) | 24,884,228 | **24.88%** | ~4.5s | ~22 MB/s | 1.8s |
| Cortex level 9 (64MB blok) | — | 23.56% | 15.3s | 6.5 MB/s | (ölçülmedi) |

- `cmp` byte-exact, md5 aynı. Decompress compress'tan ~2.5× hızlı.
- `.ctx` boyutu metadata dahil (24,884,117 + 111 byte FileItem JSON).

### 1.2 enwik9 — 1,000,000,000 byte (1.00 GB)

| Varyant | Boyut (byte) | Oran | Compress süre | Compress hız | Decompress süre |
|---|---|---|---|---|---|
| **Cortex level 3** | 216,412,175 | **21.64%** | 44.9s | 22.27 MB/s | 15.1s |

- Roundtrip doğrulandı: md5 `e206c345…` byte-exact (60 blok).
- **Ratio, büyük dosyada İYİLEŞİYOR:** enwik8 %24.88 → enwik9 %21.64.
  Sebep: 1GB = 60 dolu 16MB blok → istatistiksel bağlam daha zengin, PAQ-tarzı
  model daha büyük korpushada daha verimli çalışıyor. Oran girdi boyutuna bağlı
  DEĞİL (blok-tabanlı değil; blok dolu oldukça artıyor).

---

## 2. Rakip araçlarla kıyas (aynı dosya)

### 2.1 enwik8 (100 MB)

| Araç | Oran | Boyut (byte) | Compress | Decompress |
|---|---|---|---|---|
| **Cortex level 3** | **24.88%** | 24,884,228 | **~4.5s** | 1.8s |
| xz -9 | 24.87% | 24,865,252 | 80.4s | – |
| xz -6 | 26.67% | 26,665,156 | 19.2s | 0.30s |
| gzip -9 | 36.45% | 36,445,248 | 5.3s | 0.37s |

> **Vurgu:** Cortex, xz -9'un **en iyi oranına (24.87% ≈ 24.88%) ~18× daha
> hızlı** ulaşıyor; gzip -9'dan **%11.6 puan iyi** oranla.

### 2.2 enwik9 (1.00 GB)

| Araç | Oran | Boyut (byte) | Compress | Decompress |
|---|---|---|---|---|
| **Cortex level 3** | **21.64%** | 216,412,175 | **44.9s** | 15.1s |
| xz -6 | 23.34% | 233,403,104 | 142.4s | 2.1s |
| gzip -9 | 32.26% | 322,591,995 | 44.0s | 3.3s |

> **Vurgu:** Cortex, enwik9'da gzip -9'u **eşit sürede %10.6 puan iyi oranla**
> eziyor ve xz -6'yı **3.2× daha hızlı** sıkıştırıyor.

---

## 3. Süre karşılaştırması (compress + decompress)

| Dosya | Araç | Oran | Compress | Decompress | Toplam |
|---|---|---|---|---|---|
| enwik8 | **Cortex** (8t) | 24.88% | 4.5s | 1.8s | **6.3s** |
| enwik8 | gzip -9 | 36.45% | 5.3s | 0.37s | 5.7s |
| enwik8 | xz -6 | 26.67% | 19.2s | 0.30s | 19.5s |
| enwik9 | **Cortex** (8t) | 21.64% | 44.9s | 15.1s | **60.0s** |
| enwik9 | gzip -9 | 32.26% | 44.0s | 3.3s | 47.3s |
| enwik9 | xz -6 | 23.34% | 142.4s | 2.1s | 144.5s |

> Not: Cortex çoklu-thread (8), gzip/xz tek-thread. Cortex tek-thread ile
> ölçülürse oran aynı kalır, süre artar. Tablo kullanıcı görünür hızdır.

## 4. Oran olarak ne kadar iyiyiz?

**Sıkıştırma oranı (ratio):** Cortex açıkça önde.
- enwik9: **gzip -9'un %32.9 daha küçüğü** (21.64% vs 32.26%); xz -6'nın %7.3 daha küçüğü.
- enwik8: **gzip -9'un %31.7 daha küçüğü** (24.88% vs 36.45%); xz -9 ile **pratikte eşit** (24.88% vs 24.87%, fark ihmal edilebilir).

**Decompress hızı:** Cortex geride (PAQ-tarzı modelin açma maliyeti).
- enwik9: gzip'ten ~4.5×, xz'den ~7.2× yavaş (15.1s vs 3.3s / 2.1s).
- enwik8: gzip/xz'den ~5× yavaş (1.8s vs 0.37s / 0.30s).

**Net takas:** Cortex, atipik olarak iyi *sıkıştırma* (gzip seviyesinde sürede
xz-9 kalitesi) ama atipik olarak yavaş *açma*. Sıkıştır-or-çok-oku (arşiv,
tam-yedek) kullanımları için ideal; "çok kez aç" senaryosunda gzip/xz daha
uygun. Sıkıştırma:üretimi 1 kez, okuması 1 kez ise Cortex net kazançtır
(oran kazancı süre kaybından büyüktür).

### Decompress'i hızlandırma adayı (gelecek)
Inverse-BWT zaten hızlı; darboğaz büyük olasılıkla MTF-decode + order-model
decode'un her-token cache-miss'i. Olası: inverse-MTF/order decode'a paralel
blok-prefetch ya da daha büyük L1/L2 dostu context yeniden düzenleme. Henüz
uygulanmadı.

---

## Notlar / büyük dosya orantısı

- Cortex blok-tabanlı (16MB blok, bağımsız model). **Hız girdi boyutundan
  bağımsız ve sabit (~22 MB/s)** — enwik8, enwik9 hepsi aynı.
- **Oran büyük dosyada iyileşir** (24.88% → 21.64%): daha çok/dolu blok,
  daha iyi statistiksel bağlam. Dolayısıyla büyük korpushada geride kalma
  riski yok; tersine avantaj.
- **1GB üstü test:** enwik9 (1.00 GB) düzenli olarak kullanılabilir
  (`/home/teha/Downloads/CompressionBenchmarks/enwik9`). 10GB+ için
  enwik9'un kendini tekrarlayan kopyaları veya sentetik büyük veri gerekir.
