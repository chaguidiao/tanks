//! Minimal RGBA8 PNG writer (debug renders and baked display textures).

use std::io::Write;
use std::path::Path;

use crate::{Result, TerrainError};

fn chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut c = crc32fast::Hasher::new();
    c.update(ty);
    c.update(data);
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    out.extend_from_slice(&c.finalize().to_be_bytes());
}

pub fn encode_rgba(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), w * h * 4);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    let mut raw = Vec::with_capacity(h * (w * 4 + 1));
    for row in rgba.chunks_exact(w * 4) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(&raw).expect("in-memory write");
    chunk(&mut out, b"IDAT", &z.finish().expect("in-memory write"));
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn write_rgba(path: &Path, w: usize, h: usize, rgba: &[u8]) -> Result<()> {
    std::fs::write(path, encode_rgba(w, h, rgba)).map_err(|e| TerrainError::io(path, e))
}
