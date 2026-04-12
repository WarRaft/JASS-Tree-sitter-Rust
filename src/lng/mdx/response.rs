//! Binary MDX response for `POST /render/mdx`.
//!
//! Returns `application/octet-stream` — no JSON, no base64.
//!
//! ## Binary layout (all little-endian)
//!
//! ```text
//! ── HEADER ───────────────────────────────────────────
//! u32  version
//! u32  name_len
//! u8[name_len]  name (UTF-8)
//! u32  file_size
//! u32  total_vertices
//! u32  total_faces
//! u32  num_geosets
//! u32  num_sequences
//! u32  num_global_sequences
//! u32  num_textures
//! u32  num_materials
//! u32  num_bones
//! u32  num_helpers
//! u32  num_attachments
//! u32  num_geoset_anims
//! u32  num_pivot_points
//!
//! ── GEOSETS (×num_geosets) ───────────────────────────
//! u32  material_id
//! u32  vertex_count
//! u32  face_count
//! u32  vertices_bytes          (= vertex_count * 3 * 4)
//! u8[vertices_bytes]           raw f32 LE
//! u32  normals_bytes
//! u8[normals_bytes]            raw f32 LE
//! u32  faces_bytes             (= face_count * 3 * 2)
//! u8[faces_bytes]              raw u16 LE
//! u32  uvs_bytes
//! u8[uvs_bytes]                raw f32 LE
//! u32  vertex_groups_len
//! u8[vertex_groups_len]
//! u32  matrix_ids_count
//! u32[matrix_ids_count]
//! u32  matrix_group_counts_count
//! u32[matrix_group_counts_count]
//!
//! ── SEQUENCES (×num_sequences) ──────────────────────
//! u32  name_len
//! u8[name_len]  name (UTF-8)
//! u32  interval_start
//! u32  interval_end
//! f32  move_speed
//! u32  non_looping
//! f32  rarity
//!
//! ── GLOBAL SEQUENCES ────────────────────────────────
//! u32[num_global_sequences]
//!
//! ── TEXTURES (×num_textures) ────────────────────────
//! u32  replaceable_id
//! u32  file_name_len
//! u8[file_name_len]  file_name (UTF-8)
//! u32  flags
//!
//! ── MATERIALS (×num_materials) ──────────────────────
//! u32  priority_plane
//! u32  flags
//! u32  num_layers
//!   LAYER (×num_layers):
//!     u32  filter_mode
//!     u32  shading_flags
//!     u32  texture_id
//!     f32  alpha
//!
//! ── BONES (×num_bones) ──────────────────────────────
//!   NODE (see below)
//!
//! ── HELPERS (×num_helpers) ──────────────────────────
//!   NODE (see below)
//!
//! ── ATTACHMENTS (×num_attachments) ──────────────────
//!   NODE (see below)
//!
//! NODE:
//!   u32  name_len
//!   u8[name_len]  name (UTF-8)
//!   u32  object_id
//!   u32  parent_id
//!   u32  flags
//!   u8   has_translation
//!   [if has_translation: ANIM_TRACK]
//!   u8   has_rotation
//!   [if has_rotation: ANIM_TRACK]
//!   u8   has_scaling
//!   [if has_scaling: ANIM_TRACK]
//!
//! ANIM_TRACK:
//!   u32  line_type
//!   i32  global_seq_id
//!   u32  num_keyframes
//!   u32  value_size            (3 for trans/scale, 4 for rotation, 1 for alpha)
//!   KEYFRAME (×num_keyframes):
//!     u32  frame
//!     f32[value_size]  value
//!     [if line_type ≥ 2:]
//!       f32[value_size]  in_tan
//!       f32[value_size]  out_tan
//!
//! ── GEOSET ANIMS (×num_geoset_anims) ────────────────
//! u32  geoset_id
//! u8   has_alpha_track
//! [if has_alpha_track: ANIM_TRACK]
//!
//! ── PIVOT POINTS ────────────────────────────────────
//! f32[num_pivot_points * 3]    xyz interleaved
//! ```

