# CORTEX — Faz 15: enwik8'de Boyut Düşürme Görevi

## Mevcut Durum (Doğrulanmış Rakamlar)
100 MB enwik8, 6 çekirdek, rayon paralel bloklar (16 MB):

| Yöntem | Boyut | Hız |
|---|---|---|
| Cortex (order 4) | 36.47 MB | ~8 MB/s |
| zstd -19 | ~26.9 MB | ~1.4 MB/s |
| brotli -q 11 | ~25.7 MB | ~0.48 MB/s |

**Gerçek durum:** Hızda zaten çok öndeyiz. SORUN BOYUT: 36.47 MB ile zstd'den 9.5 MB, brotliden 11 MB gerideyiz. "Brotli'yi geçtik" iddiası ANCAK boyut ≤ 25.0 MB olursa doğru olur. Önce zstd (-19, 26.9 MB), sonra brotli (25.7 MB) hedefi.

## HEDEF (Sıralı)
1. **Hedef A (öncelik):** enwik8 ≤ 26.5 MB, hız ≥ 4 MB/s → "zstd -19'u geç + 5x hızlı" iddiası kanıtlanır
2. **Hedef B (sonra):** enwik8 ≤ 25.0 MB, hız ≥ 3 MB/s → "brotli -q 11'i geç + 6x hızlı" iddiası kanıtlanır

İkisinden biri tutarsa görev başarılı. İkisi de tutmazsa: en iyi sonucu raporla, gerileme yapma.

## Yapılacaklar (Öncelik Sırası)

### 1. Match Model Derinleştirme (en kolay kazanç)
- `MATCH_HASH_LEN` 4 → 8 (context.rs'de). HTML/tekrar yoğun metinde uzun eşleşmeleri yakalar.
- Tek hash bucket yerine **2 aday** tut: hash tablosuna (pos, len) çifti yaz, adayı seçerken en uzun doğrulanmış eşleşmeyi al.
- `match_len > 12` için margin 15 → 8 deneyerek uzun eşleşmelerde özgüveni artır. Regresyon olursa geri al.

### 2. Order Derinliği 0-4 → 0-6 (dikkatli, hash collision!)
- MEVCUT SORUN: tüm order'lar aynı `HASH_MASK` (2^17) bucket alanını paylaşıyor — order 5-6 eklemek çakışmayı artırır.
- ÇÖZÜM: her order'a kendi bucket alanı. `ContextModel::new`'de `buckets = if order <= 4 { 1<<17 } else { 1 << (17 + (order - 4)) }` (order 5: 2^18, order 6: 2^19). `context_hash` buna göre mask'lesin.
- RAM HESABI: order 6 için 2^19 × 256 × 2 = 256 MB. 7 order toplam ~600 MB/blok. 6 çekirdek = ~3.6 GB. Makinede 15 GB var, sorun değil ama 16 MB bloklarla paralel çalışırken OOM olursa blok boyutunu 8 MB'a düşürme (bağlam kaybı!) — önce RAM'i ölç.
- `probs`/`bases` array boyutları: `MAX_MIXER_INPUTS` (16) yeterli (order 6 = 7 model + 2 = 9 input). DOKUNMA.

### 3. Context-Conditioned Mixer (PAQ'nun asıl gücü, en büyük kazanç)
- Şu an 8 mixer sabit ağırlıklı, context'ten bağımsız. PAQ'da mixer AĞIRLIKLARI context'e göre seçilir.
- BASİT SÜRÜM: 4 mixer seti (32 mixer) yap; `prev_byte & 3` (veya `prev_byte % 4`) ile set seç. Kodlama/decode'da aynı seçim — simetriyi bozma!
- `Mixer::new(n_inputs)` zaten var; `mixers: Vec<Mixer>` → `[Mixer; 32]` veya `Vec` + index hesabı. `MAX_MIXER_INPUTS` limitini aşma.
- Bu adım tek başına 1-2 MB kazandırabilir.

### 4. Doğrulama (HER ADIMDA ŞART)
```bash
cargo test --release                                   # roundtrip bit-exact
cargo build --release
./target/release/cortex compress data/enwik8 /tmp/crx_o6.crx -o 6   # Hedef ölçüm
ls -la /tmp/crx_o6.crx                                 # boyut raporla
```
- Her adım sonunda boyutu yaz: önceki boyut → yeni boyut, fark.
- Regresyon görürsen o değişikliği geri al, bir sonraki adıma geç.

## YASAK (Dokunma)
- `main.rs`: rayon, blok boyutu (16 MB), header formatı (CTX2) — DOKUNMA.
- `rangecoder.rs`: kodlayıcı mantığı bit-exact, DOKUNMA.
- Mixer update'ini `pm`'den `pf`'ye çevirme — ayrı görev, bu görevde değil.
- istek.md dosyasını silme/taşıma.

## Teslim
Görev bitince: `git diff --stat` + her adımın boyut tablosu + hangi hedefin tuttuğu (A/B/hiçbiri). Commit YAPMA — Hermes inceler, test eder ve commit eder.
