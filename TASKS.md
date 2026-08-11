# Fleet görevi — Cortex roundtrip testlerini güçlendir

### core/tests/roundtrip.rs -> FCC-CLAUDE
`core/tests/roundtrip.rs` dosyasındaki roundtrip byte-exact testlerini güçlendir.
Amaç: BWT->MTF->RLE decode path'ini (özellikle pidx LF-mapping) uç durumlarda
doğrulamak. Mevcut 5 test yetersiz: en büyük veri 1000 byte, tekrar verisi tek
case, boş veri gerçekten test edilmiyor.

Yapılacaklar:
1. `test_roundtrip` helper'ındaki `if data.is_empty() { return; }` early-return'ü
   kaldır — boş veriyi GERÇEKTEN test et (test_roundtrip_empty boş girdiyle
   assert'e ulaşsın).
2. Edge case testleri ekle:
   - Tümü 0x00 (ör. 100 byte)
   - Tümü 0xFF (ör. 100 byte)
   - 0,1,2,...,255 ardışık artan
   - 255,254,...,0 azalan
   - Aynı byte'ın uzun tekrarı (ör. 10_000 x 0x41) — RLE patlaması
   - 0x00..0xFF tüm 256 farklı byte
3. Fuzz testi ekle: sabit seed'li deterministik RNG (rand crate zaten bağımlılıkta,
   StdRng::seed_from_u64 kullan). 5 ayrı seed sabitle; her seed için boyutlar:
   0, 1, 2, 3, 15, 64, 255, 256, 257, 1000, 4096, 64_000 civarı. Her boyutu
   gerçekten test et. Seed'ler ve boyutlar SABİT olacak ki test tekrarlanabilir
   olsun (fuzz "at random" değil, deterministik).

KURALLAR: SADECE core/tests/roundtrip.rs dosyasını yaz. Başka dosya OLUŞTURMA,
core/Cargo.toml / core/src/* dosyalarına DOKUNMA, commit yapma. Mevcut
`test_roundtrip` imzasını koru; yeni testler onu çağırsın. Mevcut 5 testi
SİLME veya imzalarını bozma (hepsi #![test] fonksiyonu). `use cortex::...`
importlarını gerekirse güncelle. Derlenebilir ve geçebilir olmalı.

Milestone: cargo test --manifest-path core/Cargo.toml --test roundtrip