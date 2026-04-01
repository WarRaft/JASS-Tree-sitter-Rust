use std::error::Error;

/// Parsed MDX model — only geometry data needed for the 3D viewer.
pub struct MdxModel {
    pub version: u32,
    pub name: String,
    pub geosets: Vec<MdxGeoset>,
    pub sequences: Vec<MdxSequence>,
    pub textures: Vec<MdxTexture>,
    pub materials: Vec<MdxMaterial>,
}

pub struct MdxSequence {
    pub name: String,
}

pub struct MdxTexture {
    pub replaceable_id: u32,
    pub file_name: String,
    pub flags: u32,
}

pub struct MdxMaterialLayer {
    pub filter_mode: u32,
    pub shading_flags: u32,
    pub texture_id: u32,
    pub alpha: f32,
}

pub struct MdxMaterial {
    pub priority_plane: u32,
    pub flags: u32,
    pub layers: Vec<MdxMaterialLayer>,
}

pub struct MdxGeoset {
    /// xyz interleaved — `[x0, y0, z0, x1, y1, z1, …]`
    pub vertices: Vec<f32>,
    /// xyz interleaved
    pub normals: Vec<f32>,
    /// Triangle indices (u16)
    pub faces: Vec<u16>,
    /// First UV set, uv interleaved — V already flipped for Three.js (1−v)
    pub uvs: Vec<f32>,
    pub material_id: u32,
    pub vertex_count: u32,
    pub face_count: u32,
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn read_tag(buf: &[u8], offset: &mut usize) -> Result<[u8; 4], Box<dyn Error + Send + Sync>> {
    if *offset + 4 > buf.len() {
        return Err("Unexpected end of file reading tag".into());
    }
    let tag: [u8; 4] = buf[*offset..*offset + 4].try_into()?;
    *offset += 4;
    Ok(tag)
}

fn read_u32(buf: &[u8], offset: &mut usize) -> Result<u32, Box<dyn Error + Send + Sync>> {
    if *offset + 4 > buf.len() {
        return Err("Unexpected end of file reading u32".into());
    }
    let v = u32::from_le_bytes(buf[*offset..*offset + 4].try_into()?);
    *offset += 4;
    Ok(v)
}

fn read_f32(buf: &[u8], offset: &mut usize) -> Result<f32, Box<dyn Error + Send + Sync>> {
    if *offset + 4 > buf.len() {
        return Err("Unexpected end of file reading f32".into());
    }
    let v = f32::from_le_bytes(buf[*offset..*offset + 4].try_into()?);
    *offset += 4;
    Ok(v)
}

fn read_f32_slice(buf: &[u8], offset: &mut usize, count: usize) -> Result<Vec<f32>, Box<dyn Error + Send + Sync>> {
    let byte_len = count * 4;
    if *offset + byte_len > buf.len() {
        return Err(format!("Unexpected end of file reading {} floats", count).into());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let start = *offset + i * 4;
        let v = f32::from_le_bytes(buf[start..start + 4].try_into()?);
        out.push(v);
    }
    *offset += byte_len;
    Ok(out)
}

fn read_u16_slice(buf: &[u8], offset: &mut usize, count: usize) -> Result<Vec<u16>, Box<dyn Error + Send + Sync>> {
    let byte_len = count * 2;
    if *offset + byte_len > buf.len() {
        return Err(format!("Unexpected end of file reading {} u16s", count).into());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let start = *offset + i * 2;
        let v = u16::from_le_bytes(buf[start..start + 2].try_into()?);
        out.push(v);
    }
    *offset += byte_len;
    Ok(out)
}

fn read_null_string(buf: &[u8], offset: usize, max_len: usize) -> String {
    let end = (offset + max_len).min(buf.len());
    let slice = &buf[offset..end];
    let null_pos = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..null_pos]).into_owned()
}

fn tag_eq(tag: &[u8; 4], s: &[u8; 4]) -> bool {
    tag == s
}

// ── geoset parser ───────────────────────────────────────────────────────────

