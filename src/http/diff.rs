//! Unified diff infrastructure for positioned document items.
//!
//! Every item type (semantic tokens, inlay hints, …) is represented as
//! [`Item`] — an absolute `(line, character)` position plus a typed
//! [`Payload`] enum.  The diff algorithm is payload-agnostic: it finds
//! the longest common prefix (by absolute position) and suffix (by
//! delta-key — position delta to predecessor), then emits COPY/SKIP/INSERT
//! commands.
//!
//! ## Coordinate systems
//!
//! | Type | Internal (Item) | Wire format |
//! |------|-----------------|-------------|
//! | Semantic tokens | absolute `(line, char)` | delta-encoded `(Δline, Δchar)` relative to previous |
//! | Inlay hints | absolute `(line, char)` | absolute `(line, char)` |
//!
//! Conversion methods: [`Item::from_semantic_u32`] / [`Item::to_semantic_u32`],
//! [`Item::from_hints`].

use crate::http::inlay_hint::{InlayHint, InlayHintKind};

// ── Wire-format constants for semantic edit streams ──────────────────────────

/// Sentinel value — first u32 of a COPY/SKIP command tuple.
/// Cannot appear as a valid `deltaLine` in real data.
pub const SENTINEL: u32 = 0xFFFF_FFFF;
/// COPY opcode: copy N items from old array.
pub const OP_COPY: u32 = 0;
/// SKIP opcode: skip (delete) N items from old array.
pub const OP_SKIP: u32 = 1;

// ── Core types ───────────────────────────────────────────────────────────────

/// Absolute document position (0-based line and character).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub character: u32,
}

/// Typed data carried by each positioned item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Payload {
    /// Semantic token: `(length, token_type, modifiers)`.
    Semantic(u32, u32, u32),
    /// Inlay hint: `(kind, label)`.
    Hint { kind: u8, label: String },
}

/// A positioned document item with absolute coordinates — the unit of
/// diffing.
///
/// Items are expected to be sorted by `pos` (document order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub pos: Pos,
    pub data: Payload,
}

// ── Conversions ──────────────────────────────────────────────────────────────

impl Item {
    /// Decode a delta-encoded semantic token `u32` array into absolute
    /// [`Item`]s with [`Payload::Semantic`].
    ///
    /// Input layout: `[deltaLine, deltaChar, len, type, mods]` repeated.
    pub fn from_semantic_u32(raw: &[u32]) -> Vec<Item> {
        debug_assert!(
            raw.len() % 5 == 0,
            "semantic token array not aligned to 5"
        );
        let count = raw.len() / 5;
        let mut items = Vec::with_capacity(count);
        let mut line: u32 = 0;
        let mut ch: u32 = 0;
        for i in 0..count {
            let b = i * 5;
            let dl = raw[b];
            let dc = raw[b + 1];
            if dl == 0 {
                ch += dc;
            } else {
                line += dl;
                ch = dc;
            }
            items.push(Item {
                pos: Pos { line, character: ch },
                data: Payload::Semantic(raw[b + 2], raw[b + 3], raw[b + 4]),
            });
        }
        items
    }

    /// Encode [`Payload::Semantic`] items back to a delta-encoded `u32`
    /// array.  Non-`Semantic` items are silently skipped.
    #[allow(dead_code)]
    pub fn to_semantic_u32(items: &[Item]) -> Vec<u32> {
        let mut out = Vec::with_capacity(items.len() * 5);
        let mut prev = Pos::default();
        for item in items {
            let Payload::Semantic(len, tt, mods) = &item.data else {
                continue;
            };
            let dl = item.pos.line - prev.line;
            let dc = if dl == 0 {
                item.pos.character - prev.character
            } else {
                item.pos.character
            };
            out.extend_from_slice(&[dl, dc, *len, *tt, *mods]);
            prev = item.pos;
        }
        out
    }

