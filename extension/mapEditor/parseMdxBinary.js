'use strict'

/**
 * Parse the binary MDX render response into a structured JS object.
 *
 * Layout matches `src/lng/mdx/response.rs` — see the doc comment there
 * for the full specification.
 *
 * @param {Buffer} buf  Raw binary response from POST /render/mdx
 * @returns {object}    Structured model data with TypedArrays for geometry
 */
function parseMdxBinary(buf) {
    const ab = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength)
    const dv = new DataView(ab)
    let o = 0

    // ── read helpers ────────────────────────────────────────────────
    function u8() { const v = dv.getUint8(o); o += 1; return v }
    function u32() { const v = dv.getUint32(o, true); o += 4; return v }
    function i32() { const v = dv.getInt32(o, true); o += 4; return v }
    function f32() { const v = dv.getFloat32(o, true); o += 4; return v }

    function str() {
        const len = u32()
        const bytes = new Uint8Array(ab, o, len)
        o += len
        return Buffer.from(bytes).toString('utf8')
    }

    function readFloat32Array(byteLen) {
        // Copy into a new aligned buffer for Float32Array
        const copy = new ArrayBuffer(byteLen)
        new Uint8Array(copy).set(new Uint8Array(ab, o, byteLen))
        o += byteLen
        return new Float32Array(copy)
    }

    function readUint16Array(byteLen) {
        const copy = new ArrayBuffer(byteLen)
        new Uint8Array(copy).set(new Uint8Array(ab, o, byteLen))
        o += byteLen
        return new Uint16Array(copy)
    }

    function readUint8Array(len) {
        const arr = new Uint8Array(ab, o, len).slice() // copy
        o += len
        return arr
    }

    function readUint32Array(count) {
        const byteLen = count * 4
        const copy = new ArrayBuffer(byteLen)
        new Uint8Array(copy).set(new Uint8Array(ab, o, byteLen))
        o += byteLen
        return new Uint32Array(copy)
    }

    // ── anim track ──────────────────────────────────────────────────
    function readOptionalTrack() {
        const has = u8()
        if (!has) return null
        return readTrack()
    }

    function readTrack() {
        const line_type = u32()
        const global_seq_id = i32()
        const num_kf = u32()
        const value_size = u32()
        const keyframes = []
        for (let i = 0; i < num_kf; i++) {
            const frame = u32()
            const value = []
            for (let j = 0; j < value_size; j++) value.push(f32())
            let in_tan = [], out_tan = []
            if (line_type >= 2) {
                for (let j = 0; j < value_size; j++) in_tan.push(f32())
                for (let j = 0; j < value_size; j++) out_tan.push(f32())
            }
            keyframes.push({frame, value, in_tan, out_tan})
        }
        return {line_type, global_seq_id, keyframes}
    }

    // ── node (bone / helper / attachment) ───────────────────────────
    function readNode() {
        const name = str()
        const object_id = u32()
        const parent_id = u32()
        const flags = u32()
        const translation = readOptionalTrack()
        const rotation = readOptionalTrack()
        const scaling = readOptionalTrack()
        return {name, object_id, parent_id, flags, translation, rotation, scaling}
    }

    // ── header ──────────────────────────────────────────────────────
    const version = u32()
    const name = str()
    const file_size = u32()
    const total_vertices = u32()
    const total_faces = u32()
    const num_geosets = u32()
    const num_sequences = u32()
    const num_global_sequences = u32()
    const num_textures = u32()
    const num_materials = u32()
    const num_bones = u32()
    const num_helpers = u32()
    const num_attachments = u32()
    const num_geoset_anims = u32()
    const num_pivot_points = u32()

    // ── geosets ─────────────────────────────────────────────────────
    const geosets = []
    for (let i = 0; i < num_geosets; i++) {
        const material_id = u32()
        const vertex_count = u32()
        const face_count = u32()

        const vBytes = u32(); const vertices = readFloat32Array(vBytes)
        const nBytes = u32(); const normals = readFloat32Array(nBytes)
        const fBytes = u32(); const faces = readUint16Array(fBytes)
        const uBytes = u32(); const uvs = readFloat32Array(uBytes)

        const vgLen = u32(); const vertex_groups = readUint8Array(vgLen)
        const miCount = u32(); const matrix_ids = readUint32Array(miCount)
        const mgCount = u32(); const matrix_group_counts = readUint32Array(mgCount)

        geosets.push({
            material_id, vertex_count, face_count,
            vertices, normals, faces, uvs,
            vertex_groups, matrix_ids, matrix_group_counts,
        })
    }

    // ── sequences ───────────────────────────────────────────────────
    const sequences = []
    for (let i = 0; i < num_sequences; i++) {
        const sname = str()
        const interval_start = u32()
        const interval_end = u32()
        const move_speed = f32()
        const non_looping = u32() !== 0
        const rarity = f32()
        sequences.push({name: sname, interval_start, interval_end, move_speed, non_looping, rarity})
    }

    // ── global sequences ────────────────────────────────────────────
    const global_sequences = []
    for (let i = 0; i < num_global_sequences; i++) global_sequences.push(u32())

    // ── textures ────────────────────────────────────────────────────
    const textures = []
    for (let i = 0; i < num_textures; i++) {
        const replaceable_id = u32()
        const file_name = str()
        const flags = u32()
        textures.push({replaceable_id, file_name, flags})
    }

    // ── materials ───────────────────────────────────────────────────
    const materials = []
    for (let i = 0; i < num_materials; i++) {
        const priority_plane = u32()
        const flags = u32()
        const num_layers = u32()
        const layers = []
        for (let j = 0; j < num_layers; j++) {
            const filter_mode = u32()
            const shading_flags = u32()
            const texture_id = u32()
            const alpha = f32()
            layers.push({filter_mode, shading_flags, texture_id, alpha})
        }
        materials.push({priority_plane, flags, layers})
    }

    // ── bones ───────────────────────────────────────────────────────
    const bones = []
    for (let i = 0; i < num_bones; i++) bones.push(readNode())

    // ── helpers ─────────────────────────────────────────────────────
    const helpers = []
    for (let i = 0; i < num_helpers; i++) helpers.push(readNode())

    // ── attachments ─────────────────────────────────────────────────
    const attachments = []
    for (let i = 0; i < num_attachments; i++) attachments.push(readNode())

    // ── geoset anims ────────────────────────────────────────────────
    const geoset_anims = []
    for (let i = 0; i < num_geoset_anims; i++) {
        const geoset_id = u32()
        const alpha_track = readOptionalTrack()
        geoset_anims.push({geoset_id, alpha_track})
    }

    // ── pivot points ────────────────────────────────────────────────
    const pivot_points = []
    for (let i = 0; i < num_pivot_points; i++) {
        pivot_points.push([f32(), f32(), f32()])
    }

    return {
        version, name, size: file_size,
        total_vertices, total_faces,
        geosets, sequences, global_sequences,
        textures, materials,
        bones, helpers, attachments,
        geoset_anims, pivot_points,
    }
}

module.exports = {parseMdxBinary}

