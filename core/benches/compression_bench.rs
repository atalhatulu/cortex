use cortex::mtf::{bwt_mtf_rle, decode_rle_mtf_bwt, MtfModel};
use cortex::rangecoder::{Decoder, Encoder};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_compression(c: &mut Criterion) {
    let data = vec![b'A'; 10000];

    c.bench_function("bwt_mtf_rle_encode", |b| {
        b.iter(|| {
            let (pidx, tokens) = bwt_mtf_rle(black_box(&data));
            let mut enc = Encoder::new();
            let mut model = MtfModel::new();
            model.encode_tokens(&mut enc, &tokens);
            let _ = enc.finish();
            black_box(pidx);
        })
    });
}

criterion_group!(benches, bench_compression);
criterion_main!(benches);
