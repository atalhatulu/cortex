# Fleet görevi — Cortex: `test` komutu + ratio iyileştirme

İki bağımsız görev, FARKLI dosyalar (uyumlu, PARALEL). Milestone'lar nesneldir.

---

### core/src/cli.rs + core/src/main.rs -> FCC-CLAUDE

Yeni `test` alt komutu: bir dosyayı sıkıştırıp geri açarak roundtrip'i
byte-exact doğrulama modu. Amaç hızlı bütünlük kontrolü.

API SÖZLEŞMESİ (BİREBİR):
- Komut: `cortex test <input>`
- `core/src/cli.rs`: `Commands::Test { input: String }` varyantı ekle
  (clap derive; açıklama: "Verify a file survives a compress/decompress roundtrip").
- `core/src/main.rs` `Commands::Test` kolunda:
  - Geçici dizin kullan (std::env::temp_dir + process id + rastgele ek).
  - `compress_file(input, tmp.crx)` sonra `decompress_file(tmp.crx, tmp.out)` çağır.
  - İki dosyayı `fs::read` ile karşılaştır (akıllıca: boyut farkıysa erken fail).
  - Başarı: `PASS: <input> roundtripped byte-exact (<n> bytes)` (exit 0).
  - Fark varsa: `FAIL: <input> differs after roundtrip` + `cmp` benzeri ilk fark
    ofsetini ve exit code 1 döndür.
  - Geçici dosyaları HER DURUMDA temizle (ok, fail, hata).
- Mevcut `Info`/`Compress`/`Decompress` kollarına dokunma; `Info` koluna yeni
  alan EKLEME.

KURALLAR: Yalnızca `core/src/cli.rs` ve `core/src/main.rs`. commit yapma.
Clap derive mevcut varyantlarını bozma. Derlenebilir, mevcut testler yeşil.

Milestone: cargo build --manifest-path core/Cargo.toml

---

### core/src/mtf.rs -> AGY

Amaç: sıkıştırma oranını (ratio) iyileştir + geliştirme görüşü raporla.
`MtfModel`'deki bağlam karma modelini (order1 + order2) geliştir. Şu an karma
SABİT: `p_mix = (p1 + p2) >> 1`. Ratio kazancı için karışım AĞIRLIĞINI
adaptif yap (ör. order-1 güvenine göre ağırlık, yumuşak geçiş), VEYA order-3
bağlam ekle — ama karıştırıcının encode/decode tarafı AYNI olmalı
(roundtrip byte-exact garantisi). Kod kalitesi + doğruluk öncelikli; ratio
kazancı istendi ama davranış DEĞİŞİMİ encode ve decode'da simetrik olmalı.

ÖNEMLİ KONTRAK: `encode_tokens` ile `decode_tokens` AYNI context karıştırma
formülünü üretmeli — decode tarafında context / hash hesabı encode ile
BİREBİR aynı kalmalı. Yoksa roundtrip bozulur. Roundtrip descendency:
`cargo test --manifest-path core/Cargo.toml --test roundtrip` 12 test YEŞİL
kalmalı (0.02 ile %2'lik MTF değişimi zaten var, onu bozma).

Nesnel hedef (mümkünse): mevcut `data/enwik8` (100MB değil, küçük bir
örnekle test et — tam 100MB koşma, süre alır) üzerinde ORAN düşmeli
(daha iyi). Ama kesin kriter roundtrip'teki 12 test + mevcut unit testler.

GÖRÜŞ RAPORU (JSON içinde `ideas` alanı): Cortex'in ratio ve hızını en çok
artıracak 3-5 gerçekçi fikri sırala (ör. context modeli, RLE, BWT taraması,
full-file mode). Her birinin tahmini etki + riskini yaz.

KURALLAR: SADECE `core/src/mtf.rs`. Diğer dosyalara dokunma, commit yapma.
`bwt_mtf_rle` / `decode_rle_mtf_bwt` imzalarını AYNEN koru. Mevcut unit
testleri kırma. Derlenebilir olmalı.

Milestone: cargo test --manifest-path core/Cargo.toml --test roundtrip