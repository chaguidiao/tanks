//! Single-band GeoTIFF read/write (pure Rust via the `tiff` crate). Rasters carry
//! ModelPixelScale + ModelTiepoint tags so GIS tools (QGIS) place them correctly in the local
//! metric frame. TIFF row 0 is the northern row; our grids store row 0 in the south.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::encoder::{Compression, DeflateLevel, TiffEncoder, colortype};
use tiff::tags::Tag;

use crate::{Grid, Result, TerrainError};

fn flip_rows<T: Copy>(g: &Grid<T>) -> Vec<T> {
    let mut out = Vec::with_capacity(g.data.len());
    for j in (0..g.h).rev() {
        out.extend_from_slice(&g.data[j * g.w..(j + 1) * g.w]);
    }
    out
}

fn unflip<T: Copy>(w: usize, h: usize, data: Vec<T>) -> Grid<T> {
    let mut out = Vec::with_capacity(data.len());
    for j in (0..h).rev() {
        out.extend_from_slice(&data[j * w..(j + 1) * w]);
    }
    Grid::from_vec(w, h, out)
}

macro_rules! write_impl {
    ($name:ident, $t:ty, $ct:ty) => {
        pub fn $name(path: &Path, g: &Grid<$t>, cell: f64) -> Result<()> {
            let f = File::create(path).map_err(|e| TerrainError::io(path, e))?;
            let err = |e: tiff::TiffError| TerrainError::io(path, e);
            let mut enc = TiffEncoder::new(BufWriter::new(f))
                .map_err(err)?
                .with_compression(Compression::Deflate(DeflateLevel::Balanced));
            let mut img = enc.new_image::<$ct>(g.w as u32, g.h as u32).map_err(err)?;
            let top = g.h as f64 * cell;
            img.encoder().write_tag(Tag::ModelPixelScaleTag, &[cell, cell, 0.0][..]).map_err(err)?;
            img.encoder().write_tag(Tag::ModelTiepointTag, &[0.0, 0.0, 0.0, 0.0, top, 0.0][..]).map_err(err)?;
            img.write_data(&flip_rows(g)).map_err(err)
        }
    };
}

write_impl!(write_f32, f32, colortype::Gray32Float);
write_impl!(write_u8, u8, colortype::Gray8);

fn open(path: &Path) -> Result<Decoder<File>> {
    let f = File::open(path).map_err(|e| TerrainError::io(path, e))?;
    Ok(Decoder::new(f).map_err(|e| TerrainError::io(path, e))?.with_limits(Limits::unlimited()))
}

pub fn read_f32(path: &Path) -> Result<Grid<f32>> {
    let mut d = open(path)?;
    let (w, h) = d.dimensions().map_err(|e| TerrainError::io(path, e))?;
    match d.read_image().map_err(|e| TerrainError::io(path, e))? {
        DecodingResult::F32(v) => Ok(unflip(w as usize, h as usize, v)),
        _ => Err(TerrainError::io(path, "expected float32 raster")),
    }
}

pub fn read_u8(path: &Path) -> Result<Grid<u8>> {
    let mut d = open(path)?;
    let (w, h) = d.dimensions().map_err(|e| TerrainError::io(path, e))?;
    match d.read_image().map_err(|e| TerrainError::io(path, e))? {
        DecodingResult::U8(v) => Ok(unflip(w as usize, h as usize, v)),
        _ => Err(TerrainError::io(path, "expected uint8 raster")),
    }
}
