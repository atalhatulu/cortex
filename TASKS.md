# Fleet görevi — Cortex: `test` komutu + ratio iyileştirme

İki bağımsız görev, FARKLI dosyalar (uyumlu, PARALEL). Milestone'lar nesneldir.

---

## GÖREV A — `test` komutu: ✅ TAMAMLANDI (zaten implemente edilmişti)

`cortex test <input>` komutu `core/src/cli.rs` + `core/src/main.rs` içinde **zaten tam ve spec'e uygun** olarak mevcuttu.

- `Commands::Test { input: String }` — clap derive, açıklama: "Verify a file survives a compress/decompress roundtrip"
- Geçici dizin: `std::env::temp_dir() + pid + nanos` soneki
- `compress_file` → `decompress_file` → `fs::read` ile karşılaştırma
- Boyut farkı = erken fail; ilk fark ofseti raporlanır
- Her durumda (PASS/FAIL/error) geçici dosyalar temizlenir
- Çalıştığı doğrulandı: `PASS: <input> roundtripped byte-exact (<n> bytes)`

**Sonuç:** Kod değişikliği gerekmedi. AGY + Hermes doğrulaması yapıldı.

---

## GÖREV B — ratio iyileştirme: ⚠️ DENENDİ, KAZANÇ YOK — BASELINE'A DÖNÜLDÜ

### Denenen: order-3 bağlam ekleme (AGY, 2 tur)

**Tur 1:** `MtfModel`'e `order3` eklendi; hash `((hash2*257) ^ prev3) & 0x1FFF`, ağırlıklar `O0=1,O1=3,O2=4,O3=8>>4`.
- Ratio 5MB enwik8: **26.35% → 26.52%** (kötüleşti, -8.6KB)

**Tur 2:** Bağımsız hash `((prev1*92821) ^ (prev2*6899) ^ prev3) & 0x3FFF`, vektör 16384*512, ağırlıklar `O0=1,O1=3,O2=8,O3=4>>4`.
- Ratio 5MB enwik8: **26.35% → 26.64%** (daha da kötüleşti)

**Analiz:** order-3 MTF-token bağlamında fayda vermiyor; order2'nin üzerine gürültü + cache-miss hız düşüşü (8.20→6.26 MB/s).

**Karar:** order-3 **geri alındı** (`git checkout -- core/src/mtf.rs`). Baseline ratio restore edildi: **26.35% @ 8.20 MB/s**. Roundtrip 12+2 test her aşamada yeşildi — davranış simetrisi korundu, sadece ratio kazancı yok.

### Sonraki adaylar (AGY görüş raporundan → `~/Desktop/cortex_gorus_raporu.md`):
1. **Adaptif Context Mixing / SSE** — sabit ağırlıklar yerine her order'ın son tahmin hatasına göre dinamik ağırlık. En yüksek ratio potansiyeli, orta-zor.
2. **RLE iyileştirmesi** — BWT zero-run tokenlarına daha verimli kod. Orta ratio + orta risk.
3. **Blok-boyutlu divsufsort / hız** — ratio değil hız odaklı.
4. **SIMD inverse BWT** — hız odaklı, ratio etkisiz.

---

## Milestonelar

- [x] `cargo build --manifest-path core/Cargo.toml` — temiz derleniyor
- [x] `cargo test --manifest-path core/Cargo.toml --test roundtrip` — 12 test YEŞİL
- [x] `cargo test --lib` — 2 test YEŞİL
- [x] 5MB enwik8 benchmark 26.35% (baseline)
- [ ] Ratio iyileştirme (adaptif mix / RLE) — henüz kazanç bulunamadı
