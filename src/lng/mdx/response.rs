use crate::lng::mdx::parse::{MdxBone, MdxGeoset, MdxHelper, MdxMaterial, MdxMaterialLayer, MdxModel, MdxSequence, MdxTexture};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use url::Url;

/// Top-level response for `mdx/render`.
#[derive(Serialize)]
pub struct MdxResponse<'a> {
    pub uri: &'a Url,
    pub version: u32,
    pub name: String,
    pub size: usize,
    pub geosets: Vec<MdxGeosetResponse>,
    pub sequences: Vec<MdxSequenceResponse>,
    pub textures: Vec<MdxTextureResponse>,
    pub materials: Vec<MdxMaterialResponse>,
    pub bones: Vec<MdxNodeResponse>,
    pub helpers: Vec<MdxNodeResponse>,
    pub pivot_points: Vec<[f32; 3]>,
    pub total_vertices: u32,
    pub total_faces: u32,
}

/// Each geoset's geometry data — arrays are base64-encoded little-endian binary,
/// ready to be decoded into TypedArrays on the client.
#[derive(Serialize)]
pub struct MdxGeosetResponse {
    pub material_id: u32,
    pub vertex_count: u32,
    pub face_count: u32,
    /// base64(Float32Array) — xyz interleaved
    pub vertices: String,
    /// base64(Float32Array) — xyz interleaved
    pub normals: String,
    /// base64(Uint16Array) — triangle indices
    pub faces: String,
    /// base64(Float32Array) — uv interleaved, V already flipped
    pub uvs: String,
    /// base64(Float32Array) — pre-computed normal line segments
    /// `[px, py, pz, px+nx*L, py+ny*L, pz+nz*L, …]` for every vertex.
    /// Ready to feed into `THREE.LineSegments` as-is.
    pub normal_lines: String,
}

#[derive(Serialize)]
pub struct MdxSequenceResponse {
    pub name: String,
}

#[derive(Serialize)]
pub struct MdxTextureResponse {
    pub replaceable_id: u32,
    pub file_name: String,
    pub flags: u32,
}

#[derive(Serialize)]
pub struct MdxMaterialLayerResponse {
    pub filter_mode: u32,
    pub shading_flags: u32,
    pub texture_id: u32,
    pub alpha: f32,
}

#[derive(Serialize)]
pub struct MdxMaterialResponse {
    pub priority_plane: u32,
    pub flags: u32,
    pub layers: Vec<MdxMaterialLayerResponse>,
}

/// Bone or helper node — shared response structure.
#[derive(Serialize)]
pub struct MdxNodeResponse {
    pub name: String,
    pub object_id: u32,
    pub parent_id: u32,
    pub flags: u32,
}

// ── zero-copy byte cast ─────────────────────────────────────────────────────
// All modern targets (x86, ARM, WASM) are little-endian — we can
// reinterpret numeric slices as `&[u8]` without any per-element conversion.

#[inline]
fn as_u8_slice_f32(data: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding; LE byte order matches JS Float32Array.
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) }
}

#[inline]
fn as_u8_slice_u16(data: &[u16]) -> &[u8] {
    // SAFETY: u16 has no padding; LE byte order matches JS Uint16Array.
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) }
}

#[inline]
fn encode_f32(data: &[f32]) -> String {
    STANDARD.encode(as_u8_slice_f32(data))
}

#[inline]
fn encode_u16(data: &[u16]) -> String {
    STANDARD.encode(as_u8_slice_u16(data))
}

// ── normal lines pre-computation ────────────────────────────────────────────

const NORMAL_LINE_LENGTH: f32 = 5.0;

