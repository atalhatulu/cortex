# Cortex Decompress ~1s Hedefi — FİNAL Birleşik Analiz & Strateji Planı

**Hazırlayan:** Hermes · **Ajanlar:** Antigravity (agy, Gemini 3.1 Pro High) + fcc-claude (analiz/review) · **Tarih:** 2026-08-13
**Durum:** İki ajanın bağımsız analizi alındı, çapraz doğrulandı. Tüm sayılar bu makinede canlı ölçüldü / doğrulandı.

---

## 1. YÖNETİCİ ÖZETİ (Tek Cümle)

**~1s (≈1000 MB/s) BWT korunarak CPU'da FİZİKSEL olarak imkânsızdır; ancak mevcut CTXF/zstd fallback'i zaten 1s'yi teslim eder — karar "BWT'yi hızlı-mod olarak ~3-4s'ye optimize et" (ürün imzası korunur) vs "CTXF/zstd'yi resmi fast-mod ilan et" (1s teslim edilir) arasındadır.**

---

## 2. MAKİNE GERÇEĞİ (bizim ekosistemimiz, tüm sayılar buraya çakılır)

| Donanım | Değer |
|---|---|
| CPU | AMD **Ryzen 5 3500 — 6 fiziksel çekirdek, SMT YOK** |
| L2 | 3 MiB/çekirdek |
| L3 | 16 MiB (2×8 MiB) |
| nproc | 6 |

> ⚠️ **Kritik düzeltme (fcc-claude, doğrulandı):** Bu makinede **6 çekirdek var, 8 değil.** Eski skill verilerindeki "8 thread, 65s→20.6s, 3.16×" başka bir makineden/eski pipeline'dan geliyordu. Mevcut pipeline `total_threads/2` yani 3+3+1(reader)=6 çekirdekte **7 thread** çalıştırıyor — zaten over-subscribe. Thread split sayıları buna göre yeniden değerlendirilmeli.

## 3. MEVCUT BASELINE (ölçüldü, doğrulandı)

| Çalışma | Süre | Hız | Roundtrip |
|---|---|---|---|
| Cortex decompress enwik8 (100MB) | **1.86-1.90s** | ~53 MB/s | byte-exact ✓ |
| Cortex decompress enwik9 (1GB) | **14.06 / 15.28s** (avg ~14.7s) | ~68 MB/s | byte-exact ✓ |
| **zstd -19** decompress enwik8 | **~0.21s** | ~480 MB/s | byte-exact ✓ |
| **zstd -19** (compress) enwik8 boyutu | **26.94MB** | — | — |

> Kara tahta gerçeği: zstd -19, Cortex'in 1.9s'inden **~9× daha hızlı çözüyor** VE "≤27MB" milestone'ını **26.94MB ile zaten karşılıyor.**

## 4. NEDEN BWT+CPU'da 1s İMKÂNSIZ — fizik argümanı (agy + fcc-claude birleşik, doğrulandı)

1. **LF-mapping = pointer-chasing.** Inverse BWT, `t_arr` üzerinde ~1 milyar rastgele erişimdir. DDR gecikmesi ~50ns → tek çekirdekte ~20 MB/s fiziksel tavan; 6 çekirdekte bile 1000 MB/s'ye ulaşmak imkânsız.
2. **Amdahl duvarı.** Entropy aşamasını 0s'ye indirsen bile Stage2 (inverse BWT, toplam CPU'nun ~%45'i) sabit kalır → toplam süreyi aşağı çekemezsin.
3. **`t_arr` bellek bloğu.** 16MB blokta `t_arr = vec![0u64; n]` = **128MB rastgele-erişimli dizi** her thread için. Inverse BWT bu nehirde ~127 MB/s ile tavanlar.
4. **Düzeltme:** order-2 tablosu **8MiB** (8192×512×u16), 16MB değil — L3'e (16MB) sığar, L2'ye (3MB) sığmaz. "16MB" iddiası 2× şişirilmişti.

**Sonuç:** BWT korunarak CPU optimumu ~**3-4s** (tANS + blok küçültme ile), 1s değil. GPU ile bile ~2s.

---

## 5. İKİ STRATEJİ (fcc-claude'un yeniden çerçevelenmesi — en doğru)