fn parse_geoset(buf: &[u8], geo_end: usize, offset: &mut usize) -> Result<MdxGeoset, Box<dyn Error + Send + Sync>> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut faces: Vec<u16> = Vec::new();
    let mut uvs: Vec<f32> = Vec::new();
    let mut material_id: u32 = 0;

    while *offset < geo_end {
        if *offset + 4 > buf.len() { break; }
        let sub_tag = read_tag(buf, offset)?;

        if tag_eq(&sub_tag, b"VRTX") {
            let count = read_u32(buf, offset)? as usize;
            vertices = read_f32_slice(buf, offset, count * 3)?;
        } else if tag_eq(&sub_tag, b"NRMS") {
            let count = read_u32(buf, offset)? as usize;
            normals = read_f32_slice(buf, offset, count * 3)?;
        } else if tag_eq(&sub_tag, b"PTYP") {
            let count = read_u32(buf, offset)? as usize;
            *offset += count * 4;
        } else if tag_eq(&sub_tag, b"PCNT") {
            let count = read_u32(buf, offset)? as usize;
            *offset += count * 4;
        } else if tag_eq(&sub_tag, b"PVTX") {
            let count = read_u32(buf, offset)? as usize;
            faces = read_u16_slice(buf, offset, count)?;
        } else if tag_eq(&sub_tag, b"GNDX") {
            let count = read_u32(buf, offset)? as usize;
            // 1 byte per vertex group
            *offset += count;
        } else if tag_eq(&sub_tag, b"MTGC") {
            let count = read_u32(buf, offset)? as usize;
            *offset += count * 4;
        } else if tag_eq(&sub_tag, b"MATS") {
            let count = read_u32(buf, offset)? as usize;
            *offset += count * 4; // matrix ids

            // materialId, selectionGroup, selectionFlags
            if *offset + 12 <= buf.len() {
                material_id = read_u32(buf, offset)?;
                *offset += 4; // selectionGroup
                *offset += 4; // selectionFlags
            }

            // 7 floats: boundsRadius, minExtent(3), maxExtent(3)
            if *offset + 7 * 4 <= buf.len() {
                *offset += 7 * 4;
            }

            // anim extents: count + count * 7 floats
            if *offset + 4 <= buf.len() {
                let num_anims = read_u32(buf, offset)? as usize;
                *offset += num_anims * 7 * 4;
            }
        } else if tag_eq(&sub_tag, b"UVAS") {
            let num_sets = read_u32(buf, offset)? as usize;
            for s in 0..num_sets {
                // "UVBS" tag
                if *offset + 4 > buf.len() { break; }
                *offset += 4; // skip UVBS tag
                let uv_count = read_u32(buf, offset)? as usize;
                let raw = read_f32_slice(buf, offset, uv_count * 2)?;
                // Only keep first UV set; flip V for Three.js (1 − v)
                if s == 0 {
                    let mut flipped = Vec::with_capacity(raw.len());
                    for pair in raw.chunks_exact(2) {
                        flipped.push(pair[0]);
                        flipped.push(1.0 - pair[1]);
                    }
                    uvs = flipped;
                }
            }
        } else {
            // Unknown sub-tag inside geoset — skip to geoset end
            *offset = geo_end;
        }
    }

    let vertex_count = (vertices.len() / 3) as u32;
    let face_count = (faces.len() / 3) as u32;

    Ok(MdxGeoset {
        vertices,
        normals,
        faces,
        uvs,
        material_id,
        vertex_count,
        face_count,
    })
}

// ── top-level parser ────────────────────────────────────────────────────────

