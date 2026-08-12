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

pub fn e8e9_filter(data: &mut [u8], is_encode: bool) {
    let len = data.len();
    if len < 5 {
        return;
    }
    let mut i = 0;
    while i < len - 4 {
        if data[i] == 0xE8 || data[i] == 0xE9 {
            let offset = u32::from_le_bytes([data[i + 1], data[i + 2], data[i + 3], data[i + 4]]);
            let new_offset = if is_encode {
                offset.wrapping_add(i as u32)
            } else {
                offset.wrapping_sub(i as u32)
            };
            data[i + 1..i + 5].copy_from_slice(&new_offset.to_le_bytes());
            i += 5;
        } else {
            i += 1;
        }
    }
}
