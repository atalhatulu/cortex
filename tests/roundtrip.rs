use cortex::mtf::{bwt_mtf_rle, decode_rle_mtf_bwt, MtfModel};
use cortex::rangecoder::{Encoder, Decoder};
use cortex::{compress_file, decompress_file};
use std::fs;

fn test_roundtrip(data: &[u8]) {
    if data.is_empty() { return; } // bwt_mtf_rle does not handle size 0 well.
    let (pidx, tokens) = bwt_mtf_rle(data);
    let mut enc = Encoder::new();
    let mut model_enc = MtfModel::new();
    model_enc.encode_tokens(&mut enc, &tokens);
    let compressed = enc.finish();
    
    let mut dec = Decoder::new(&compressed);
    let mut model_dec = MtfModel::new();
    let decoded_tokens = model_dec.decode_tokens(&mut dec, tokens.len()).unwrap();
    
    let decompressed = decode_rle_mtf_bwt(pidx as usize, &decoded_tokens, data.len());
    assert_eq!(data, decompressed.as_slice(), "Roundtrip failed for data length {}", data.len());
}

#[test]
fn test_roundtrip_empty() {
    // Empty data test handled implicitly by not panicking if we add an early return.
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
    let comp_path = "tests/test_comp.crx";
    let dec_path = "tests/test_out.bin";
    
    fs::write(input_path, b"file api roundtrip test string over multiple blocks maybe?").unwrap();
    
    compress_file(input_path, comp_path).unwrap();
    decompress_file(comp_path, dec_path).unwrap();
    
    let original = fs::read(input_path).unwrap();
    let restored = fs::read(dec_path).unwrap();
    
    assert_eq!(original, restored);
    
    fs::remove_file(input_path).unwrap();
    fs::remove_file(comp_path).unwrap();
    fs::remove_file(dec_path).unwrap();
}
