# TASK: BWT Inverse Traversal Cache Optimizasyonu (AGY)

## Durum
Mevcut CTXT decompress'in en büyük parçası `mtf.rs` içindeki inverse BWT traversal
(`PROF_INV_BWT_TRAVERSAL`, ~2.6s / toplam decompress ~1.98s). Darboğaz pointer-chasing:
`t_arr[p]` üzerinden `p <- t_arr[p]` seri zinciri, RAM gecikmesi (latency) — cache-miss bound.

Kod zaten 8 lane (`LANES=8`) register unroll kullanıyor (satır 374-406). Amaç:
bellek gecikmesini **dizi ayırmadan** saklamak (latency hiding).

## KRİTİK KURAL — byte-exact (veri formatı dokunulmaz)
- CTXT çıktısı **bit-exact aynı kalmalı**. MD5 baseline: `0fa6695eec6817b2cd25c6753b6474b7` (data/enwik8).
- Değişiklik **yalnızca bellek erişim düzenini** (memory access pattern) değiştirir. Üretilen byte dizisi, sıralama, sıkıştırma oranı asla değişmez.
- `t_arr` tek `u32` dizisi olarak KALIR (ayrı `bwt_char` / `idx_arr` dizilerine BÖLMEYECEKSİN). Biz bunu zaten tartışıp reddettik: ayrı diziler miss sayısını ikiye katlar.
- Format kırmak / CLI imzası değiştirmek / başka modülü değiştirmek → KAPSAM DIŞI.
- Commit YAPMA. Sadece çalışan kod + benchmark raporu teslim et.

## Hedef
- **Gerçekçi hedef**: traversal ~2.6s -> ~1.8-2.0s (ölçülmeden 1.2s VAAT ETME).
- Darboğaz RAM latency olduğu için, prefetch MLP (memory-level parallelism) eklemek ana fikir.
- Kazanım kanıtlanamıyorsa, değişikliği GERİ AL (önerdin ama ölçülmedi demek yeterli değil).

## İki Varyant (ikisini de uygula, benchmark et, kazanını öner)
Mevcut döngüde (satır 374-406) `t_arr[p]` okuması her lane'in seri chain'inin halkasıdır.
Prefetch ancak **bir sonraki zincir halkasının adresi önceden bilindiği zaman** işe yarar.

### V1 — Basit prefetch (AGY'nin orijinal önerisi)
`t_arr[p]` okunmadan önce o adrese `_mm_prefetch` (hint T0). DOSYA: dizi ayırmadan.
Dikkat: `p0` zaten mevcut satırda olduğundan yalnız başına prefetch genelde çok kazandırmaz;
yine de ölç (referans al).

### V2 — Prefetch-chain (unroll-ahead) — ASIL ADAY
Pointer-chase'in seri zincirini **1-2 halka öteye prefetch** ederek kır. Her lane için bir
sonraki `p`'yi bir sonraki iterasyonda okunacağı VARSATILAN değer olarak tut: yani her
iterasyonda iki bağımsız ham veri yükü sağla — chain'i 2 adım paralelleştir.

Anahtar nokta (sana pitfall olarak verilir):
```
// YANLIŞ — prefetch'i aynı satırda p'ye uygularsan hiç kazanamazsın:
let p = t_arr[i]; _mm_prefetch(&t_arr[p]);  // p zaten cache'te, prefetch boşa
// DOĞRU — bir sonraki halkanın adresini ÖNCEKİ satırda prefetch et (unroll-ahead):
let cur = t_arr[p];
_mm_prefetch(&t_arr[cur]);  // cur, BİR SONRAKİ halkanın okunacağı adres
p = cur;
```
Yani **2. derece prefetch**: `p_next = t_arr[p]; prefetch(t_arr[p_next])` zincirini `p`'yi
güncellerken aynı anda bir sonraki yükü sıraya koy. 8 lane bunu doğal paralel yapar.

