use std::error::Error;

/// Parsed MDX model — only geometry data needed for the 3D viewer.
pub struct MdxModel {
    pub version: u32,
    pub name: String,
    pub geosets: Vec<MdxGeoset>,
    pub sequences: Vec<MdxSequence>,
    pub global_sequences: Vec<u32>,
    pub textures: Vec<MdxTexture>,
    pub materials: Vec<MdxMaterial>,
    pub bones: Vec<MdxBone>,
    pub helpers: Vec<MdxHelper>,
    pub attachments: Vec<MdxAttachment>,
    pub geoset_anims: Vec<MdxGeosetAnim>,
    pub pivot_points: Vec<[f32; 3]>,
}

pub struct MdxSequence {
    pub name: String,
    pub interval_start: u32,
    pub interval_end: u32,
    pub move_speed: f32,
    pub non_looping: u32,
    pub rarity: f32,
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

/// A single keyframe in an animation track.
pub struct MdxAnimKeyframe {
    pub frame: u32,
    /// Values: 3 floats for translation/scale, 4 floats for rotation (quaternion)
    pub value: Vec<f32>,
    /// Hermite/Bezier in-tangent (same size as value)
    pub in_tan: Vec<f32>,
    /// Hermite/Bezier out-tangent (same size as value)
    pub out_tan: Vec<f32>,
}

/// An animated track (KGTR, KGRT, KGSC).
pub struct MdxAnimTrack {
    /// Interpolation type: 0=None, 1=Linear, 2=Hermite, 3=Bezier
    pub line_type: u32,
    pub global_seq_id: i32,
    pub keyframes: Vec<MdxAnimKeyframe>,
}

/// A bone node parsed from the BONE chunk.
pub struct MdxBone {
    pub name: String,
    pub object_id: u32,
    pub parent_id: u32,   // 0xFFFFFFFF = no parent
    pub flags: u32,
    #[allow(dead_code)]
    pub geoset_id: u32,
    #[allow(dead_code)]
    pub geoset_anim_id: u32,
    pub translation: Option<MdxAnimTrack>,
    pub rotation: Option<MdxAnimTrack>,
    pub scaling: Option<MdxAnimTrack>,
}

/// A helper node parsed from the HELP chunk.
pub struct MdxHelper {
    pub name: String,
    pub object_id: u32,
    pub parent_id: u32,   // 0xFFFFFFFF = no parent
    pub flags: u32,
    pub translation: Option<MdxAnimTrack>,
    pub rotation: Option<MdxAnimTrack>,
    pub scaling: Option<MdxAnimTrack>,
}

/// An attachment node parsed from the ATCH chunk.
pub struct MdxAttachment {
    pub name: String,
    pub object_id: u32,
    pub parent_id: u32,
    pub flags: u32,
    pub translation: Option<MdxAnimTrack>,
    pub rotation: Option<MdxAnimTrack>,
    pub scaling: Option<MdxAnimTrack>,
}

/// Geoset animation parsed from the GEOA chunk.
pub struct MdxGeosetAnim {
    pub geoset_id: u32,
    pub alpha_track: Option<MdxAnimTrack>,
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
    /// 1 byte per vertex — index into matrix_groups
    pub vertex_groups: Vec<u8>,
    /// Flattened matrix group bone IDs — decode using matrix_group_counts
    pub matrix_ids: Vec<u32>,
    /// Number of bone IDs per matrix group
    pub matrix_group_counts: Vec<u32>,
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

// ── animation track parser ──────────────────────────────────────────────────

/// Try to read an animation track (KGTR/KGRT/KGSC) at the current offset.
/// Returns `None` if the next 4 bytes don't match `expected_tag`.
fn try_read_anim_track(
    buf: &[u8],
    offset: &mut usize,
    node_end: usize,
    expected_tag: &[u8; 4],
    value_size: usize, // 3 for translation/scale, 4 for rotation
) -> Result<Option<MdxAnimTrack>, Box<dyn Error + Send + Sync>> {
    if *offset + 4 > node_end || *offset + 4 > buf.len() {
        return Ok(None);
    }
    let tag_bytes: [u8; 4] = buf[*offset..*offset + 4].try_into()?;
    if !tag_eq(&tag_bytes, expected_tag) {
        return Ok(None);
    }
    *offset += 4; // consume tag

    if *offset + 12 > buf.len() {
        return Ok(None);
    }
    let num_keys = read_u32(buf, offset)? as usize;
    let line_type = read_u32(buf, offset)?;
    let global_seq_id = read_u32(buf, offset)? as i32;

    let mut keyframes = Vec::with_capacity(num_keys);
    for _ in 0..num_keys {
        if *offset + 4 + value_size * 4 > buf.len() { break; }
        let frame = read_u32(buf, offset)?;
        let value = read_f32_slice(buf, offset, value_size)?;
        let (in_tan, out_tan) = if line_type >= 2 {
            // Hermite or Bezier
            if *offset + value_size * 4 * 2 > buf.len() { break; }
            let it = read_f32_slice(buf, offset, value_size)?;
            let ot = read_f32_slice(buf, offset, value_size)?;
            (it, ot)
        } else {
            (Vec::new(), Vec::new())
        };
        keyframes.push(MdxAnimKeyframe { frame, value, in_tan, out_tan });
    }

    Ok(Some(MdxAnimTrack { line_type, global_seq_id, keyframes }))
}

// ── geoset parser ───────────────────────────────────────────────────────────

fn parse_geoset(buf: &[u8], geo_end: usize, offset: &mut usize) -> Result<MdxGeoset, Box<dyn Error + Send + Sync>> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut faces: Vec<u16> = Vec::new();
    let mut uvs: Vec<f32> = Vec::new();
    let mut material_id: u32 = 0;
    let mut vertex_groups: Vec<u8> = Vec::new();
    let mut matrix_ids: Vec<u32> = Vec::new();
    let mut matrix_group_counts: Vec<u32> = Vec::new();

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
            if *offset + count <= buf.len() {
                vertex_groups = buf[*offset..*offset + count].to_vec();
                *offset += count;
            } else {
                *offset += count;
            }
        } else if tag_eq(&sub_tag, b"MTGC") {
            let count = read_u32(buf, offset)? as usize;
            matrix_group_counts = Vec::with_capacity(count);
            for _ in 0..count {
                if *offset + 4 > buf.len() { break; }
                matrix_group_counts.push(read_u32(buf, offset)?);
            }
        } else if tag_eq(&sub_tag, b"MATS") {
            let count = read_u32(buf, offset)? as usize;
            matrix_ids = Vec::with_capacity(count);
            for _ in 0..count {
                if *offset + 4 > buf.len() { break; }
                matrix_ids.push(read_u32(buf, offset)?);
            }

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
        vertex_groups,
        matrix_ids,
        matrix_group_counts,
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
        global_sequences: Vec::new(),
        textures: Vec::new(),
        materials: Vec::new(),
        bones: Vec::new(),
        helpers: Vec::new(),
        attachments: Vec::new(),
        geoset_anims: Vec::new(),
        pivot_points: Vec::new(),
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
            // Each sequence = 132 bytes: name(80) + intervalStart(4) + intervalEnd(4) +
            // moveSpeed(4) + nonLooping(4) + rarity(4) + padding(4) + bounds(7*4)
            let seq_size = 132usize;
            let count = chunk_size / seq_size;
            for i in 0..count {
                let seq_start = offset + i * seq_size;
                let name = read_null_string(buf, seq_start, 80);
                let mut so = seq_start + 80;
                let interval_start = read_u32(buf, &mut so).unwrap_or(0);
                let interval_end = read_u32(buf, &mut so).unwrap_or(0);
                let move_speed = read_f32(buf, &mut so).unwrap_or(0.0);
                let non_looping = read_u32(buf, &mut so).unwrap_or(0);
                let rarity = read_f32(buf, &mut so).unwrap_or(0.0);
                // skip padding(4) + bounds(7*4) = 32 bytes
                model.sequences.push(MdxSequence {
                    name, interval_start, interval_end,
                    move_speed, non_looping, rarity,
                });
            }
        } else if tag_eq(&chunk_tag, b"GLBS") {
            // Global sequences: array of u32 durations
            let count = chunk_size / 4;
            for _ in 0..count {
                if offset + 4 <= chunk_end {
                    model.global_sequences.push(read_u32(buf, &mut offset)?);
                }
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

                let priority_plane = if mat_offset + 4 <= buf.len() {
                    let v = read_u32(buf, &mut mat_offset).unwrap_or(0); v
                } else { 0 };
                let mat_flags = if mat_offset + 4 <= buf.len() {
                    let v = read_u32(buf, &mut mat_offset).unwrap_or(0); v
                } else { 0 };

                let mut layers = Vec::new();

                if mat_offset + 8 <= buf.len() {
                    let lays_tag = read_tag(buf, &mut mat_offset);
                    if lays_tag.is_ok() && tag_eq(&lays_tag.unwrap(), b"LAYS") {
                        let layer_count = read_u32(buf, &mut mat_offset).unwrap_or(0) as usize;
                        for _ in 0..layer_count {
                            if mat_offset + 4 > buf.len() { break; }
                            let layer_inclusive = read_u32(buf, &mut mat_offset).unwrap_or(0) as usize;
                            let layer_end = (mat_offset - 4 + layer_inclusive).min(mat_end);

                            if mat_offset + 24 <= buf.len() {
                                let filter_mode = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let shading_flags = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let texture_id = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let _tex_anim_id = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let _coord_id = read_u32(buf, &mut mat_offset).unwrap_or(0);
                                let alpha = read_f32(buf, &mut mat_offset).unwrap_or(1.0);

                                layers.push(MdxMaterialLayer {
                                    filter_mode, shading_flags, texture_id, alpha,
                                });
                            }
                            mat_offset = layer_end;
                        }
                    }
                }

                model.materials.push(MdxMaterial {
                    priority_plane, flags: mat_flags, layers,
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
        } else if tag_eq(&chunk_tag, b"BONE") {
            // Each bone = OBJ node (inclusive size) + GeosetID(u32) + GeosetAnimID(u32)
            let mut bone_offset = offset;
            while bone_offset + 4 < chunk_end {
                let node_inclusive = u32::from_le_bytes(
                    buf[bone_offset..bone_offset + 4].try_into().unwrap_or_default()
                ) as usize;
                if node_inclusive < 96 { break; }
                let node_end = (bone_offset + node_inclusive).min(chunk_end);
                bone_offset += 4; // skip inclusive size

                if bone_offset + 92 <= buf.len() {
                    let name = read_null_string(buf, bone_offset, 80);
                    bone_offset += 80;
                    let object_id = read_u32(buf, &mut bone_offset).unwrap_or(0);
                    let parent_id = read_u32(buf, &mut bone_offset).unwrap_or(0xFFFFFFFF);
                    let flags = read_u32(buf, &mut bone_offset).unwrap_or(0);

                    // Parse animation tracks (KGTR, KGRT, KGSC) inside the OBJ node
                    let translation = try_read_anim_track(buf, &mut bone_offset, node_end, b"KGTR", 3).unwrap_or(None);
                    let rotation = try_read_anim_track(buf, &mut bone_offset, node_end, b"KGRT", 4).unwrap_or(None);
                    let scaling = try_read_anim_track(buf, &mut bone_offset, node_end, b"KGSC", 3).unwrap_or(None);

                    bone_offset = node_end;

                    // After node: GeosetID, GeosetAnimID
                    let geoset_id = if bone_offset + 4 <= buf.len() {
                        read_u32(buf, &mut bone_offset).unwrap_or(0)
                    } else { 0 };
                    let geoset_anim_id = if bone_offset + 4 <= buf.len() {
                        read_u32(buf, &mut bone_offset).unwrap_or(0)
                    } else { 0 };

                    model.bones.push(MdxBone {
                        name, object_id, parent_id, flags,
                        geoset_id, geoset_anim_id,
                        translation, rotation, scaling,
                    });
                } else {
                    bone_offset = node_end + 8;
                }
            }
        } else if tag_eq(&chunk_tag, b"HELP") {
            let mut help_offset = offset;
            while help_offset + 4 < chunk_end {
                let node_inclusive = u32::from_le_bytes(
                    buf[help_offset..help_offset + 4].try_into().unwrap_or_default()
                ) as usize;
                if node_inclusive < 96 { break; }
                let node_end = (help_offset + node_inclusive).min(chunk_end);
                help_offset += 4;

                if help_offset + 92 <= buf.len() {
                    let name = read_null_string(buf, help_offset, 80);
                    help_offset += 80;
                    let object_id = read_u32(buf, &mut help_offset).unwrap_or(0);
                    let parent_id = read_u32(buf, &mut help_offset).unwrap_or(0xFFFFFFFF);
                    let flags = read_u32(buf, &mut help_offset).unwrap_or(0);

                    let translation = try_read_anim_track(buf, &mut help_offset, node_end, b"KGTR", 3).unwrap_or(None);
                    let rotation = try_read_anim_track(buf, &mut help_offset, node_end, b"KGRT", 4).unwrap_or(None);
                    let scaling = try_read_anim_track(buf, &mut help_offset, node_end, b"KGSC", 3).unwrap_or(None);

                    model.helpers.push(MdxHelper {
                        name, object_id, parent_id, flags,
                        translation, rotation, scaling,
                    });
                }
                help_offset = node_end;
            }
        } else if tag_eq(&chunk_tag, b"ATCH") {
            // Each attachment = OBJ node (inclusive size) + path(256) + attachmentId(u32) + optional KATV
            let mut atch_offset = offset;
            while atch_offset + 4 < chunk_end {
                let node_inclusive = u32::from_le_bytes(
                    buf[atch_offset..atch_offset + 4].try_into().unwrap_or_default()
                ) as usize;
                if node_inclusive < 96 { break; }
                let node_end = (atch_offset + node_inclusive).min(chunk_end);
                atch_offset += 4;

                if atch_offset + 92 <= buf.len() {
                    let name = read_null_string(buf, atch_offset, 80);
                    atch_offset += 80;
                    let object_id = read_u32(buf, &mut atch_offset).unwrap_or(0);
                    let parent_id = read_u32(buf, &mut atch_offset).unwrap_or(0xFFFFFFFF);
                    let flags = read_u32(buf, &mut atch_offset).unwrap_or(0);

                    let translation = try_read_anim_track(buf, &mut atch_offset, node_end, b"KGTR", 3).unwrap_or(None);
                    let rotation = try_read_anim_track(buf, &mut atch_offset, node_end, b"KGRT", 4).unwrap_or(None);
                    let scaling = try_read_anim_track(buf, &mut atch_offset, node_end, b"KGSC", 3).unwrap_or(None);

                    model.attachments.push(MdxAttachment {
                        name, object_id, parent_id, flags,
                        translation, rotation, scaling,
                    });
                }
                // Skip rest (path 256 bytes + attachmentId + optional KATV track)
                atch_offset = node_end;
                // After OBJ node: path(256) + attachmentId(u32) — these are outside the node inclusive size
                if atch_offset + 260 <= chunk_end {
                    atch_offset += 260; // path(256) + attachmentId(4)
                }
                // Skip optional KATV (visibility) animated track
                // peek for KATV tag
                if atch_offset + 4 <= chunk_end && atch_offset + 4 <= buf.len() {
                    let peek: [u8; 4] = buf[atch_offset..atch_offset + 4].try_into().unwrap_or_default();
                    if tag_eq(&peek, b"KATV") {
                        atch_offset += 4;
                        if atch_offset + 12 <= buf.len() {
                            let num_keys = read_u32(buf, &mut atch_offset).unwrap_or(0) as usize;
                            let lt = read_u32(buf, &mut atch_offset).unwrap_or(0);
                            let _gs = read_u32(buf, &mut atch_offset).unwrap_or(0);
                            let per_key = if lt >= 2 { 4 + 4 * 3 } else { 4 + 4 };
                            atch_offset += num_keys * per_key;
                        }
                    }
                }
            }
        } else if tag_eq(&chunk_tag, b"GEOA") {
            // Geoset animations — each has inclusive size
            let mut geoa_offset = offset;
            while geoa_offset + 4 < chunk_end {
                if geoa_offset + 4 > buf.len() { break; }
                let inclusive_size = u32::from_le_bytes(
                    buf[geoa_offset..geoa_offset + 4].try_into().unwrap_or_default()
                ) as usize;
                let geoa_end = (geoa_offset + inclusive_size).min(chunk_end);
                geoa_offset += 4;

                // alpha(f32), flags(u32), color(3 floats), geosetId(u32) = 4+4+12+4 = 24 bytes
                let mut alpha_track = None;
                let mut geoset_id = 0u32;
                if geoa_offset + 24 <= buf.len() {
                    let _static_alpha = read_f32(buf, &mut geoa_offset).unwrap_or(1.0);
                    let _flags = read_u32(buf, &mut geoa_offset).unwrap_or(0);
                    let _r = read_f32(buf, &mut geoa_offset).unwrap_or(1.0);
                    let _g = read_f32(buf, &mut geoa_offset).unwrap_or(1.0);
                    let _b = read_f32(buf, &mut geoa_offset).unwrap_or(1.0);
                    geoset_id = read_u32(buf, &mut geoa_offset).unwrap_or(0);

                    // Try to read KGAO (alpha) track
                    alpha_track = try_read_anim_track(buf, &mut geoa_offset, geoa_end, b"KGAO", 1).unwrap_or(None);
                    // KGAC (color) track may follow — skip for now
                }

                model.geoset_anims.push(MdxGeosetAnim { geoset_id, alpha_track });
                geoa_offset = geoa_end;
            }
        } else if tag_eq(&chunk_tag, b"PIVT") {
            // Pivot points: simple array of [x, y, z] floats
            let num_pivots = chunk_size / 12;
            let mut pivt_offset = offset;
            for _ in 0..num_pivots {
                if pivt_offset + 12 > buf.len() { break; }
                let x = read_f32(buf, &mut pivt_offset).unwrap_or(0.0);
                let y = read_f32(buf, &mut pivt_offset).unwrap_or(0.0);
                let z = read_f32(buf, &mut pivt_offset).unwrap_or(0.0);
                model.pivot_points.push([x, y, z]);
            }
        }
        // Skip any other chunk
        offset = chunk_end;
    }

    Ok(model)
}

