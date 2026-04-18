use std::collections::HashMap;
use crate::http::folding::{FoldingRange, FoldingRangeKind};
use crate::http::range::Range;
use crate::http::ref_map::DeclKey;
use crate::lng::jass::ast::{Id, Param};
use crate::lng::symbol::ParamSym;
use crate::util::roper::node::NodeExt;
use tree_sitter::Node;
use super::{Cursor, VarInfo};

impl Cursor {
    pub(super) fn node_text(&self, node: &Node) -> String {
        node.text(&self.rope).to_string()
    }

    pub(super) fn push_fold_region(&mut self, node: &Node) {
        let sr = node.start_position().row;
        let er = node.end_position().row;
        if er > sr {
            self.folding.push(FoldingRange {
                start_line: sr,
                end_line: er,
                kind: Some(FoldingRangeKind::Region),
                ..Default::default()
            });
        }
    }

    pub(super) fn flush_comment_run(&mut self) {
        if let Some(s) = self.comment_start.take() {
            if self.comment_end > s {
                self.folding.push(FoldingRange {
                    start_line: s,
                    end_line: self.comment_end,
                    kind: Some(FoldingRangeKind::Comment),
                    ..Default::default()
                });
            }
        }
    }

    pub(super) fn register_id(&mut self, id: &Option<Id>) {
        if let Some(id) = id {
            self.id_roles.insert(id.node.start_byte(), id.role);
        }
    }

    pub(super) fn id_name(&self, id: &Option<Id>) -> String {
        id.as_ref()
            .map(|id| self.node_text(&id.node))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<unnamed>".into())
    }

    pub(super) fn id_sel_range(&self, id: &Option<Id>, fallback: &Node) -> Range {
        id.as_ref()
            .map(|id| id.node.to_range(&self.rope))
            .unwrap_or_else(|| fallback.to_range(&self.rope))
    }

    pub(super) fn scope_define(
        vars: &mut HashMap<String, VarInfo>,
        name: &str,
        start_byte: usize,
        type_name: Option<String>,
        is_array: bool,
        is_constant: bool,
        is_initialized: bool,
        is_param: bool,
    ) {
        vars.insert(
            name.to_string(),
            VarInfo { start_byte, type_name, is_array, is_constant, is_initialized, is_param },
        );
    }

    pub(super) fn next_decl_index(&mut self) -> usize {
        let idx = self.decl_counter;
        self.decl_counter += 1;
        idx
    }

    pub(super) fn alloc_key(&mut self) -> DeclKey {
        let key = self.next_decl_key;
        self.next_decl_key += 1;
        key
    }

    pub(super) fn params_to_sym(&self, params: &[Param]) -> Vec<ParamSym> {
        params
            .iter()
            .map(|p| ParamSym {
                name: self.id_name(&p.name),
                type_name: self.id_name(&p.type_id),
            })
            .collect()
    }

    pub(super) fn record_callee(&mut self, name: &str) {
        if let Some(ref mut callees) = self.current_callees {
            callees.insert(name.to_string());
        } else {
            self.bare_callees.insert(name.to_string());
        }
    }

    pub(super) fn record_func_ref(&mut self, name: &str) {
        self.file_symbols.func_refs.insert(name.to_string());
    }
}

