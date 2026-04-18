//! Statement visitor — processes each [`Statement`] variant during the AST walk.
//!
//! Extracted from `mod.rs` to keep the main module focused on the Cursor struct,
//! initialization, phase-1/phase-2 walk, and helper methods.

use std::collections::{HashMap, HashSet};

use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::document_symbol::{DocumentSymbol, SymbolKind};
use crate::http::highlight::DocumentHighlightKind;
use crate::http::position::Position;
use crate::lng::jass::ast::*;
use crate::lng::jass::type_map::{ComptimeValue, DeclType, FuncType, ParamPair, TypeDeclInfo, VarType};
use crate::lng::symbol::{FunctionSym, GlobalVarSym, NativeSym, TypeSym};
use crate::util::roper::node::NodeExt;

use super::{extract_annotations, Cursor, Scope, VarInfo};

impl Cursor {
    // ─── statement list visitor ──────────────────────────────────────────

    pub(super) fn visit_stmts(
        &mut self,
        stmts: &[Statement],
        vars: &mut Vec<HashMap<String, VarInfo>>,
    ) -> Vec<DocumentSymbol> {
        let mut syms = Vec::new();
        for stmt in stmts {
            if let Some(sym) = self.visit_stmt(stmt, vars) {
                syms.push(sym);
            }
        }
        self.flush_comment_run();
        syms
    }

    // ─── single statement visitor ────────────────────────────────────────

    pub(super) fn visit_stmt(
        &mut self,
        stmt: &Statement,
        vars: &mut Vec<HashMap<String, VarInfo>>,
    ) -> Option<DocumentSymbol> {
        // Import directives — skip comment tracking, add dedicated semantic
        if let Statement::Import(imp) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(imp.node.start_byte());
            crate::lng::directive::visit_import_semantic(
                imp,
                &mut self.semantic,
                &mut self.diagnostics,
                &self.rope,
            );
            return None;
        }

        // SetDir directives — skip comment tracking, add dedicated semantic
        if let Statement::SetDir(sd) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(sd.node.start_byte());
            crate::lng::directive::visit_set_semantic(
                sd,
                &mut self.semantic,
                &mut self.diagnostics,
                &mut self.file_settings,
                &self.rope,
            );

