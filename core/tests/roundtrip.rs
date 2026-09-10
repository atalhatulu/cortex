use cortex::mtf::{bwt_mtf_rle, decode_rle_mtf_bwt, MtfModel};
use cortex::rangecoder::{Decoder, Encoder};
use cortex::{
    compress_file, compress_file_with_progress, decompress_file, decompress_file_with_progress,
};
use rand::seq::SliceRandom;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::fs;

fn test_roundtrip(data: &[u8]) {
    if data.is_empty() {
        // Empty block contract: encode yields pidx == 0 with no tokens, and
        // decode must return an empty output (Ok(vec![])) — a real byte-exact
        // roundtrip for the empty input, not a rejection.
        let (pidx, tokens) = bwt_mtf_rle(data);
        assert_eq!(pidx, [0; 8], "empty input must yield primary index [0; 8]");
        assert!(tokens.is_empty(), "empty input must yield no RLE tokens");
        let out = decode_rle_mtf_bwt(pidx, &tokens, data.len()).unwrap();
        assert!(out.is_empty(), "empty block must decode to empty output");
        return;
    }
    let (pidx, tokens) = bwt_mtf_rle(data);
    let mut enc = Encoder::new();
    let mut model_enc = MtfModel::new();
    model_enc.encode_tokens(&mut enc, &tokens);
    let compressed = enc.finish();

    let mut dec = Decoder::new(&compressed);
    let mut model_dec = MtfModel::new();
    let decoded_tokens = model_dec.decode_tokens(&mut dec, tokens.len()).unwrap();
    let decompressed = decode_rle_mtf_bwt(pidx, &decoded_tokens, data.len()).unwrap();
    if data != decompressed.as_slice() {
        let first_diff = data
            .iter()
            .zip(decompressed.iter())
            .enumerate()
            .find(|(_, (a, b))| a != b);
        println!("PIDX: {:?}", pidx);
        println!("First diff at: {:?}", first_diff);
    }
    assert_eq!(
        data,
        decompressed.as_slice(),
        "Roundtrip failed for data length {}",
        data.len()
    );
}

#[test]
fn test_roundtrip_empty() {
    // Empty data must reach the assertions inside test_roundtrip (no early return).
    test_roundtrip(b"");
}

#[test]
fn test_roundtrip_size_1() {
    test_roundtrip(b"A");
}

#[test]
fn test_roundtrip_basic() {
    test_roundtrip(b"hello world hello world hello world");
}

#[test]
fn test_roundtrip_random() {
    let mut data = Vec::with_capacity(1000);
    for i in 0..1000 {
        data.push((i * 13 % 256) as u8);
    }
    test_roundtrip(&data);
}

#[test]
fn test_file_api_roundtrip() {
    let input_path = "tests/test_in.bin";
    let comp_path = "tests/test_comp.ctx";
    let dec_path = "tests/test_out.bin";

    fs::write(
        input_path,
        b"file api roundtrip test string over multiple blocks maybe?",
    )
    .unwrap();

    compress_file(input_path, comp_path).unwrap();
    decompress_file(comp_path, dec_path).unwrap();

    let original = fs::read(input_path).unwrap();
    let restored = fs::read(dec_path).unwrap();

    assert_eq!(original, restored);

    fs::remove_file(input_path).unwrap();
    fs::remove_file(comp_path).unwrap();
    fs::remove_file(dec_path).unwrap();
}

