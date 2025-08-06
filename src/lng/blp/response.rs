use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use blp_rs::mipmap::Mipmap;
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::Serialize;
use std::io::Cursor;
use url::Url;

#[derive(Serialize)]
pub struct BlpResponse<'a> {
    pub uri: &'a Url,
    pub mipmaps: Vec<BlpMipmapMeta>,
}

#[derive(Serialize)]
pub struct BlpMipmapMeta {
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_data_url: Option<String>,
}

impl From<&Mipmap> for BlpMipmapMeta {
    fn from(m: &Mipmap) -> Self {
        Self {
            width: m.width,
            height: m.height,
            image_data_url: m.image.as_ref().and_then(image_to_data_url),
        }
    }
}

fn image_to_data_url(img: &RgbaImage) -> Option<String> {
    let dynamic = DynamicImage::ImageRgba8(img.clone());
    let mut cursor = Cursor::new(Vec::new());

    if dynamic.write_to(&mut cursor, ImageFormat::Png).is_ok() {
        let bytes = cursor.into_inner();
        Some(format!("data:image/png;base64,{}", STANDARD.encode(&bytes)))
    } else {
        None
    }
}