            return None;
        }

        // IgnoreDir directives — skip comment tracking, add dedicated semantic
        if let Statement::IgnoreDir(ig) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(ig.node.start_byte());
            crate::lng::directive::visit_ignore_semantic(
                ig,
                &mut self.semantic,
                &mut self.diagnostics,
                &mut self.file_ignore_tags,
                &self.rope,
            );
            return None;
        }

        // UjapiImport directives — skip comment tracking, add dedicated semantic
        if let Statement::UjapiImport(ud) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(ud.node.start_byte());
            crate::lng::directive::visit_ujapi_semantic(
                ud,
                &mut self.semantic,
                &mut self.diagnostics,
                &self.rope,
            );
            return None;
        }

        // EntryDir directives — skip comment tracking, add dedicated semantic
        if let Statement::EntryDir(ed) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(ed.node.start_byte());
            crate::lng::directive::visit_entry_semantic(
                ed,
                &mut self.semantic,
                &self.rope,
            );
            self.file_symbols.is_entry = true;
            return None;
        }

        // Comment tracking
        if let Statement::Comment(c) = stmt {
            let row = c.node.start_position().row;
            match self.comment_start {
                Some(_) => self.comment_end = row,
                None => {
                    self.comment_start = Some(row);
                    self.comment_end = row;
                }
            }
            return None;
        }
        self.flush_comment_run();

        match stmt {
            Statement::Type(t) => {
                self.register_id(&t.name);
                self.register_id(&t.base);
                let name = self.id_name(&t.name);
                let decl_index = self.next_decl_index();
                let decl_key = if let Some(ref name_id) = t.name {
                    Some(self.hl_declare_type(&name, &name_id.node))
                } else {
                    None
                };
                if let Some(ref base_id) = t.base {
                    let bname = self.node_text(&base_id.node);
                    self.hl_reference_type(&bname, &base_id.node, DocumentHighlightKind::Read);
                }
                // TypeMap: record type declaration
                if let Some(key) = decl_key {
                    self.type_map.insert(key, DeclType::Type(TypeDeclInfo {
                        base: t.base.as_ref().map(|id| self.node_text(&id.node)),
                    }));
                }
                let ann = extract_annotations(&self.rope, t.node.start_position().row);
                self.file_symbols.types.push(TypeSym {
                    name: name.clone(),
                    base: t.base.as_ref().map(|id| self.node_text(&id.node)),
                    decl_index,
                    doc_comment: ann.doc_comment,
                    ignore_tags: ann.ignore_tags,
                });
                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Class,
                    range: t.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&t.name, &t.node),
                    ..Default::default()
                })
            }

            Statement::Native(n) => {
                self.register_id(&n.name);
                for p in &n.params {
                    self.register_id(&p.type_id);
                    self.register_id(&p.name);
                }
                self.register_id(&n.return_type);
                let name = self.id_name(&n.name);
                let decl_index = self.next_decl_index();
                let native_decl_key = if let Some(ref name_id) = n.name {
                    Some(self.hl_declare_func(&name, &name_id.node))
                } else {
                    None
                };
                // hl: reference parameter types & declare param vars for TypeMap
                let mut param_pairs = Vec::new();
                for p in &n.params {
                    if let Some(ref tid) = p.type_id {
                        let tname = self.node_text(&tid.node);
                        self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                    }
                    let pname = self.id_name(&p.name);
                    let ptype = p.type_id.as_ref().map(|id| self.node_text(&id.node)).unwrap_or_default();
                    param_pairs.push(ParamPair { name: pname, type_name: ptype });
                }
                // hl: reference return type
                if let Some(ref rt_id) = n.return_type {
                    let rt_name = self.node_text(&rt_id.node);
                    self.hl_reference_type(&rt_name, &rt_id.node, DocumentHighlightKind::Read);
                }
                // TypeMap: record native signature
                let return_type = n.return_type.as_ref().map(|id| self.node_text(&id.node));
                if let Some(key) = native_decl_key {
                    self.type_map.insert(key, DeclType::Func(FuncType {
                        params: param_pairs,
                        return_type: return_type.clone(),
                    }));
                }
                let ann = extract_annotations(&self.rope, n.node.start_position().row);
                self.file_symbols.natives.push(NativeSym {
                    name: name.clone(),
                    params: self.params_to_sym(&n.params),
                    return_type: return_type.clone(),
                    is_constant: n.is_constant,
                    decl_index,
                    doc_comment: ann.doc_comment,
                    ignore_tags: ann.ignore_tags,
                });
                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Interface,
                    range: n.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&n.name, &n.node),
                    ..Default::default()
                })
            }

            Statement::Function(f) => {
                self.register_id(&f.name);
                self.register_id(&f.return_type);
                self.push_fold_region(&f.node);

                let func_name = self.id_name(&f.name);
                let decl_index = self.next_decl_index();
                let param_syms = self.params_to_sym(&f.params);
                let return_type = f.return_type.as_ref().map(|id| self.node_text(&id.node));

                // hl: declare function name in the current (global) scope
                let func_decl_key = if let Some(ref name_id) = f.name {
                    Some(self.hl_declare_func(&func_name, &name_id.node))
                } else {
                    None
                };
                // hl: reference return type
                if let Some(ref rt_id) = f.return_type {
                    let rt_name = self.node_text(&rt_id.node);
                    self.hl_reference_type(&rt_name, &rt_id.node, DocumentHighlightKind::Read);
                }

                // Start collecting callees for this function.
                self.current_callees = Some(HashSet::new());

                // hl: push function scope for params and locals
                self.hl_push_scope();

                let mut func_vars = HashMap::new();
                let mut children = Vec::new();
                let mut param_pairs = Vec::new();

                for p in &f.params {
                    self.register_id(&p.type_id);
                    self.register_id(&p.name);
                    if let Some(name_id) = &p.name {
                        let pname = self.node_text(&name_id.node);
                        let type_name = p.type_id.as_ref().map(|id| self.node_text(&id.node));
                        // hl: reference param type
                        if let Some(ref tid) = p.type_id {
                            let tname = self.node_text(&tid.node);
                            self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                        }
                        // hl: declare param
                        let param_key = self.hl_declare_var(&pname, &name_id.node);
                        self.arg_decl_keys.insert(param_key);
                        // TypeMap: record parameter
                        self.type_map.insert(param_key, DeclType::Var(VarType {
                            name: type_name.clone().unwrap_or_default(),
                            is_array: false,
                            is_constant: false,
                            is_comptime: false,
                        }));
                        param_pairs.push(ParamPair {
                            name: pname.clone(),
                            type_name: type_name.clone().unwrap_or_default(),
                        });
                        Self::scope_define(
                            &mut func_vars,
                            &self.node_text(&name_id.node),
                            name_id.node.start_byte(),
                            type_name.clone(),
                            false, false, true, true,
                        );
                        children.push(DocumentSymbol {
                            name: self.node_text(&name_id.node),
                            detail: type_name,
                            kind: SymbolKind::Variable,
                            range: p.node.to_range(&self.rope),
                            selection_range: name_id.node.to_range(&self.rope),
                            ..Default::default()
                        });
                    }
                }

                // TypeMap: record function signature
                if let Some(key) = func_decl_key {
                    self.type_map.insert(key, DeclType::Func(FuncType {
                        params: param_pairs,
                        return_type: return_type.clone(),
                    }));
                }

                // Track the declared return type so `Statement::Return`
                // can check type compatibility.
                let old_return_type = self.current_return_type.take();
                self.current_return_type = Some(
                    return_type.clone().unwrap_or_else(|| "nothing".to_string()),
                );

                vars.push(func_vars);
                children.extend(self.visit_stmts(&f.body, vars));
                let func_vars = vars.pop().unwrap_or_default();

                // Restore the previous return type (for nested functions if any).
                self.current_return_type = old_return_type;

                let ann = extract_annotations(&self.rope, f.node.start_position().row);

                // Handle leak detection (respects //@ignore leak on the function)
                if !ann.ignore_tags.contains("leak") {
                    self.check_handle_leaks(&f.body, &func_vars, &f.node, &func_name);
                }

                // Redundant if-return simplification check.
                {
                    let body = f.body.clone();
                    self.check_redundant_if_return(&body);
                }

                // Empty else detection.
                {
                    let body = f.body.clone();
                    self.check_empty_else(&body);
                }

                // And-chain collapse check.
                {
                    let body = f.body.clone();
                    self.check_and_chains(&body);
                }

                // Or-chain collapse check.
                {
                    let body = f.body.clone();
                    self.check_or_chains(&body);
                }

                // Dead code detection.
                {
                    let body = f.body.clone();
                    self.check_dead_code(&body);
                }

                // hl: pop function scope
                self.hl_pop_scope();

                // Finalize callee collection.
                let callees = self.current_callees.take().unwrap_or_default();

                // Detect inline candidate: takes nothing + single `return expr`.
                let is_single_return = f.params.is_empty()
                    && f.body.len() == 1
                    && matches!(&f.body[0], Statement::Return(r) if r.value.is_some());

                let (inline_return_text, inline_is_compound) = if is_single_return {
                    if let Statement::Return(r) = &f.body[0] {
                        if let Some(ref expr) = r.value {
                            let node = expr.cst_node();
                            let text = self.rope.slice_to_cow(node.start_byte()..node.end_byte());
                            let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                            let compound = matches!(expr, Expr::Binary { .. } | Expr::Unary { .. });
                            (Some(flat), compound)
                        } else {
                            (None, false)
                        }
                    } else {
                        (None, false)
                    }
                } else {
                    (None, false)
                };

                self.file_symbols.functions.push(FunctionSym {
                    name: func_name.clone(),
                    params: param_syms,
                    return_type,
                    namespace: String::new(),
                    decl_byte: 0,
                    is_constant: f.is_constant,
                    decl_index,
                    callees,
                    doc_comment: ann.doc_comment,
                    ignore_tags: ann.ignore_tags,
                    is_single_return,
                    inline_return_text,
                    inline_is_compound,
                });

                self.scopes.push(Scope {
                    name: func_name.clone(),
                    vars: func_vars,
                });

                Some(DocumentSymbol {
                    name: func_name,
                    kind: SymbolKind::Function,
                    range: f.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&f.name, &f.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }

            Statement::Globals(g) => {
                self.push_fold_region(&g.node);
                let mut children = Vec::new();
                vars.push(HashMap::new());

                for v in &g.vars {
                    self.register_id(&v.type_id);
                    let type_name = v.type_id.as_ref().map(|id| self.node_text(&id.node));
                    // hl: reference the type
                    if let Some(ref tid) = v.type_id {
                        let tname = self.node_text(&tid.node);
                        self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                    }

                    for d in &v.decls {
                        self.register_id(&d.name);
                        let expr_type = if let Some(expr) = &d.value {
                            self.check_expr_hints(expr);
                            self.visit_expr(expr)
                        } else {
                            None
                        };
                        let var_name = self.id_name(&d.name);
                        let decl_index = self.next_decl_index();
                        let var_decl_key = if let Some(name_id) = &d.name {
                            // hl: declare global variable
                            let key = self.hl_declare_var(&var_name, &name_id.node);
                            self.var_decl_keys.insert(key);
                            Self::scope_define(
                                vars.last_mut().unwrap(),
                                &self.node_text(&name_id.node),
                                name_id.node.start_byte(),
                                type_name.clone(),
                                v.is_array, v.is_constant, d.value.is_some(), false,
                            );
                            Some(key)
                        } else {
                            None
                        };
                        // Type mismatch check: unknown → concrete type
                        if let (Some(tn), Some(et)) = (&type_name, &expr_type) {
                            self.check_type_mismatch(tn, Some(et.as_str()), &d.node);
                        }
                        let ann = extract_annotations(&self.rope, v.node.start_position().row);
                        self.file_symbols.globals.push(GlobalVarSym {
                            name: var_name.clone(),
                            type_name: type_name.clone(),
                            namespace: String::new(),
                            decl_byte: 0,
                            is_constant: v.is_constant,
                            is_array: v.is_array,
                            has_initializer: d.value.is_some(),
                            decl_index,
                            doc_comment: ann.doc_comment,
                            ignore_tags: ann.ignore_tags,
                        });
                        // type hint: show type with const/comptime/array modifiers
                        if let Some(name_id) = &d.name {
                            if let Some(ref tn) = type_name {
                                let cv = d.value.as_ref().and_then(|e| self.eval_expr(e));
                                let is_comptime = v.is_constant && cv.is_some();
                                if is_comptime {
                                    if let Some(ref val) = cv {
                                        self.comptime_values.insert(var_name.clone(), val.clone());
                                    }
                                }
                                // TypeMap: record global variable type
                                if let Some(key) = var_decl_key {
                                    self.type_map.insert(key, DeclType::Var(VarType {
                                        name: tn.clone(),
                                        is_array: v.is_array,
                                        is_constant: v.is_constant,
                                        is_comptime,
                                    }));
                                }
                                let label = Self::build_type_label(
                                    tn, v.is_constant, is_comptime, v.is_array,
                                );
                                self.emit_type_hint(&name_id.node, &label, cv.as_ref());
                            }
                        }
                        children.push(DocumentSymbol {
                            name: var_name,
                            detail: type_name.clone(),
                            kind: if v.is_constant { SymbolKind::Constant } else { SymbolKind::Variable },
                            range: d.node.to_range(&self.rope),
                            selection_range: self.id_sel_range(&d.name, &d.node),
                            ..Default::default()
                        });
                    }
                }

                let globals_vars = vars.pop().unwrap_or_default();
                self.scopes.push(Scope {
                    name: "globals".into(),
                    vars: globals_vars,
                });

                Some(DocumentSymbol {
                    name: "globals".into(),
                    kind: SymbolKind::Namespace,
                    range: g.node.to_range(&self.rope),
                    selection_range: g.node.to_range(&self.rope),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }

            Statement::Local(l) => {
                self.register_id(&l.type_id);
                self.register_id(&l.name);
                // hl: reference the type
                if let Some(ref tid) = l.type_id {
                    let tname = self.node_text(&tid.node);
                    self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                }
                let expr_type = if let Some(expr) = &l.value {
                    self.check_expr_hints(expr);
                    self.visit_expr(expr)
                } else {
                    None
                };
                // Type mismatch check: unknown → concrete type
                if let Some(ref tid) = l.type_id {
                    let tn = self.node_text(&tid.node);
                    if let Some(ref et) = expr_type {
                        self.check_type_mismatch(&tn, Some(et.as_str()), &l.node);
                    }
                }
                if let (Some(scope), Some(name_id)) = (vars.last_mut(), &l.name) {
                    let lname = self.node_text(&name_id.node);
                    // hl: declare local variable
                    let local_key = self.hl_declare_var(&lname, &name_id.node);
                    self.var_decl_keys.insert(local_key);
                    // TypeMap: record local variable type
                    if let Some(ref tid) = l.type_id {
                        let tn = self.node_text(&tid.node);
                        self.type_map.insert(local_key, DeclType::Var(VarType {
                            name: tn.clone(),
                            is_array: l.is_array,
                            is_constant: false,
                            is_comptime: false,
                        }));
                        // type hint: show local type + comptime value of initializer
                        let cv = l.value.as_ref().and_then(|e| self.eval_expr(e));
                        self.emit_type_hint(&name_id.node, &tn, cv.as_ref());
                    }
                    Self::scope_define(
                        scope,
                        &self.node_text(&name_id.node),
                        name_id.node.start_byte(),
                        l.type_id.as_ref().map(|id| self.node_text(&id.node)),
                        l.is_array, false, l.value.is_some(), false,
                    );
                    // ── Array initializer diagnostic ───────────────────
                    if l.is_array && l.value.is_some() {
                        let remove_start = name_id.node.end_byte();
                        let remove_end = l.node.end_byte();
                        self.diagnostics.push(Diagnostic {
                            range: l.node.to_range(&self.rope),
                            message: crate::util::i18n::array_no_init(&lname),
                            severity: Some(DiagnosticSeverity::Error),
                            data: Some(serde_json::json!({
                                "array_no_init_remove_start": remove_start,
                                "array_no_init_remove_end": remove_end,
                            })),
                            ..Diagnostic::new("jass", "array-no-init")
                        });
                    }
                }
                Some(DocumentSymbol {
                    name: self.id_name(&l.name),
                    detail: l.type_id.as_ref().map(|id| self.node_text(&id.node)),
                    kind: SymbolKind::Variable,
                    range: l.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&l.name, &l.node),
                    ..Default::default()
                })
            }

            Statement::VarStmt(v) => {
                // Inside a function body → treat as local variable declaration
                // (no `local` keyword required). In global scope → global variable.
                let in_function = self.current_callees.is_some();

                self.register_id(&v.type_id);
                let type_name = v.type_id.as_ref().map(|id| self.node_text(&id.node));
                // hl: reference the type
                if let Some(ref tid) = v.type_id {
                    let tname = self.node_text(&tid.node);
                    self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                }
                for d in &v.decls {
                    self.register_id(&d.name);
                    let expr_type = if let Some(expr) = &d.value {
                        self.check_expr_hints(expr);
                        self.visit_expr(expr)
                    } else {
                        None
                    };
                    let var_name = self.id_name(&d.name);
                    let decl_index = self.next_decl_index();
                    let var_decl_key = if let Some(ref name_id) = d.name {
                        let vname = self.node_text(&name_id.node);
                        // hl: declare variable
                        let key = self.hl_declare_var(&vname, &name_id.node);
                        self.var_decl_keys.insert(key);
                        // If inside a function, register in the local scope vars map
                        if in_function {
                            if let Some(scope) = vars.last_mut() {
                                Self::scope_define(
                                    scope,
                                    &vname,
                                    name_id.node.start_byte(),
                                    type_name.clone(),
                                    v.is_array, v.is_constant, d.value.is_some(), false,
                                );
                            }
                        }
                        Some(key)
                    } else {
                        None
                    };
                    if !in_function {
                        // Export to file_symbols so the scope resolver makes
                        // this variable visible to importing files.
                        let ann = extract_annotations(&self.rope, v.node.start_position().row);
                        self.file_symbols.globals.push(GlobalVarSym {
                            name: var_name.clone(),
                            type_name: type_name.clone(),
                            namespace: String::new(),
                            decl_byte: 0,
                            is_constant: v.is_constant,
                            is_array: v.is_array,
                            has_initializer: d.value.is_some(),
                            decl_index,
                            doc_comment: ann.doc_comment,
                            ignore_tags: ann.ignore_tags,
                        });
                    }
                    // TypeMap + type hint
                    if let Some(ref name_id) = d.name {
                        if let Some(ref tn) = type_name {
                            // Type mismatch check: unknown → concrete type
                            if let Some(ref et) = expr_type {
                                self.check_type_mismatch(tn, Some(et.as_str()), &d.node);
                            }
                            let cv = d.value.as_ref().and_then(|e| self.eval_expr(e));
                            let is_comptime = v.is_constant && cv.is_some();
                            if is_comptime {
                                let vname = self.node_text(&name_id.node);
                                if let Some(ref val) = cv {
                                    self.comptime_values.insert(vname, val.clone());
                                }
                            }
                            if let Some(key) = var_decl_key {
                                self.type_map.insert(key, DeclType::Var(VarType {
                                    name: tn.clone(),
                                    is_array: v.is_array,
                                    is_constant: v.is_constant,
                                    is_comptime,
                                }));
                            }
                            let label = Self::build_type_label(
                                tn, v.is_constant, is_comptime, v.is_array,
                            );
                            self.emit_type_hint(&name_id.node, &label, cv.as_ref());
                        }
                        // ── Array initializer diagnostic ───────────────────
                        if v.is_array && d.value.is_some() {
                            let vname = self.node_text(&name_id.node);
                            let remove_start = name_id.node.end_byte();
                            let remove_end = d.node.end_byte();
                            self.diagnostics.push(Diagnostic {
                                range: d.node.to_range(&self.rope),
                                message: crate::util::i18n::array_no_init(&vname),
                                severity: Some(DiagnosticSeverity::Error),
                                data: Some(serde_json::json!({
                                    "array_no_init_remove_start": remove_start,
                                    "array_no_init_remove_end": remove_end,
                                })),
                                ..Diagnostic::new("jass", "array-no-init")
                            });
                        }
                    }
                }
                v.decls.first().map(|d| DocumentSymbol {
                    name: self.id_name(&d.name),
                    kind: if v.is_constant { SymbolKind::Constant } else { SymbolKind::Variable },
                    range: d.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&d.name, &d.node),
                    ..Default::default()
                })
            }

            Statement::Set(s) => {
                self.register_id(&s.variable);
                if let Some(expr) = &s.index {
                    self.check_expr_hints(expr);
                    self.visit_expr(expr);
                }
                let value_type = if let Some(expr) = &s.value {
                    self.check_expr_hints(expr);
                    self.visit_expr(expr)
                } else {
                    None
                };
                if let Some(var_id) = &s.variable {
                    let name = self.node_text(&var_id.node);
                    // Type mismatch check: unknown → concrete type
                    if let Some(ref vt) = value_type {
                        if let Some(declared) = self.lookup_var_type(&name) {
                            self.check_type_mismatch(
                                &declared,
                                Some(vt.as_str()),
                                &s.node,
                            );
                        }
                    }
                    // ── Array set without index diagnostic ────────────
                    if s.index.is_none() && self.is_var_array(&name) {
                        let insert_pos = var_id.node.end_byte();
                        self.diagnostics.push(Diagnostic {
                            range: s.node.to_range(&self.rope),
                            message: crate::util::i18n::array_set_no_index(&name),
                            severity: Some(DiagnosticSeverity::Error),
                            data: Some(serde_json::json!({
                                "array_set_insert_pos": insert_pos,
                            })),
                            ..Diagnostic::new("jass", "array-set-no-index")
                        });
                    }
                    // hl: reference variable as Write
                    self.hl_reference_var(&name, &var_id.node, DocumentHighlightKind::Write);
                    for scope in vars.iter_mut().rev() {
                        if let Some(info) = scope.get_mut(&name) {
                            info.is_initialized = true;
                            break;
                        }
                    }
                }
                None
            }

            Statement::Call(c) => {
                if let Some(fc) = &c.func {
                    self.register_id(&fc.name);
                    if let Some(name_id) = &fc.name {
                        let fname = self.node_text(&name_id.node);
                        self.record_callee(&fname);
                        // hl: reference function as Read
                        self.hl_reference_func(&fname, &name_id.node, DocumentHighlightKind::Read);

                        // ── ExecuteFunc diagnostic ───────────────────────
                        if fname == "ExecuteFunc"
                            && !self.file_ignore_tags.contains("execute-func")
                        {
                            if fc.args.len() == 1 {
                                if let Some(ComptimeValue::Str(target)) = self.eval_expr(&fc.args[0]) {
                                    // Argument is a computable string literal → hint + quick fix data
                                    let new_text = format!("call {}()", target);
                                    self.diagnostics.push(Diagnostic {
                                        range: c.node.to_range(&self.rope),
                                        message: crate::util::i18n::execute_func_hint(&target),
                                        severity: Some(DiagnosticSeverity::Hint),
                                        data: Some(serde_json::json!({
                                            "execute_func_new_text": new_text,
                                        })),
                                        ..Diagnostic::new("jass", "execute-func")
                                    });
                                } else {
                                    // Argument is NOT computable → warning
                                    self.diagnostics.push(Diagnostic {
                                        range: c.node.to_range(&self.rope),
                                        message: crate::util::i18n::execute_func_bad_hack().to_string(),
                                        severity: Some(DiagnosticSeverity::Warning),
                                        ..Diagnostic::new("jass", "execute-func-bad")
                                    });
                                }
                            }
                        }
                    }
                    for arg in &fc.args {
                        // Check: passing an array variable as an argument is forbidden
                        // (see through parentheses).
                        let inner = Self::unwrap_parens(arg);
                        if let Expr::Id(id) = inner {
                            let aname = self.node_text(&id.node);
                            if self.is_var_array(&aname) {
                                self.diagnostics.push(Diagnostic {
                                    range: id.node.to_range(&self.rope),
                                    message: crate::util::i18n::array_in_argument(&aname),
                                    severity: Some(DiagnosticSeverity::Error),
                                    ..Diagnostic::new("jass", "array-in-arg")
                                });
                            }
                        }
                        self.check_expr_hints(arg);
                        self.visit_expr(arg);
                    }
                }
                None
            }

            Statement::If(i) => {
                self.push_fold_region(&i.node);
                if let Some(cond) = &i.condition {
                    self.check_expr_hints(cond);
                    self.visit_expr(cond);
                }
                let _body = self.visit_stmts(&i.body, vars);
                for branch in &i.branches {
                    if let Some(cond) = &branch.condition {
                        self.check_expr_hints(cond);
                        self.visit_expr(cond);
                    }
                    let _body = self.visit_stmts(&branch.body, vars);
                }
                // `else if` detected → diagnostic with quick-fix data.
                if let Some(fix) = &i.else_if_fix {
                    let if_range = fix.if_node.to_range(&self.rope);
                    let else_start = Position::from_byte_offset(&self.rope, fix.else_node.start_byte()).unwrap_or_default();
                    let if_start = Position::from_byte_offset(&self.rope, fix.if_node.start_byte()).unwrap_or_default();
                    let if_end = Position::from_byte_offset(&self.rope, fix.if_node.end_byte()).unwrap_or_default();
                    let outer_end = Position::from_byte_offset(&self.rope, i.node.end_byte()).unwrap_or_default();

                    let mut data = serde_json::json!({
                        "else_start_line": else_start.line,
                        "else_start_char": else_start.character,
                        "if_start_line": if_start.line,
                        "if_start_char": if_start.character,
                        "if_end_line": if_end.line,
                        "if_end_char": if_end.character,
                        "insert_endif_line": outer_end.line,
                        "insert_endif_char": else_start.character,
                    });

                    // Inner endif position (needed for indentation adjustment).
                    if let Some(ei) = &fix.inner_endif {
                        let ei_start = Position::from_byte_offset(&self.rope, ei.start_byte()).unwrap_or_default();
                        let ei_end = Position::from_byte_offset(&self.rope, ei.end_byte()).unwrap_or_default();
                        data["inner_endif_start_line"] = serde_json::json!(ei_start.line);
                        data["inner_endif_start_char"] = serde_json::json!(ei_start.character);
                        data["inner_endif_end_line"] = serde_json::json!(ei_end.line);
                        data["inner_endif_end_char"] = serde_json::json!(ei_end.character);
                    }

                    self.diagnostics.push(Diagnostic {
                        range: if_range,
                        message: crate::util::i18n::else_if_should_be_elseif().into(),
                        severity: Some(DiagnosticSeverity::Error),
                        data: Some(data),
                        ..Diagnostic::new("jass", "else-if")
                    });
                }
                None
            }

            Statement::Loop(l) => {
                self.push_fold_region(&l.node);
                let _body = self.visit_stmts(&l.body, vars);
                None
            }

            Statement::Return(r) => {
                if let Some(expr) = &r.value {
                    // Check: returning an array variable is forbidden in JASS
                    // (see through parentheses).
                    let inner = Self::unwrap_parens(expr);
                    if let Expr::Id(id) = inner {
                        let name = self.node_text(&id.node);
                        if self.is_var_array(&name) {
                            self.diagnostics.push(Diagnostic {
                                range: id.node.to_range(&self.rope),
                                message: crate::util::i18n::array_in_return(&name),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Diagnostic::new("jass", "array-in-return")
                            });
                        }
                    }
                    self.check_expr_hints(expr);
                    let expr_type = self.visit_expr(expr);

                    // Return type mismatch checks.
                    if let Some(ref rt) = self.current_return_type {
                        if rt == "nothing" {
                            // Value returned from `returns nothing`.
                            self.diagnostics.push(Diagnostic {
                                range: r.node.to_range(&self.rope),
                                message: crate::util::i18n::return_value_in_nothing(),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Diagnostic::new("jass", "return-nothing")
                            });
                        } else if let Some(ref et) = expr_type {
                            // Type mismatch: expression type vs declared return type.
                            if !Self::is_type_assignable(rt, et) {
                                self.diagnostics.push(Diagnostic {
                                    range: expr.cst_node().to_range(&self.rope),
                                    message: crate::util::i18n::return_type_mismatch(et, rt),
                                    severity: Some(DiagnosticSeverity::Error),
                                    ..Diagnostic::new("jass", "return-type-mismatch")
                                });
                            }
                        }
                    }
                } else {
                    // Bare `return` in a function with a non-nothing return type.
                    if let Some(ref rt) = self.current_return_type {
                        if rt != "nothing" {
                            self.diagnostics.push(Diagnostic {
                                range: r.node.to_range(&self.rope),
                                message: crate::util::i18n::return_missing_value(rt),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Diagnostic::new("jass", "return-missing-value")
                            });
                        }
                    }
                }
                None
            }
            Statement::Exitwhen(e) => {
                if let Some(expr) = &e.condition {
                    self.check_expr_hints(expr);
                    self.visit_expr(expr);
                }
                None
            }
            Statement::Comment(_) => unreachable!("handled above"),
            Statement::Import(_) => unreachable!("handled above"),
            Statement::SetDir(_) => unreachable!("handled above"),
            Statement::IgnoreDir(_) => unreachable!("handled above"),
            Statement::UjapiImport(_) => unreachable!("handled above"),
            Statement::EntryDir(_) => unreachable!("handled above"),
            Statement::Error(_) => None, // diagnostics already collected from ast.errors
        }
    }
}