/// High-entropy (already-compressed-like) data must exercise the STORE path:
/// classify() returns AlreadyCompressed → compress emits a raw block (bit 30
/// set) → decode returns DecompressStage1::Stored verbatim. Verifies the whole
/// STORE roundtrip through the file API (default CTX8 mode).
#[test]
fn test_file_api_stored_roundtrip() {
    let input_path = "tests/test_stored_in.bin";
    let comp_path = "tests/test_stored_comp.ctx";
    let dec_path = "tests/test_stored_out.bin";

    // Deterministic pseudo-random, high-entropy payload (no magic bytes, so the
    // entropy heuristic must classify it as already-compressed).
    let mut data = Vec::with_capacity(4096);
    let mut rng = StdRng::seed_from_u64(0x57EED_0000_1234);
    for _ in 0..8 {
        let mut chunk = [0u8; 512];
        rng.fill_bytes(&mut chunk);
        data.extend_from_slice(&chunk);
    }
    fs::write(input_path, &data).unwrap();

    compress_file(input_path, comp_path).unwrap();
    decompress_file(comp_path, dec_path).unwrap();

    let original = fs::read(input_path).unwrap();
    let restored = fs::read(dec_path).unwrap();

    assert_eq!(original, restored, "STORE roundtrip must be byte-exact");

    fs::remove_file(input_path).unwrap();
    fs::remove_file(comp_path).unwrap();
    fs::remove_file(dec_path).unwrap();
}

/// A file whose head carries a well-known already-compressed magic (PNG) must
/// be classified AlreadyCompressed and stored raw, still decoding byte-exact.
#[test]
fn test_file_api_magic_stored_roundtrip() {
    let input_path = "tests/test_magic_in.bin";
    let comp_path = "tests/test_magic_comp.ctx";
    let dec_path = "tests/test_magic_out.bin";

    let mut data = Vec::with_capacity(2048);
    data.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut rng = StdRng::seed_from_u64(0x0000_1234_0001);
    let mut body = [0u8; 1500];
    rng.fill_bytes(&mut body);
    data.extend_from_slice(&body);
    fs::write(input_path, &data).unwrap();

    compress_file(input_path, comp_path).unwrap();
    decompress_file(comp_path, dec_path).unwrap();

    let original = fs::read(input_path).unwrap();
    let restored = fs::read(dec_path).unwrap();

    assert_eq!(
        original, restored,
        "magic STORE roundtrip must be byte-exact"
    );

    fs::remove_file(input_path).unwrap();
    fs::remove_file(comp_path).unwrap();
    fs::remove_file(dec_path).unwrap();
}