use crate::lng::mdx::parse::{MdxAnimTrack, MdxModel};

// ── helpers ─────────────────────────────────────────────────────────────────

#[inline]
fn push_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

#[inline]
fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_f32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_str(buf: &mut Vec<u8>, s: &str) {
    push_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

/// Write a `&[f32]` as raw little-endian bytes (zero-copy reinterpret).
#[inline]
fn push_f32_slice(buf: &mut Vec<u8>, data: &[f32]) {
    let bytes = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4)
    };
    buf.extend_from_slice(bytes);
}

/// Write a `&[u16]` as raw little-endian bytes (zero-copy reinterpret).
#[inline]
fn push_u16_slice(buf: &mut Vec<u8>, data: &[u16]) {
    let bytes = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2)
    };
    buf.extend_from_slice(bytes);
}

/// Write a `&[u32]` as raw little-endian bytes (zero-copy reinterpret).
#[inline]
fn push_u32_slice(buf: &mut Vec<u8>, data: &[u32]) {
    let bytes = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4)
    };
    buf.extend_from_slice(bytes);
}

// ── anim track writer ───────────────────────────────────────────────────────

fn write_optional_track(buf: &mut Vec<u8>, track: &Option<MdxAnimTrack>) {
    match track {
        None => push_u8(buf, 0),
        Some(t) => {
            push_u8(buf, 1);
            write_track(buf, t);
        }
    }
}

fn write_track(buf: &mut Vec<u8>, t: &MdxAnimTrack) {
    push_u32(buf, t.line_type);
    push_i32(buf, t.global_seq_id);
    push_u32(buf, t.keyframes.len() as u32);

    // value_size: determined from the first keyframe (all same within a track)
    let value_size = t.keyframes.first().map_or(0, |kf| kf.value.len()) as u32;
    push_u32(buf, value_size);

    for kf in &t.keyframes {
        push_u32(buf, kf.frame);
        push_f32_slice(buf, &kf.value);
        if t.line_type >= 2 {
            push_f32_slice(buf, &kf.in_tan);
            push_f32_slice(buf, &kf.out_tan);
        }
    }
}

// ── public API ──────────────────────────────────────────────────────────────