    /// Convert [`InlayHint`]s to [`Item`]s with [`Payload::Hint`].
    ///
    /// Hint positions are already absolute — no delta decoding needed.
    #[allow(dead_code)]
    pub fn from_hints(hints: &[InlayHint]) -> Vec<Item> {
        hints
            .iter()
            .map(|h| Item {
                pos: Pos {
                    line: h.position.line as u32,
                    character: h.position.character as u32,
                },
                data: Payload::Hint {
                    kind: h.kind as u8,
                    label: h.label.clone(),
                },
            })
            .collect()
    }

    /// Convert [`Payload::Hint`] items back to [`InlayHint`]s.
    /// Non-`Hint` items are silently skipped.
    #[allow(dead_code)]
    pub fn to_hints(items: &[Item]) -> Vec<InlayHint> {
        items
            .iter()
            .filter_map(|item| {
                let Payload::Hint { kind, label } = &item.data else {
                    return None;
                };
                Some(InlayHint {
                    position: crate::http::position::Position {
                        line: item.pos.line as usize,
                        character: item.pos.character as usize,
                    },
                    label: label.clone(),
                    kind: match kind {
                        1 => InlayHintKind::Type,
                        2 => InlayHintKind::Parameter,
                        _ => InlayHintKind::None,
                    },
                    byte_offset: 0,
                })
            })
            .collect()
    }
}

// ── Diff algorithm ───────────────────────────────────────────────────────────

/// Result of diffing two [`Item`] arrays.
///
/// Describes how to transform `old` into `new` using four regions:
///
/// ```text
/// old: [ prefix |   skip   | suffix ]
/// new: [ prefix | insert   | suffix ]
/// ```
#[derive(Debug)]
pub struct Diff {
    /// Copy this many items from the start of old.
    pub prefix: usize,
    /// Skip (delete) this many items from old after the prefix.
    pub skip: usize,
    /// Insert this many items from `new[prefix..prefix+insert_count]`.
    pub insert_count: usize,
    /// Copy this many items from the end of old.
    pub suffix: usize,
}

impl Diff {
    /// `true` when `old == new` — nothing to send.
    pub fn is_noop(&self) -> bool {
        self.skip == 0 && self.insert_count == 0
    }

    /// Compare two sorted item arrays and produce a [`Diff`].
    ///
    /// - **Prefix**: matched by absolute `Pos` + `Payload` equality.
    ///   Stops precisely at the edit point regardless of line shifts.
    /// - **Suffix**: matched by **delta-key** — position delta to
    ///   predecessor plus payload equality.  This guarantees the
    ///   client's COPY is always correct without boundary fixup.
    pub fn compute(old: &[Item], new: &[Item]) -> Diff {
        let min = old.len().min(new.len());

        // ── Common prefix (absolute Pos + Payload) ───────────────
        let mut prefix = 0;
        while prefix < min && old[prefix] == new[prefix] {
            prefix += 1;
        }

        // Identical arrays
        if prefix == old.len() && prefix == new.len() {
            return Diff {
                prefix: 0,
                skip: 0,
                insert_count: 0,
                suffix: 0,
            };
        }

        // ── Common suffix (delta-key comparison, from the end) ───
        let mut suffix = 0;
        let max_suffix = min - prefix;
        while suffix < max_suffix {
            let oi = old.len() - 1 - suffix;
            let ni = new.len() - 1 - suffix;
            if !delta_key_eq(old, oi, new, ni) {
                break;
            }
            suffix += 1;
        }

        let old_mid = old.len() - prefix - suffix;
        let new_mid = new.len() - prefix - suffix;

        Diff {
            prefix,
            skip: old_mid,
            insert_count: new_mid,
            suffix,
        }
    }

