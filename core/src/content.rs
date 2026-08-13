//! Content-aware classification + filtering framework.
//!
//! Decides, per block, whether the data is worth running through the full
//! BWT+entropy pipeline, or whether it is already-compressed (high entropy /
//! known container magic) and should be stored raw (`STORE`). Raw storage
//! avoids a double-compression waste of time AND a ratio loss on uncompressible
//! data.
//!
//! Two-stage classifier:
//!   1. Magic-byte match against known already-compressed containers.
//!   2. Shannon entropy estimate on a leading sample (only when no magic hit).
//!
//! The classification result feeds a per-block "stored" flag in the archive
//! format (bit 30 of `num_tokens` for the BWT modes) — it does NOT change the
//! user-selected mode; it only says "this block skips entropy".

/// How substantial, qualitatively, a block's content is. Classification is a
/// hint, not a hard guarantee; a wrong guess on a hard boundary costs at most
/// one block of ratio, never correctness (decode is driven by the stored flag,
/// not by re-classification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    /// Plain text / source code — full BWT pipeline is the right call.
    Text,
    /// Generic binary — full BWT pipeline (with E8/E9 for executables if detected).
    Binary,
    /// Executable (PE/ELF/Mach-O) — BWT + E8/E9 pre-pass.
    Executable,
    /// Already-compressed / high-entropy — store raw, skip entropy entirely.
    AlreadyCompressed,
}

impl ContentKind {
    /// Returns true when the block should be stored raw instead of going through
    /// the BWT/entropy pipeline.
    #[inline]
    pub fn is_already_compressed(self) -> bool {
        matches!(self, ContentKind::AlreadyCompressed)
    }
}

/// Lead-in bytes we sample from each block for magic + entropy testing.
/// 64 KiB is cheap and plenty to bias the entropy estimate.
pub const SAMPLE_BYTES: usize = 64 * 1024;

/// Entropy threshold (bits / byte) above which we treat a block as
/// already-compressed. Raw bytes sit ~4.5-5.5; text ~4-4.5; compressed output
/// of a decent codec is ~7.5-8. Using 7.0 is safe: text never reaches it, and
/// a genuinely-compressed stream almost always exceeds it.
const ALREADY_COMPRESSED_ENTROPY: f64 = 7.0;

/// Minimum block size to bother sampling at all. Tiny blocks cost more to
/// classify than they save.
const MIN_SAMPLE_SIZE: usize = 1024;

/// Known already-compressed container signatures (magic bytes). Covers the
/// formats a real user archive actually holds. Ordered longest-first so a
/// longer magic (e.g. RIFF/WEBP) matches before a generic prefix.
fn magic_kind(sample: &[u8]) -> Option<ContentKind> {
    // GIF, PNG, JPEG (two variants), WebP, ZIP, GZIP, zstd, xz-lzma (.xz magic),
    // bzip2, 7z, PDF (often compressed streams), MP3 (with/without ID3), FLAC,
    // OGG/Vorbis/Opus, MP4/MOV (ftyp), AVI (RIFF), WAV (RIFF), WebM/Matroska
    // (0x1A 0x45 0xDF 0xA3), gzip header, tar without leading zeros sometimes
    // looks low-entropy so we do NOT classify tar alone as compressed.
    if sample.len() >= 4 && sample[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return Some(ContentKind::AlreadyCompressed); // Matroska/WebM
    }
    if sample.len() >= 4 && &sample[0..4] == b"RIFF" {
        return Some(ContentKind::AlreadyCompressed); // WAV/AVI/WebP family
    }
    if sample.len() >= 4 && &sample[0..3] == b"ID3" {
        return Some(ContentKind::AlreadyCompressed); // MP3 with ID3 tag
    }
    let sigs: &[(&[u8], ContentKind)] = &[
        (b"\x89PNG\r\n\x1a\n", ContentKind::AlreadyCompressed), // PNG
        (b"\xff\xd8\xff", ContentKind::AlreadyCompressed),       // JPEG
        (b"GIF87a", ContentKind::AlreadyCompressed),
        (b"GIF89a", ContentKind::AlreadyCompressed),
        (b"PK\x03\x04", ContentKind::AlreadyCompressed), // ZIP
        (b"\x1f\x8b\x08", ContentKind::AlreadyCompressed), // gzip
        (b"\x28\xb5\x2f\xfd", ContentKind::AlreadyCompressed), // zstd
        (b"\xfd7zXZ\x00", ContentKind::AlreadyCompressed), // xz
        (b"BZh", ContentKind::AlreadyCompressed),       // bzip2
        (b"7z\xbc\xaf\x27\x1c", ContentKind::AlreadyCompressed), // 7z
        (b"%PDF-", ContentKind::AlreadyCompressed),     // PDF (mostly compressed)
        (b"fLaC", ContentKind::AlreadyCompressed),      // FLAC (lossless audio, still dense)
        (b"OggS", ContentKind::AlreadyCompressed),      // OGG/Opus
        (b"fLaC\x00\x00\x00\x22", ContentKind::AlreadyCompressed),
    ];
    for (sig, kind) in sigs {
        if sample.len() >= sig.len() && &sample[..sig.len()] == *sig {
            return Some(*kind);
        }
    }
    // ISO / udf boot block often yields zero-filled then high entropy; do not
    // special-case — entropy heuristic handles it.
    None
}

