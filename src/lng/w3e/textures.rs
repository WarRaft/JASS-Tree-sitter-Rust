//! Load and encode terrain tile textures from game files.
//!
//! For each ground tile code in the `.w3e` file, this module:
//! 1. Looks up the texture path via Terrain.slk metadata
//! 2. Loads the file from MPQ / game folder via cascading lookup
//! 3. Decodes BLP or TGA to RGBA
//! 4. Encodes as a PNG base64 data URL for the webview

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::{DynamicImage, ImageFormat, RgbaImage};
use log::debug;
use serde::Serialize;
use std::io::Cursor;

use super::file_lookup::lookup_file;
use super::slk::TerrainSlkResult;
use crate::util::bin_reader::Rawcode;

/// Texture data for a single tile type, ready for the webview.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TileTexture {
    /// Width of the decoded texture in pixels.
    pub width: u32,
    /// Height of the decoded texture in pixels.
    pub height: u32,
    /// `data:image/png;base64,…` string.
    pub data_url: String,
}

/// Load textures for all ground tile codes.
///
/// Returns a `Vec` of the same length as `ground_tile_codes`.
/// Each entry is `None` if the texture could not be loaded.
pub fn load_tile_textures(
    ground_tile_codes: &[Rawcode],
    slk: Option<&TerrainSlkResult>,
    archive_path: Option<&str>,
) -> Vec<Option<TileTexture>> {
    let slk = match slk {
        Some(s) => s,
        None => return ground_tile_codes.iter().map(|_| None).collect(),
    };

    ground_tile_codes
        .iter()
        .map(|code| {
            let tile_info = slk.tiles.iter().find(|t| t.tile_id == code.0)?;
            if tile_info.dir.is_empty() || tile_info.file.is_empty() || tile_info.ext.is_empty() {
                debug!("textures: tile {} has incomplete path info", code);
                return None;
            }

            let rel_path = format!("{}\\{}{}", tile_info.dir, tile_info.file, tile_info.ext);
            let (buf, source) = lookup_file(&rel_path, archive_path)?;
            debug!("textures: loaded {} from {}", rel_path, source);

            let img = decode_texture(&buf, &tile_info.ext)?;
            let data_url = rgba_to_png_data_url(&img)?;

            Some(TileTexture {
                width: img.width(),
                height: img.height(),
                data_url,
            })
        })
        .collect()
}

/// Decode a texture file (BLP or TGA) into an RGBA image.
fn decode_texture(buf: &[u8], ext: &str) -> Option<RgbaImage> {
    match ext.to_lowercase().as_str() {
        ".blp" => {
            let mut image = blp::core::image::ImageBlp::from_buf(buf).ok()?;
            image.decode(buf, &[]).ok()?;
            image.mipmaps.first()?.image.clone()
        }
        ".tga" => {
            let img = image::load_from_memory_with_format(buf, ImageFormat::Tga).ok()?;
            Some(img.to_rgba8())
        }
        _ => {
            // Try generic image loading as fallback
            let img = image::load_from_memory(buf).ok()?;
            Some(img.to_rgba8())
        }
    }
}

/// Encode an RGBA image as a PNG base64 data URL.
fn rgba_to_png_data_url(img: &RgbaImage) -> Option<String> {
    let dynamic = DynamicImage::ImageRgba8(img.clone());
    let mut cursor = Cursor::new(Vec::new());
    dynamic.write_to(&mut cursor, ImageFormat::Png).ok()?;
    let bytes = cursor.into_inner();
    Some(format!("data:image/png;base64,{}", STANDARD.encode(&bytes)))
}