    /// Encode this diff as a semantic token edit stream (5 × u32 tuples).
    ///
    /// The stream contains COPY/SKIP/INSERT commands that the client
    /// applies to its stored old delta-encoded array.
    ///
    /// Returns an empty `Vec` when [`is_noop()`](Self::is_noop).
    pub fn encode_semantic(
        &self,
        _old_items: &[Item],
        new_items: &[Item],
    ) -> Vec<u32> {
        if self.is_noop() {
            return Vec::new();
        }

        let mut out = Vec::new();

        // COPY prefix
        if self.prefix > 0 {
            out.extend_from_slice(&[SENTINEL, OP_COPY, self.prefix as u32, 0, 0]);
        }

        // SKIP deleted items from old
        if self.skip > 0 {
            out.extend_from_slice(&[SENTINEL, OP_SKIP, self.skip as u32, 0, 0]);
        }

        // INSERT new items (delta-encoded relative to predecessor)
        if self.insert_count > 0 {
            let mut prev = if self.prefix > 0 {
                new_items[self.prefix - 1].pos
            } else {
                Pos::default()
            };
            let start = self.prefix;
            let end = self.prefix + self.insert_count;
            for item in &new_items[start..end] {
                let Payload::Semantic(len, tt, mods) = &item.data else {
                    continue;
                };
                let dl = item.pos.line - prev.line;
                let dc = if dl == 0 {
                    item.pos.character - prev.character
                } else {
                    item.pos.character
                };
                out.extend_from_slice(&[dl, dc, *len, *tt, *mods]);
                prev = item.pos;
            }
        }

        // COPY suffix — always safe because delta-key matching
        // guarantees the old array's deltas produce correct absolute
        // positions relative to the output predecessor.
        if self.suffix > 0 {
            out.extend_from_slice(&[SENTINEL, OP_COPY, self.suffix as u32, 0, 0]);
        }

        out
    }
}

// ── Delta-key comparison ─────────────────────────────────────────────────────

/// Compare items at `old[oi]` and `new[ni]` by **delta-key**: the
/// position delta relative to each item's predecessor, plus payload
/// equality.
///
/// When two items match by delta-key, COPY from the old array produces
/// the correct absolute position relative to any output predecessor.
fn delta_key_eq(old: &[Item], oi: usize, new: &[Item], ni: usize) -> bool {
    // 1. Payload must match exactly.
    if old[oi].data != new[ni].data {
        return false;
    }

    // 2. Compute delta to predecessor (same encoding as semantic u32).
    let (old_prev_l, old_prev_c) = if oi > 0 {
        (old[oi - 1].pos.line, old[oi - 1].pos.character)
    } else {
        (0, 0)
    };
    let (new_prev_l, new_prev_c) = if ni > 0 {
        (new[ni - 1].pos.line, new[ni - 1].pos.character)
    } else {
        (0, 0)
    };

    let old_dl = old[oi].pos.line - old_prev_l;
    let new_dl = new[ni].pos.line - new_prev_l;
    if old_dl != new_dl {
        return false;
    }

    let old_dc = if old_dl == 0 {
        old[oi].pos.character - old_prev_c
    } else {
        old[oi].pos.character
    };
    let new_dc = if new_dl == 0 {
        new[ni].pos.character - new_prev_c
    } else {
        new[ni].pos.character
    };
    old_dc == new_dc
}

// ── Convenience ──────────────────────────────────────────────────────────────