#[test]
fn test_file_api_all_modes_and_encryption_roundtrip() {
    let input_path = "tests/test_modes_in.bin";
    let data = b"file api roundtrip test string over multiple blocks maybe?".to_vec();
    fs::write(input_path, &data).unwrap();

    for (name, fast, tans, password) in [
        ("ctx8", false, false, None),
        ("ctxt", false, true, Some("correct horse battery staple")),
        ("ctxf", true, false, None),
    ] {
        let comp_path = format!("tests/test_modes_{name}.ctx");
        let dec_path = format!("tests/test_modes_{name}.out");
        compress_file_with_progress(
            input_path,
            &comp_path,
            Some(br#"[{\"name\":\"test\"}]"#),
            password,
            1,
            0,
            fast,
            tans,
            |_, _| {},
        )
        .unwrap();
        decompress_file_with_progress(&comp_path, &dec_path, password, |_, _| {}).unwrap();
        assert_eq!(fs::read(&dec_path).unwrap(), data, "{name} must roundtrip");
        fs::remove_file(comp_path).unwrap();
        fs::remove_file(dec_path).unwrap();
    }

    fs::remove_file(input_path).unwrap();
}

#[test]
fn test_decoder_rejects_truncated_and_malformed_archives() {
    let truncated = "tests/test_truncated.ctx";
    let truncated_out = "tests/test_truncated.out";
    // CTX8 header: one original byte, but no chunk follows.
    let mut header = Vec::new();
    header.extend_from_slice(b"CTX8");
    header.extend_from_slice(&1u64.to_le_bytes());
    header.push(0);
    header.extend_from_slice(&(1024 * 1024u32).to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    fs::write(truncated, header).unwrap();
    assert!(decompress_file(truncated, truncated_out).is_err());

    let malformed = "tests/test_malformed_tans.ctx";
    let malformed_out = "tests/test_malformed_tans.out";
    let mut archive = Vec::new();
    archive.extend_from_slice(b"CTXT");
    archive.extend_from_slice(&1u64.to_le_bytes());
    archive.push(0);
    archive.extend_from_slice(&(1024 * 1024u32).to_le_bytes());
    archive.extend_from_slice(&0u32.to_le_bytes());
    let mut chunk = vec![0u8; 32 + 4 + 4 + 1028];
    chunk[32..36].copy_from_slice(&1u32.to_le_bytes());
    chunk[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
    archive.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    archive.extend_from_slice(&chunk);
    fs::write(malformed, archive).unwrap();
    assert!(decompress_file(malformed, malformed_out).is_err());

    for path in [truncated, truncated_out, malformed, malformed_out] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn test_inverse_bwt_rejects_out_of_range_primary_indices() {
    // A malformed archive controls pidx. It must be rejected before the
    // inverse-BWT hot loop's unchecked indexing is reached.
    let err = decode_rle_mtf_bwt([u32::MAX; 8], &[0], 1).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

/// Content classification unit test: known magics, plain text, and high-entropy
/// data map to the expected kinds.
#[cfg(test)]
mod content_tests {
    use cortex::content::{classify, ContentKind};

    #[test]
    fn classify_known_magics() {
        let png = b"\x89PNG\r\n\x1a\nsome body";
        assert_eq!(classify(png), ContentKind::AlreadyCompressed);

        let gz = b"\x1f\x8b\x08compressed-body";
        assert_eq!(classify(gz), ContentKind::AlreadyCompressed);

        let zip = b"PK\x03\x04rest";
        assert_eq!(classify(zip), ContentKind::AlreadyCompressed);
    }

    #[test]
    fn classify_plain_text_is_not_already_compressed() {
        let text = b"the quick brown fox jumps over the lazy dog. ".repeat(200);
        assert!(!classify(&text).is_already_compressed());
    }
}

#[test]
fn test_roundtrip_all_zero() {
    test_roundtrip(&vec![0x00; 100]);
}

#[test]
fn test_roundtrip_all_ff() {
    test_roundtrip(&vec![0xFF; 100]);
}

#[test]
fn test_roundtrip_increasing() {
    let data: Vec<u8> = (0..=255).collect();
    test_roundtrip(&data);
}

#[test]
fn test_roundtrip_decreasing() {
    let data: Vec<u8> = (0..=255).rev().collect();
    test_roundtrip(&data);
}

#[test]
fn test_roundtrip_long_run() {
    // A single 10_000-byte run forces the RLE stage to encode a large
    // zero-run (and stresses the "RLE burst" decode path).
    test_roundtrip(&vec![0x41; 10_000]);
}

#[test]
fn test_roundtrip_all_bytes() {
    // All 256 distinct byte values, in a fixed non-monotonic order
    // (complements test_roundtrip_increasing).
    let mut data: Vec<u8> = (0..=255).collect();
    let mut rng = StdRng::seed_from_u64(0x5EED_5EED_0000_0001);
    data.shuffle(&mut rng);
    test_roundtrip(&data);
}

#[test]
fn test_roundtrip_fuzz_deterministic() {
    // Fixed seeds and sizes make this reproducible: the "fuzz" is over a
    // deterministic corpus, not whatever the RNG happens to produce per run.
    const SEEDS: [u64; 5] = [
        0x5EED_CAFE_0000_0001,
        0x1234_5678_9ABC_DEF0,
        0xDEAD_BEEF_CAFE_F00D,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
    ];
    const SIZES: [usize; 12] = [0, 1, 2, 3, 15, 64, 255, 256, 257, 1000, 4096, 64_000];

    for &seed in &SEEDS {
        for &size in &SIZES {
            let mut data = vec![0u8; size];
            // A fresh RNG per (seed, size) pair keeps every case deterministic
            // and independent; size 0 exercises the empty-input path.
            StdRng::seed_from_u64(seed).fill_bytes(&mut data);
            test_roundtrip(&data);
        }
    }
}
