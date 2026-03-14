//! Unified reference map for a single file.
//!
//! Built once during parse from `Cursor` output and stored in
//! `REF_URI_MAP`.  Serves four LSP features from the same data:
//!
//! * **documentHighlight** — all occurrences of the symbol under cursor
//! * **definition**        — jump to the declaration
//! * **references**        — every usage (incl. declaration)
//! * **rename**            — same as references, used to compute edits

use crate::lsp::highlight::lsp::DocumentHighlightKind;
use crate::lsp::range::Range;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

// ─── Global storage ──────────────────────────────────────────────────────────

/// Per-file reference map.  Populated from `Cursor` in `parse.rs`.
pub static REF_URI_MAP: Lazy<DashMap<Url, RefMap>> = Lazy::new(DashMap::new);

// ─── Types ───────────────────────────────────────────────────────────────────

/// Declaration key — `start_byte` of the declaring node for local symbols,
/// or a synthetic value (>= `EXTERNAL_KEY_BASE`) for imported symbols.
pub type DeclKey = usize;

/// Base value for synthetic DeclKeys assigned to imported symbols.
/// Local `start_byte` values are always smaller than any real file,
/// so `usize::MAX / 2` provides a collision-free partition.
pub const EXTERNAL_KEY_BASE: usize = usize::MAX / 2;

/// An imported symbol whose declaration lives in another file.
/// Stored in [`RefMap::external_decls`] for cross-file go-to-definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDecl {
    /// URI of the file that contains the declaration.
    pub uri: Url,
    /// Symbol name (function / native / type / global).
    pub name: String,
    /// DeclKey of this symbol in the origin file's RefMap (if known).
    /// Allows displaying the origin file's internal reference ID.
    pub origin_decl_key: Option<DeclKey>,
}

/// A single occurrence of an identifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Occurrence {
    /// LSP range of this occurrence.
    pub range: Range,
    /// Read / Write / Text.
    pub kind: DocumentHighlightKind,
    /// `true` for the *declaring* occurrence (the one definition jumps to).
    pub is_decl: bool,
}

/// A group of occurrences that all refer to the same declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefGroup {
    /// The name of the symbol.
    pub name: String,
    /// All occurrences, declaration first (by convention).
    pub occurrences: Vec<Occurrence>,
}

/// An entry in the sorted span index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub decl_key: DeclKey,
    /// `true` when this span refers to a symbol resolved via import
    /// (its `decl_key` lives in `external_decls`).
    pub is_external: bool,
    /// LSP range — stored here so we can return it without a Rope.
    pub range: Range,
}

/// All reference data for a single file.
#[derive(Default, Serialize, Deserialize)]
pub struct RefMap {
    /// DeclKey → group of occurrences.
    pub groups: HashMap<DeclKey, RefGroup>,
    /// Sorted by `start_byte` for O(log n) lookup.
    pub spans: Vec<Span>,
    /// Synthetic DeclKey → external declaration info (for cross-file
    /// go-to-definition on imported symbols).
    pub external_decls: HashMap<DeclKey, ExternalDecl>,
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/// Raw occurrence collected during `Cursor::walk`.
#[derive(Debug, Clone)]
pub struct RawOccurrence {
    pub range: Range,
    pub kind: DocumentHighlightKind,
    pub is_decl: bool,
}

/// Build a `RefMap` from the raw data produced by `Cursor`.
pub fn build_ref_map(
    raw_groups: HashMap<DeclKey, Vec<RawOccurrence>>,
    names: HashMap<DeclKey, String>,
    external_decls: HashMap<DeclKey, ExternalDecl>,
    rope: &lapce_xi_rope::Rope,
) -> RefMap {
    let mut groups = HashMap::new();
    let mut spans: Vec<Span> = Vec::new();

    for (&key, raw_occs) in &raw_groups {
        let name = names.get(&key).cloned().unwrap_or_default();
        let mut occurrences = Vec::with_capacity(raw_occs.len());

        for raw in raw_occs {
            if let (Some(start), Some(end)) = (
                raw.range.start.to_byte_offset(rope),
                raw.range.end.to_byte_offset(rope),
            ) {
                spans.push(Span {
                    start_byte: start,
                    end_byte: end,
                    decl_key: key,
                    is_external: external_decls.contains_key(&key),
                    range: raw.range.clone(),
                });
            }
            occurrences.push(Occurrence {
                range: raw.range.clone(),
                kind: raw.kind,
                is_decl: raw.is_decl,
            });
        }

        groups.insert(key, RefGroup { name, occurrences });
    }

    spans.sort_by_key(|s| s.start_byte);
    RefMap { groups, spans, external_decls }
}

// ─── Queries ─────────────────────────────────────────────────────────────────

impl RefMap {
    /// Find the span at `byte_offset`.
    fn span_at(&self, byte_offset: usize) -> Option<&Span> {
        let idx = self.spans.partition_point(|s| s.start_byte <= byte_offset);
        if idx == 0 {
            return None;
        }
        let span = &self.spans[idx - 1];
        if byte_offset >= span.start_byte && byte_offset < span.end_byte {
            Some(span)
        } else {
            None
        }
    }

    /// `DeclKey` for the identifier at `byte_offset`.
    pub fn decl_key_at(&self, byte_offset: usize) -> Option<DeclKey> {
        self.span_at(byte_offset).map(|s| s.decl_key)
    }

    /// LSP range of the token at `byte_offset` (for `prepareRename`).
    pub fn range_at(&self, byte_offset: usize) -> Option<&Range> {
        self.span_at(byte_offset).map(|s| &s.range)
    }

    /// Name of the symbol at `byte_offset`.
    pub fn name_at(&self, byte_offset: usize) -> Option<&str> {
        let key = self.decl_key_at(byte_offset)?;
        self.groups.get(&key).map(|g| g.name.as_str())
    }

    /// All occurrences (for highlight / references).
    pub fn occurrences_at(&self, byte_offset: usize) -> &[Occurrence] {
        match self.decl_key_at(byte_offset) {
            Some(key) => self
                .groups
                .get(&key)
                .map(|g| g.occurrences.as_slice())
                .unwrap_or(&[]),
            None => &[],
        }
    }

    /// Declaration occurrence(s) only (for go-to-definition).
    pub fn definitions_at(&self, byte_offset: usize) -> Vec<&Occurrence> {
        match self.decl_key_at(byte_offset) {
            Some(key) => self
                .groups
                .get(&key)
                .map(|g| g.occurrences.iter().filter(|o| o.is_decl).collect())
                .unwrap_or_default(),
            None => vec![],
        }
    }

    /// If the symbol at `byte_offset` is an imported (external) declaration,
    /// return its [`ExternalDecl`].
    pub fn external_at(&self, byte_offset: usize) -> Option<&ExternalDecl> {
        let key = self.decl_key_at(byte_offset)?;
        self.external_decls.get(&key)
    }
}