/// Build the line-segments array for vertex normals visualisation.
/// Layout: `[px, py, pz, px+nx*L, py+ny*L, pz+nz*L, …]`
fn build_normal_lines(vertices: &[f32], normals: &[f32]) -> Vec<f32> {
    if normals.is_empty() || vertices.len() != normals.len() {
        return Vec::new();
    }
    let vert_count = vertices.len() / 3;
    let mut out = Vec::with_capacity(vert_count * 6); // 2 points × 3 floats
    for i in 0..vert_count {
        let base = i * 3;
        let px = vertices[base];
        let py = vertices[base + 1];
        let pz = vertices[base + 2];
        let nx = normals[base];
        let ny = normals[base + 1];
        let nz = normals[base + 2];
        // start point
        out.push(px);
        out.push(py);
        out.push(pz);
        // end point
        out.push(px + nx * NORMAL_LINE_LENGTH);
        out.push(py + ny * NORMAL_LINE_LENGTH);
        out.push(pz + nz * NORMAL_LINE_LENGTH);
    }
    out
}

// ── conversions ─────────────────────────────────────────────────────────────

impl From<&MdxGeoset> for MdxGeosetResponse {
    fn from(g: &MdxGeoset) -> Self {
        let normal_lines = build_normal_lines(&g.vertices, &g.normals);
        Self {
            material_id: g.material_id,
            vertex_count: g.vertex_count,
            face_count: g.face_count,
            vertices: encode_f32(&g.vertices),
            normals: encode_f32(&g.normals),
            faces: encode_u16(&g.faces),
            uvs: encode_f32(&g.uvs),
            normal_lines: encode_f32(&normal_lines),
        }
    }
}

impl From<&MdxSequence> for MdxSequenceResponse {
    fn from(s: &MdxSequence) -> Self {
        Self { name: s.name.clone() }
    }
}

impl From<&MdxTexture> for MdxTextureResponse {
    fn from(t: &MdxTexture) -> Self {
        Self {
            replaceable_id: t.replaceable_id,
            file_name: t.file_name.clone(),
            flags: t.flags,
        }
    }
}

impl From<&MdxMaterialLayer> for MdxMaterialLayerResponse {
    fn from(l: &MdxMaterialLayer) -> Self {
        Self {
            filter_mode: l.filter_mode,
            shading_flags: l.shading_flags,
            texture_id: l.texture_id,
            alpha: l.alpha,
        }
    }
}

impl From<&MdxMaterial> for MdxMaterialResponse {
    fn from(m: &MdxMaterial) -> Self {
        Self {
            priority_plane: m.priority_plane,
            flags: m.flags,
            layers: m.layers.iter().map(MdxMaterialLayerResponse::from).collect(),
        }
    }
}

impl From<&MdxBone> for MdxNodeResponse {
    fn from(b: &MdxBone) -> Self {
        Self {
            name: b.name.clone(),
            object_id: b.object_id,
            parent_id: b.parent_id,
            flags: b.flags,
        }
    }
}

impl From<&MdxHelper> for MdxNodeResponse {
    fn from(h: &MdxHelper) -> Self {
        Self {
            name: h.name.clone(),
            object_id: h.object_id,
            parent_id: h.parent_id,
            flags: h.flags,
        }
    }
}

impl<'a> MdxResponse<'a> {
    pub fn from_model(uri: &'a Url, model: &MdxModel, file_size: usize) -> Self {
        let total_vertices: u32 = model.geosets.iter().map(|g| g.vertex_count).sum();
        let total_faces: u32 = model.geosets.iter().map(|g| g.face_count).sum();

        Self {
            uri,
            version: model.version,
            name: model.name.clone(),
            size: file_size,
            geosets: model.geosets.iter().map(MdxGeosetResponse::from).collect(),
            sequences: model.sequences.iter().map(MdxSequenceResponse::from).collect(),
            textures: model.textures.iter().map(MdxTextureResponse::from).collect(),
            materials: model.materials.iter().map(MdxMaterialResponse::from).collect(),
            bones: model.bones.iter().map(MdxNodeResponse::from).collect(),
            helpers: model.helpers.iter().map(MdxNodeResponse::from).collect(),
            pivot_points: model.pivot_points.clone(),
            total_vertices,
            total_faces,
        }
    }
}