/// Compute a semantic token diff between two delta-encoded `u32` arrays.
///
/// Returns the edit stream (empty if arrays are identical).  Drop-in
/// replacement for the old `compute_token_diff`.
pub fn semantic_diff(old_delta: &[u32], new_delta: &[u32]) -> Vec<u32> {
    let old_items = Item::from_semantic_u32(old_delta);
    let new_items = Item::from_semantic_u32(new_delta);
    let diff = Diff::compute(&old_items, &new_items);
    diff.encode_semantic(&old_items, &new_items)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply a semantic diff stream to an old delta-encoded array.
    fn apply_diff(old: &[u32], diff: &[u32]) -> Vec<u32> {
        let mut result = Vec::new();
        let mut old_cursor = 0usize;
        let mut i = 0;
        while i + 4 < diff.len() {
            if diff[i] == SENTINEL {
                let op = diff[i + 1];
                let count = diff[i + 2] as usize;
                let len = count * 5;
                if op == OP_COPY {
                    for j in 0..len {
                        if old_cursor + j < old.len() {
                            result.push(old[old_cursor + j]);
                        }
                    }
                    old_cursor += len;
                } else if op == OP_SKIP {
                    old_cursor += len;
                }
                i += 5;
            } else {
                result.extend_from_slice(&diff[i..i + 5]);
                i += 5;
            }
        }
        result
    }

    // ── Item conversions ─────────────────────────────────────────

    #[test]
    fn semantic_roundtrip() {
        let delta = vec![
            0, 5, 3, 1, 0,  // (0,5)
            0, 10, 4, 2, 0, // (0,15)
            1, 0, 5, 3, 0,  // (1,0)
            0, 3, 2, 4, 0,  // (1,3)
            2, 7, 1, 5, 0,  // (3,7)
        ];
        let items = Item::from_semantic_u32(&delta);
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].pos, Pos { line: 0, character: 5 });
        assert_eq!(items[1].pos, Pos { line: 0, character: 15 });
        assert_eq!(items[2].pos, Pos { line: 1, character: 0 });
        assert_eq!(items[3].pos, Pos { line: 1, character: 3 });
        assert_eq!(items[4].pos, Pos { line: 3, character: 7 });

        let back = Item::to_semantic_u32(&items);
        assert_eq!(back, delta);
    }

    #[test]
    fn hint_roundtrip() {
        let hints = vec![
            InlayHint {
                position: crate::http::position::Position { line: 5, character: 10 },
                label: ": integer".into(),
                kind: InlayHintKind::Type,
                byte_offset: 42,
            },
            InlayHint {
                position: crate::http::position::Position { line: 12, character: 0 },
                label: "x:".into(),
                kind: InlayHintKind::Parameter,
                byte_offset: 100,
            },
        ];
        let items = Item::from_hints(&hints);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].pos, Pos { line: 5, character: 10 });
        assert_eq!(
            items[0].data,
            Payload::Hint { kind: 1, label: ": integer".into() }
        );
        assert_eq!(items[1].pos, Pos { line: 12, character: 0 });

        let back = Item::to_hints(&items);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].position.line, 5);
        assert_eq!(back[0].label, ": integer");
        assert_eq!(back[1].position.character, 0);
    }

    // ── Diff algorithm ───────────────────────────────────────────

    #[test]
    fn identical_arrays() {
        let tokens = vec![0, 0, 5, 1, 0, 1, 0, 3, 2, 0];
        let diff = semantic_diff(&tokens, &tokens);
        assert!(diff.is_empty(), "Expected empty diff for identical arrays");
    }

    #[test]
    fn newline_insertion() {
        let old = vec![
            0, 0, 5, 1, 0, // (0,0)
            1, 0, 3, 2, 0, // (1,0)
            1, 0, 4, 3, 0, // (2,0)
        ];
        let new = vec![
            0, 0, 5, 1, 0, // (0,0)
            2, 0, 3, 2, 0, // (2,0)  — shifted
            1, 0, 4, 3, 0, // (3,0)
        ];
        let diff = semantic_diff(&old, &new);
        let reconstructed = apply_diff(&old, &diff);
        assert_eq!(reconstructed, new, "Roundtrip failed");
        assert!(diff.len() < new.len() * 2, "Diff should be compact");
    }

    #[test]
    fn token_insertion_mid_file() {
        let old = vec![
            0, 0, 5, 1, 0,
            1, 0, 5, 1, 0,
            1, 0, 5, 1, 0,
            1, 0, 5, 1, 0,
        ];
        let new = vec![
            0, 0, 5, 1, 0,
            1, 0, 5, 1, 0,
            1, 5, 3, 2, 0, // new token at (2,5)
            1, 0, 5, 1, 0, // (3,0)
            1, 0, 5, 1, 0, // (4,0)
        ];
        let diff = semantic_diff(&old, &new);
        let reconstructed = apply_diff(&old, &diff);
        assert_eq!(reconstructed, new, "Roundtrip failed for mid-file insertion");
    }

    #[test]
    fn token_deletion() {
        let old = vec![
            0, 0, 5, 1, 0,
            1, 0, 5, 2, 0,
            1, 0, 5, 3, 0,
        ];
        let new = vec![
            0, 0, 5, 1, 0,
            2, 0, 5, 3, 0,
        ];
        let diff = semantic_diff(&old, &new);
        let reconstructed = apply_diff(&old, &diff);
        assert_eq!(reconstructed, new, "Roundtrip failed for deletion");
    }

    #[test]
    fn large_file_newline_insertion() {
        let count = 1000;
        let mut old = Vec::with_capacity(count * 5);
        for i in 0..count {
            let dl = if i == 0 { 0 } else { 1 };
            old.extend_from_slice(&[dl, 0, 5, 1, 0]);
        }

        let mut new = Vec::with_capacity(count * 5);
        for i in 0..count {
            let abs_line = if i < 500 { i } else { i + 1 };
            let prev_abs = if i == 0 { 0 } else if i <= 500 { i - 1 } else { i };
            let dl = abs_line - prev_abs;
            new.extend_from_slice(&[dl as u32, 0, 5, 1, 0]);
        }

        let diff = semantic_diff(&old, &new);
        let reconstructed = apply_diff(&old, &diff);
        assert_eq!(reconstructed, new, "Roundtrip failed for large file");
        assert!(
            diff.len() < 100,
            "Expected compact diff, got {} u32s",
            diff.len()
        );
    }

    #[test]
    fn diff_struct_fields() {
        let old_items = Item::from_semantic_u32(&[
            0, 0, 5, 1, 0,
            1, 0, 3, 2, 0,
            1, 0, 4, 3, 0,
        ]);
        let new_items = Item::from_semantic_u32(&[
            0, 0, 5, 1, 0,
            2, 0, 3, 2, 0, // shifted
            1, 0, 4, 3, 0,
        ]);
        let d = Diff::compute(&old_items, &new_items);
        assert_eq!(d.prefix, 1, "prefix");
        assert_eq!(d.skip, 1, "skip");
        assert_eq!(d.insert_count, 1, "insert_count");
        assert_eq!(d.suffix, 1, "suffix");
    }

    #[test]
    fn noop_for_identical() {
        let items = Item::from_semantic_u32(&[0, 0, 5, 1, 0]);
        let d = Diff::compute(&items, &items);
        assert!(d.is_noop());
    }

    #[test]
    fn hint_diff_detects_change() {
        let old = vec![
            Item {
                pos: Pos { line: 5, character: 0 },
                data: Payload::Hint { kind: 1, label: ": int".into() },
            },
            Item {
                pos: Pos { line: 10, character: 3 },
                data: Payload::Hint { kind: 1, label: ": real".into() },
            },
        ];
        let mut new = old.clone();
        new[1] = Item {
            pos: Pos { line: 11, character: 3 },
            data: Payload::Hint { kind: 1, label: ": real".into() },
        };
        let d = Diff::compute(&old, &new);
        assert_eq!(d.prefix, 1);
        assert!(!d.is_noop());
    }

    #[test]
    fn empty_to_nonempty() {
        let old: Vec<u32> = vec![];
        let new = vec![0, 0, 5, 1, 0];
        let diff = semantic_diff(&old, &new);
        let reconstructed = apply_diff(&old, &diff);
        assert_eq!(reconstructed, new);
    }

    #[test]
    fn nonempty_to_empty() {
        let old = vec![0, 0, 5, 1, 0];
        let new: Vec<u32> = vec![];
        let diff = semantic_diff(&old, &new);
        let reconstructed = apply_diff(&old, &diff);
        assert_eq!(reconstructed, new);
    }
}

