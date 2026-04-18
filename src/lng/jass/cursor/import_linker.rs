use std::collections::HashSet;
use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::ref_map::{ExternalDecl, ExternalOrigin, RawOccurrence, EXTERNAL_KEY_BASE};
use super::{Cursor, ImportedKind, ImportedSymbol};
use super::ref_tracking::UnresolvedRef;

impl Cursor {
    /// **Phase 2**: link unresolved references against local forward
    /// declarations, imported symbols, or standalone groups.
    pub(super) fn link_imports(&mut self, imported: &[ImportedSymbol]) {
        use std::collections::HashMap as Map;

        // Build lookup: (name, namespace) → ALL matching ImportedSymbols
        let mut import_lookup: Map<(&str, ImportedKind), Vec<&ImportedSymbol>> = Map::new();
        for sym in imported {
            import_lookup
                .entry((sym.name.as_str(), sym.kind))
                .or_default()
                .push(sym);
        }

        // Group unresolved refs by (name, namespace).
        let unresolved = std::mem::take(&mut self.unresolved_refs);
        let mut by_name: Map<(String, ImportedKind), Vec<UnresolvedRef>> = Map::new();
        for uref in unresolved {
            by_name
                .entry((uref.name.clone(), uref.namespace))
                .or_default()
                .push(uref);
        }

        // Sort groups by position of the first occurrence — deterministic key assignment.
        let mut sorted_groups: Vec<_> = by_name.into_iter().collect();
        sorted_groups.sort_by(|a, b| {
            let pos = |v: &Vec<UnresolvedRef>| {
                v.first().map(|r| (r.range.start.line, r.range.start.character))
            };
            pos(&a.1).cmp(&pos(&b.1))
        });

        let mut ext_counter: u32 = 0;

        for ((name, ns), refs) in sorted_groups {
            // 1. Check local forward declarations (global scope).
            let local_key = if let Some(scope) = self.hl_scopes.first() {
                match ns {
                    ImportedKind::Func => scope.funcs.get(name.as_str()).copied(),
                    ImportedKind::Var  => scope.vars.get(name.as_str()).copied(),
                }
            } else {
                None
            };

            if let Some(key) = local_key {
                for uref in refs {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range,
                            kind: uref.kind,
                            is_decl: false,
                        });
                }
            } else if let Some(syms) = import_lookup.get(&(name.as_str(), ns)) {
                // 2. Matched imports → external group with ALL origins
                let key = EXTERNAL_KEY_BASE + ext_counter;
                ext_counter += 1;
                self.ref_names.insert(key, name.clone());

                let mut seen_uris = HashSet::new();
                let mut origins = Vec::new();
                for sym in syms {
                    if seen_uris.insert(sym.origin_uri.as_str().to_string()) {
                        origins.push(ExternalOrigin {
                            uri: sym.origin_uri.clone(),
                            origin_decl_key: sym.origin_decl_key,
                        });
                    }
                }

                self.external_decls.insert(key, ExternalDecl { name: name.clone(), origins });
                for uref in refs {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range,
                            kind: uref.kind,
                            is_decl: false,
                        });
                }
            } else {
                // 3. No match → standalone group + "Undeclared" diagnostics.
                let key = self.alloc_key();
                self.ref_names.entry(key).or_insert_with(|| name.clone());
                for (i, uref) in refs.iter().enumerate() {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range.clone(),
                            kind: uref.kind,
                            is_decl: i == 0,
                        });
                    self.diagnostics.push(Diagnostic {
                        range: uref.range.clone(),
                        message: crate::util::i18n::undeclared_symbol(
                            crate::util::i18n::undeclared_label(
                                uref.is_type_ref,
                                matches!(ns, ImportedKind::Func),
                            ),
                            &name,
                        ),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Diagnostic::new("jass", "undeclared")
                    });
                }
            }
        }
    }
}