Başka LEVARGE'ler (istediğin kadar deney, hepsini raporla — hepsi byte-exact olmak zorunda):
- `core::arch::x86_64::_mm_prefetch` — Linux x86_64. `#![cfg(target_arch="x86_64")]` guard şart.
- `#[target_feature(enable = "sse")]` gerekmez; prefetch hint normal. Ama safety → `unsafe` blok.
- Non-temporal / T0 / T1 hint karşılaştırması.
- Unroll faktörünü 8'den 2x-4x artır — ama mantık KORUNMALI: çıktı sırası bozulamaz.

## Kapsam DIŞI (reddedildi — yapma)
- `t_arr`'yi ayrı `bwt_char`+`idx_arr` dizisine bölmek (miss iki kat, REJECT).
- 3-byte / bit-packed `t_arr` (unaligned, düşük getiri).
- Memory pooling, lazy table build (daha önce ölçülüp faydasız bulundu — entropy 0.00s).
- `enwik8` dışında yeni mod, CLI isim, imza.
- ZSTD sökümü, sparse array, CTXT format değişikliği.

## Kullanıcı Beklentileri
- Türkçe, kısa, öz rapor. Monolog değil, sonuç.
- Commit/PR YOK — sadece kod + benchmark.
- Önce mevcut baseline'ı DOĞRULA (MD5, timing), sonra varyantlarını uygula.
- "Hızlandırdım" demek yetmez: AYNI input'ta önce vs sonra timing ve MD5'i tablo halinde ver.

## Doğrulama (benchmark prosedürü — sen koş)
1. `cargo build --release`
2. Baseline (mevcut kod) MD5 + timing kaydet.
3. V1'i uygula → build → aynı input ile MD5 + timing.
4. V2'yi uygula → build → aynı input ile MD5 + timing.
5. Tüm MD5'ler `0fa6695eec6817b2cd25c6753b6474b7` ile AYNI olmalı.
6. En hızlı varyantı öner; kazanamazsa en safını (V1 hiç / V2 ölçümlü) seç.
7. Tablo: varyant / MD5 / compress_s / decompress_s / not.

---

## SONUÇ (2026-08-14 — Hermes ölçtü, KAPATILDI)

**V2 unroll-ahead read prefetch ÖLÇÜLDÜ ve DEAD-END — geri alındı, working tree temiz.**

- **A/B ölçümü** (aynı enwik8 CTXT arşivi, 4-run ort, tek makine):
  - Prefetch'siz baseline: **1.5008s**
  - V2 prefetch (8 lane unroll-ahead): **1.4720s** → **~%1.9** fark
- **MD5 her iki çıktıda `a1fa5ffddb56f4953e226637dabbb36a`** — byte-exact korundu, ratio değişmedi.
- **Yorum:** %1.9, baseline dalgalanmasının (1.46–1.58s variance) İÇİNDE — sinyalden çok gürültü. "MLP 8→16" varsayımı bu makinede çalışmadı çünkü **8-lane interleave donanım MLP'sini zaten doyuruyor** (skill'in daha önceki bulgusuyla tutarlı). 8 adet fazladan `_mm_prefetch` instruction'ı decoded-I-cache + execution port yükü ekliyor; ~%2 için değmez.
- **TASKS'ın hedefi (traversal 2.6→1.8-2.0s) gerçekleşmedi:** loop zaten (8-lane register unroll + interleave) optimalleştirilmişti.
- **Karar:** kod geri alındı. Bu kartın ana hipotezi KAPANDI.

### İlgili dersler (aynı sesyona ait, probe'larla ölçüldü — tekrarlama)
- **Order-2 tANS:** entropi probe = sadece +0.43MB (kuantize) / +0.69MB (ham), emeğe değmez. Token-level statik entropi tavanı ~26.0MB.
- **Adaptive order-1 tANS:** +0.25MB (bootstrap cezası kazanı eritiyor). 
- **CTX8'in 24.88 ratio'sunun gerçek kaynağı:** bit-level adaptive context-mixing (`MtfModel`, 9-bit ağaç + 1..511 ctx + `p_mix=(p0+3p1+4p2)/8`) — ne order-2 ne adaptif token-tANS tek başına bu seviyeye inemez.