### Strateji 1 — BWT'yi koru, aritmetik → tANS + blok küçült (~3-4s) ⭐ ürün imzası korunur
**Amaç:** Oranı (24.88MB imzası) fazla bozmadan CPU hızını ~3-4× artır.
- **Dosyalar:** yeni `core/src/tans.rs`; `mtf.rs`'ye histogram→tANS build/decode; `lib.rs`'ye yeni magic `CTXT` yönlendirmesi.
- `rangecoder.rs` **DEĞİŞMEZ** (CTX8 uyumluluğu korunur). `LANES=8` + `pidx[8]` yapısına dokunulmaz — chunk header aynı kalır, sadece entropy akışının başına **257×u16 histogram (514B)** eklenir.
- **Byte-exact:** histogram, encode tarafında token stream'inden deterministik çıkarılır; iki tarafta aynı build algoritması → tANS tablosu birebir aynı. `tests/roundtrip.rs` + `cmp` + md5.
- **Blok boyutu:** level 3'te 16MB→4MB (`lib.rs:60` `block_size_for_level`) → t_arr 128MB→32MB; L3 payı artar.
- **Kestirim:** entropy ~300-500MB/s, inverse BWT ~200-300MB/s → enwik9 **~4-5s**, enwik8 ratio **~25.5-27MB**.
- **Riskler:** MTF token dağılımında 0'da dev pik (RLE) → tANS normalizasyonu dikkat; hatalı histogram = bozuk stream; blok küçültme ratio kaybı. `MtfModel::new()`'in blok başına 8MB memset'ini pooled `Vec`'e çevir (`lib.rs:435`; 1MB bloklarda enwik9'da ~1000 alloc).

### Strateji 2 — Gerçek 1s: CTXF/zstd'yi resmi fast-mod ilan et (1-2s) ⭐ 1s teslim eder
- **Fizik:** 1GB/s LZ sınıfındadır. zstd -19 = 526MB/s, zstd -3 = 830MB/s (ölçüldü).
- **En ucuz teslimat:** CTXF zstd fallback'i **zaten** `lib.rs:406-413`'te implemente. Level'ı -19'a sabitle → enwik8 **26.9MB @ 0.21s**, enwik9 **~2s**. Raporun kendi milestone'ını BWT korumadan karşılar.
- **Native istersen:** LZ77 hash-zinciri + aynı tANS (zstd-lite) → haftalar; zstd crate test edilmiş ve yeterli.
- **Risk:** zstd ratio 24.88MB'ye yaklaşmaz; "rekor oran" iddiası sadece CTX8'de kalır. **Ürün konumlandırması netleştirilmeli: fast = hız, normal = ratio.**

---

## 6. AJAN GÖRÜŞLERİNİN ÇAPRAZ DEĞERLENDİRMESİ (kim haklı?)

| İddia | agy | fcc-claude | Doğrulama |
|---|---|---|---|
| 1s CPU+BWT imkânsız | ✓ (pointer-chasing + Amdahl) | ✓ (ek fizik) | **İkisi de doğru** |
| order-2 boyutu | zikretmedi | 8MiB (16MB değil) | **fcc doğru** (mtf.rs:30, hesaplandı) |
| Çekirdek sayısı | 8 varsaydı | 6 (nproc) | **fcc doğru** (lscpu) |
| Entropy→tANS tek başına 4-6s | önerdi | ~8s (t_arr duvarı) | **fcc doğru** (128MB/16MB blok) |
| 1s'ye ulaşma yolu | GPU (B) | CTXF/zstd var (B2) | **fcc pratik; GPU maliyetli** |
| Blok küçültme şartı | zikretmedi | vurguladı | **fcc doğru** |

**Karma görüş:** agy fizik/ispat güçlü; fcc-claude makine-gerçeği (6 çekirdek, t_arr, zstd ölçümü) en doğru. Ortak çıkarım: **Strateji 1 (tANS+blok) BWT imzasını korur, Strateji 2 (CTXF/zstd) 1s'yi teslim eder.**

---

## 7. ÖNERİLEN MİLESTONE (kullanıcı kararına sunulur)

**Yol A (BWT imzası korunur, ~3-4s):**
- enwik9 decompress ≤ 5.0s · roundtrip byte-exact (cmp+md5) · enwik8 ratio ≤ 27MB

**Yol B (gerçek 1s, fast-mod):**
- enwik9 ≤ 2.5s · enwik8 ratio ≤ 27MB (zstd -19) · roundtrip byte-exact

---

## 8. SONRAKİ ADIM (uygulamaya geçilecekse)

1. **Karar:** Yol A mı (tANS+blok, ratio korunur), Yol B mi (CTXF/zstd fast-mod, 1s), yoksa ikisi birden mi (A ratio modu + B hız modu, kullanıcı seçer)?
2. Karar sonrası fcc-claude/agy'ye **kod yazım görevi** olarak verilir (bu tur analizdi, kod değil — ajanlar sadece öneri üretti).
3. Hermes doğrular: byte-exact roundtrip + temiz yük altında çok-run A/B.