pub fn parse(buf: &[u8]) -> Result<MdxModel, Box<dyn Error + Send + Sync>> {
    if buf.len() < 4 || &buf[0..4] != b"MDLX" {
        return Err("Not a valid MDX file (missing MDLX magic)".into());
    }

    let mut offset = 4usize;
    let mut model = MdxModel {
        version: 0,
        name: String::new(),
        geosets: Vec::new(),
        sequences: Vec::new(),
        textures: Vec::new(),
        materials: Vec::new(),
    };

    while offset < buf.len() {
        if offset + 8 > buf.len() { break; }
        let chunk_tag = read_tag(buf, &mut offset)?;
        let chunk_size = read_u32(buf, &mut offset)? as usize;
        let chunk_end = (offset + chunk_size).min(buf.len());

        if tag_eq(&chunk_tag, b"VERS") {
            if chunk_size >= 4 {
                model.version = read_u32(buf, &mut offset)?;
            }
        } else if tag_eq(&chunk_tag, b"MODL") {
            model.name = read_null_string(buf, offset, 80.min(chunk_size));
        } else if tag_eq(&chunk_tag, b"SEQS") {
            let seq_size = 132usize;
            let count = chunk_size / seq_size;
            for i in 0..count {
                let seq_start = offset + i * seq_size;
                let name = read_null_string(buf, seq_start, 80);
                model.sequences.push(MdxSequence { name });
            }
        } else if tag_eq(&chunk_tag, b"TEXS") {
            let tex_size = 268usize;
            let count = chunk_size / tex_size;
            for i in 0..count {
                let tex_off = offset + i * tex_size;
                let replaceable_id = if tex_off + 4 <= buf.len() {
                    u32::from_le_bytes(buf[tex_off..tex_off + 4].try_into().unwrap_or_default())
                } else { 0 };
                let file_name = read_null_string(buf, tex_off + 4, 260);
                let flags = if tex_off + 268 <= buf.len() {
                    u32::from_le_bytes(buf[tex_off + 264..tex_off + 268].try_into().unwrap_or_default())
                } else { 0 };
                model.textures.push(MdxTexture { replaceable_id, file_name, flags });
            }
        } else if tag_eq(&chunk_tag, b"MTLS") {
            let mut mat_offset = offset;
            while mat_offset < chunk_end {
                if mat_offset + 4 > buf.len() { break; }
                let inclusive_size = u32::from_le_bytes(
                    buf[mat_offset..mat_offset + 4].try_into().unwrap_or_default()
                ) as usize;
                let mat_end = (mat_offset + inclusive_size).min(chunk_end);
                mat_offset += 4;

                // priority_plane (u32), flags (u32)
                let priority_plane = if mat_offset + 4 <= buf.len() {
                    let v = read_u32(buf, &mut mat_offset).unwrap_or(0); v
                } else { 0 };
                let mat_flags = if mat_offset + 4 <= buf.len() {
                    let v = read_u32(buf, &mut mat_offset).unwrap_or(0); v
                } else { 0 };

                let mut layers = Vec::new();

                // "LAYS" tag + layer_count
                if mat_offset + 8 <= buf.len() {
                    let lays_tag = read_tag(buf, &mut mat_offset);
                    if lays_tag.is_ok() && tag_eq(&lays_tag.unwrap(), b"LAYS") {
                        let layer_count = read_u32(buf, &mut mat_offset).unwrap_or(0) as usize;
                        for _ in 0..layer_count {
                            if mat_offset + 4 > buf.len() { break; }
                            let layer_inclusive = read_u32(buf, &mut mat_offset).unwrap_or(0) as usize;
                            let layer_end = (mat_offset - 4 + layer_inclusive).min(mat_end);

                            // filter_mode(u32), shading_flags(u32), texture_id(u32),
                            // texture_animation_id(i32), coord_id(u32), alpha(f32)
                            if mat_offset + 24 <= buf.len() {
                                let filter_mode = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let shading_flags = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let texture_id = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let _tex_anim_id = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let _coord_id = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let alpha = read_f32(buf, &mut mat_offset).unwrap_or(1.0);

                                layers.push(MdxMaterialLayer {
                                    filter_mode,
                                    shading_flags,
                                    texture_id,
                                    alpha,
                                });
                            }

                            // Skip rest of layer (animated blocks, etc.)
                            mat_offset = layer_end;
                        }
                    }
                }

                model.materials.push(MdxMaterial {
                    priority_plane,
                    flags: mat_flags,
                    layers,
                });

                mat_offset = mat_end;
            }
        } else if tag_eq(&chunk_tag, b"GEOS") {
            let mut geo_offset = offset;
            while geo_offset < chunk_end {
                if geo_offset + 4 > buf.len() { break; }
                let inclusive_size = u32::from_le_bytes(
                    buf[geo_offset..geo_offset + 4].try_into().unwrap_or_default()
                ) as usize;
                let geo_end = (geo_offset + inclusive_size).min(chunk_end);
                geo_offset += 4;

                match parse_geoset(buf, geo_end, &mut geo_offset) {
                    Ok(geo) => model.geosets.push(geo),
                    Err(_) => {}
                }
                geo_offset = geo_end;
            }
        }
        // Skip any other chunk
        offset = chunk_end;
    }

    Ok(model)
}