/// Sample the block and estimate its Shannon entropy (bits per byte).
pub fn block_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let sample = &data[..data.len().min(SAMPLE_BYTES)];
    let mut counts = [0u64; 256];
    for &b in sample {
        counts[b as usize] += 1;
    }
    let n = sample.len() as f64;
    let mut ent = 0.0;
    for &c in &counts {
        if c == 0 {
            continue;
        }
        let p = c as f64 / n;
        ent -= p * p.log2();
    }
    ent
}

/// Classify a block into a `ContentKind`.
///
/// - Magic match first (cheap, decisive).
/// - Executable header detection for the E8/E9 pre-pass.
/// - Otherwise entropy heuristic: high entropy => already-compressed.
pub fn classify(data: &[u8]) -> ContentKind {
    if data.len() < MIN_SAMPLE_SIZE {
        return ContentKind::Binary;
    }
    let sample = &data[..data.len().min(SAMPLE_BYTES)];

    // Executable headers — but only if it's NOT also a known-compressed format.
    if is_executable(sample) {
        return ContentKind::Executable;
    }
    if let Some(kind) = magic_kind(sample) {
        return kind;
    }
    if block_entropy(data) >= ALREADY_COMPRESSED_ENTROPY {
        return ContentKind::AlreadyCompressed;
    }
    // Heuristic: is most of the sample printable ASCII? Then call it text.
    let printable = sample
        .iter()
        .filter(|&&b| (b >= 0x20 && b <= 0x7E) || b == b'\n' || b == b'\r' || b == b'\t')
        .count();
    if printable as f64 / sample.len() as f64 > 0.90 {
        ContentKind::Text
    } else {
        ContentKind::Binary
    }
}

/// Re-exported executable header check (was in filters.rs). Kept here so the
/// classifier owns the decision and filters.rs only does the byte transform.
pub fn is_executable(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    // MZ (DOS/Windows PE)
    if data[0] == 0x4D && data[1] == 0x5A {
        return true;
    }
    // ELF (Linux/Unix)
    if data[0] == 0x7F && data[1] == 0x45 && data[2] == 0x4C && data[3] == 0x46 {
        return true;
    }
    // Mach-O (macOS)
    if (data[0] == 0xCF && data[1] == 0xFA && data[2] == 0xED && data[3] == 0xFE)
        || (data[0] == 0xCE && data[1] == 0xFA && data[2] == 0xED && data[3] == 0xFE)
        || (data[0] == 0xCA && data[1] == 0xFE && data[2] == 0xBA && data[3] == 0xBE)
    {
        return true;
    }
    false
}
