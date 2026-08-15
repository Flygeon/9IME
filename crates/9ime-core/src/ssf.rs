//! Sogou .ssf skin container handling.
//!
//! Two formats (validated against the v1 implementation and ssfconv):
//! - plain zip archive
//! - encrypted: "Skin" magic (8 bytes) + AES-256-CBC stream (fixed key/iv,
//!   public knowledge) whose plaintext is a 4-byte header + zlib stream of
//!   a custom table-of-contents archive.

use std::collections::HashMap;
use std::io::Read;

use aes::cipher::{BlockDecryptMut, KeyIvInit};
use cbc::Decryptor;

type Aes256Cbc = Decryptor<aes::Aes256>;

/// Public Sogou skin encryption key/iv (same constants as ssfconv).
const SSF_KEY: [u8; 32] = [
    0x52, 0x36, 0x46, 0x1A, 0xD3, 0x85, 0x03, 0x66, 0x90, 0x45, 0x16, 0x28,
    0x79, 0x03, 0x36, 0x23, 0xDD, 0xBE, 0x6F, 0x03, 0xFF, 0x04, 0xE3, 0xCA,
    0xD5, 0x7F, 0xFC, 0xA3, 0x50, 0xE4, 0x9E, 0xD9,
];

const SSF_IV: [u8; 16] = [
    0xE0, 0x7A, 0xAD, 0x35, 0xE0, 0x90, 0xAA, 0x03, 0x8A, 0x51, 0xFD, 0x05,
    0xDF, 0x8C, 0x5D, 0x0F,
];

const MAX_FILE: usize = 64 * 1024 * 1024;

/// Extracted skin files: lowercase name -> content.
pub type SkinFiles = HashMap<String, Vec<u8>>;

fn u32le(p: &[u8]) -> u32 {
    u32::from_le_bytes([p[0], p[1], p[2], p[3]])
}

/// Inflate a zlib stream into `out` (grows up to 64 MiB).
fn zlib_inflate(src: &[u8], out: &mut Vec<u8>) -> bool {
    use flate2::read::ZlibDecoder;
    let mut dec = ZlibDecoder::new(src);
    let mut buf = [0u8; 65536];
    loop {
        match dec.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if out.len() + n > MAX_FILE {
                    return false;
                }
                out.extend_from_slice(&buf[..n]);
            }
            Err(_) => return false,
        }
    }
    true
}

fn aes_decrypt(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() % 16 != 0 {
        return None;
    }
    let mut buf = data.to_vec();
    let dec = Aes256Cbc::new_from_slices(&SSF_KEY, &SSF_IV).ok()?;
    dec.decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf)
        .ok()?;
    Some(buf)
}

/// Parse the encrypted container payload (after AES + zlib).
fn parse_custom_archive(data: &[u8], files: &mut SkinFiles) -> bool {
    if data.len() < 8 {
        return false;
    }
    let offsets_size = u32le(&data[4..]) as usize;
    if offsets_size > data.len() - 8 {
        return false;
    }
    let mut off = 8;
    while off + 4 <= 8 + offsets_size {
        let e = u32le(&data[off..]) as usize;
        if e + 8 > data.len() {
            off += 4;
            continue;
        }
        let name_len = u32le(&data[e..]) as usize;
        if name_len > data.len() - e - 4 {
            off += 4;
            continue;
        }
        let name_u16 = &data[e + 4..e + 4 + name_len];
        let mut name = String::new();
        for c in name_u16.chunks_exact(2) {
            let u = u16::from_le_bytes([c[0], c[1]]);
            name.push(char::from_u32(u as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
        }
        let content_len_pos = e + 4 + name_len;
        if content_len_pos + 4 > data.len() {
            off += 4;
            continue;
        }
        let content_len = u32le(&data[content_len_pos..]) as usize;
        let start = content_len_pos + 4;
        if start + content_len > data.len() {
            off += 4;
            continue;
        }
        files.insert(name.to_lowercase(), data[start..start + content_len].to_vec());
        off += 4;
    }
    !files.is_empty()
}

/// Extract files from a plain zip archive.
fn parse_zip(buf: &[u8], files: &mut SkinFiles) -> bool {
    let mut zip = match zip::ZipArchive::new(std::io::Cursor::new(buf)) {
        Ok(z) => z,
        Err(_) => return false,
    };
    for i in 0..zip.len() {
        let mut f = match zip.by_index(i) {
            Ok(f) => f,
            Err(_) => continue,
        };
        if f.is_dir() || f.size() == 0 || f.size() > MAX_FILE as u64 {
            continue;
        }
        let name = f.name().to_string();
        let mut content = Vec::new();
        if f.read_to_end(&mut content).is_err() {
            continue;
        }
        files.insert(name.to_lowercase(), content);
    }
    !files.is_empty()
}

/// Extract all files from a .ssf buffer. Returns None when the data is not
/// a recognized skin container.
pub fn extract(buf: &[u8]) -> Option<SkinFiles> {
    let mut files = SkinFiles::new();
    if buf.len() >= 8 && buf.starts_with(b"Skin") {
        let cipher = &buf[8..];
        let plain = aes_decrypt(cipher)?;
        if plain.len() < 8 {
            return None;
        }
        let mut data = Vec::new();
        if !zlib_inflate(&plain[4..], &mut data) || data.len() < 8 {
            return None;
        }
        return if parse_custom_archive(&data, &mut files) { Some(files) } else { None };
    }
    if parse_zip(buf, &mut files) {
        Some(files)
    } else {
        None
    }
}