/// Pack the parsed MDX model into a flat binary buffer.
/// Layout matches the doc comment at the top of this module.
pub fn pack_binary(model: &MdxModel, file_size: usize) -> Vec<u8> {
    let total_vertices: u32 = model.geosets.iter().map(|g| g.vertex_count).sum();
    let total_faces: u32 = model.geosets.iter().map(|g| g.face_count).sum();

    let mut buf = Vec::with_capacity(64 * 1024);

    // ── header ──────────────────────────────────────────────────────────
    push_u32(&mut buf, model.version);
    push_str(&mut buf, &model.name);
    push_u32(&mut buf, file_size as u32);
    push_u32(&mut buf, total_vertices);
    push_u32(&mut buf, total_faces);
    push_u32(&mut buf, model.geosets.len() as u32);
    push_u32(&mut buf, model.sequences.len() as u32);
    push_u32(&mut buf, model.global_sequences.len() as u32);
    push_u32(&mut buf, model.textures.len() as u32);
    push_u32(&mut buf, model.materials.len() as u32);
    push_u32(&mut buf, model.bones.len() as u32);
    push_u32(&mut buf, model.helpers.len() as u32);
    push_u32(&mut buf, model.attachments.len() as u32);
    push_u32(&mut buf, model.geoset_anims.len() as u32);
    push_u32(&mut buf, model.pivot_points.len() as u32);

    // ── geosets ─────────────────────────────────────────────────────────
    for g in &model.geosets {
        push_u32(&mut buf, g.material_id);
        push_u32(&mut buf, g.vertex_count);
        push_u32(&mut buf, g.face_count);

        // vertices (f32)
        push_u32(&mut buf, (g.vertices.len() * 4) as u32);
        push_f32_slice(&mut buf, &g.vertices);

        // normals (f32)
        push_u32(&mut buf, (g.normals.len() * 4) as u32);
        push_f32_slice(&mut buf, &g.normals);

        // faces (u16)
        push_u32(&mut buf, (g.faces.len() * 2) as u32);
        push_u16_slice(&mut buf, &g.faces);

        // uvs (f32)
        push_u32(&mut buf, (g.uvs.len() * 4) as u32);
        push_f32_slice(&mut buf, &g.uvs);

        // vertex groups (u8)
        push_u32(&mut buf, g.vertex_groups.len() as u32);
        buf.extend_from_slice(&g.vertex_groups);

        // matrix ids (u32)
        push_u32(&mut buf, g.matrix_ids.len() as u32);
        push_u32_slice(&mut buf, &g.matrix_ids);

        // matrix group counts (u32)
        push_u32(&mut buf, g.matrix_group_counts.len() as u32);
        push_u32_slice(&mut buf, &g.matrix_group_counts);
    }

    // ── sequences ───────────────────────────────────────────────────────
    for s in &model.sequences {
        push_str(&mut buf, &s.name);
        push_u32(&mut buf, s.interval_start);
        push_u32(&mut buf, s.interval_end);
        push_f32(&mut buf, s.move_speed);
        push_u32(&mut buf, s.non_looping);
        push_f32(&mut buf, s.rarity);
    }

    // ── global sequences ────────────────────────────────────────────────
    push_u32_slice(&mut buf, &model.global_sequences);

    // ── textures ────────────────────────────────────────────────────────
    for t in &model.textures {
        push_u32(&mut buf, t.replaceable_id);
        push_str(&mut buf, &t.file_name);
        push_u32(&mut buf, t.flags);
    }

    // ── materials ───────────────────────────────────────────────────────
    for m in &model.materials {
        push_u32(&mut buf, m.priority_plane);
        push_u32(&mut buf, m.flags);
        push_u32(&mut buf, m.layers.len() as u32);
        for l in &m.layers {
            push_u32(&mut buf, l.filter_mode);
            push_u32(&mut buf, l.shading_flags);
            push_u32(&mut buf, l.texture_id);
            push_f32(&mut buf, l.alpha);
        }
    }

    // ── bones ───────────────────────────────────────────────────────────
    for b in &model.bones {
        push_str(&mut buf, &b.name);
        push_u32(&mut buf, b.object_id);
        push_u32(&mut buf, b.parent_id);
        push_u32(&mut buf, b.flags);
        write_optional_track(&mut buf, &b.translation);
        write_optional_track(&mut buf, &b.rotation);
        write_optional_track(&mut buf, &b.scaling);
    }

    // ── helpers ─────────────────────────────────────────────────────────
    for h in &model.helpers {
        push_str(&mut buf, &h.name);
        push_u32(&mut buf, h.object_id);
        push_u32(&mut buf, h.parent_id);
        push_u32(&mut buf, h.flags);
        write_optional_track(&mut buf, &h.translation);
        write_optional_track(&mut buf, &h.rotation);
        write_optional_track(&mut buf, &h.scaling);
    }

    // ── attachments ─────────────────────────────────────────────────────
    for a in &model.attachments {
        push_str(&mut buf, &a.name);
        push_u32(&mut buf, a.object_id);
        push_u32(&mut buf, a.parent_id);
        push_u32(&mut buf, a.flags);
        write_optional_track(&mut buf, &a.translation);
        write_optional_track(&mut buf, &a.rotation);
        write_optional_track(&mut buf, &a.scaling);
    }

    // ── geoset anims ────────────────────────────────────────────────────
    for ga in &model.geoset_anims {
        push_u32(&mut buf, ga.geoset_id);
        write_optional_track(&mut buf, &ga.alpha_track);
    }

    // ── pivot points ────────────────────────────────────────────────────
    for p in &model.pivot_points {
        push_f32(&mut buf, p[0]);
        push_f32(&mut buf, p[1]);
        push_f32(&mut buf, p[2]);
    }

    buf
}
