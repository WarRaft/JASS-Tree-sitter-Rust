use crate::lsp::hover::lsp::{MarkupContent, MarkupKind};
use crate::lsp::position::Position;
use crate::lsp::signature_help::lsp::{
    ParameterInformation, SignatureHelp, SignatureInformation,
};
use crate::util::file_store::FILE_STORE;
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{SymbolNS, SCOPE_RESOLVER};
use crate::util::uri_map::LNG_URI_MAP;
use url::Url;


pub(crate) fn compute(uri: &Url, position: &Position) -> Option<SignatureHelp> {
    let lng = LNG_URI_MAP.get(uri)?;
    let lng_val = lng.value().clone();
    if lng_val != "jass" && lng_val != "angelscript" {
        return None;
    }

    let rope_entry = ROPE_MAP.get(uri)?;
    let rope = rope_entry.value();

    let line_idx = position.line;
    let line_count = rope.line_of_offset(rope.len()) + 1;
    if line_idx >= line_count {
        return None;
    }

    let line_start = rope.offset_of_line(line_idx);
    let line_end = if line_idx + 1 < line_count {
        rope.offset_of_line(line_idx + 1)
    } else {
        rope.len()
    };
    let line_text = rope.slice_to_cow(line_start..line_end);

    // Walk backward from cursor column to find the function name and active parameter
    let col = position.character.min(line_text.len());
    let bytes = line_text.as_bytes();

    // Find matching open paren, counting commas for active parameter
    let mut depth = 0i32;
    let mut comma_count = 0u32;
    let mut paren_pos = None;

    let mut i = col;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    paren_pos = Some(i);
                    break;
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                comma_count += 1;
            }
            _ => {}
        }
    }

    let paren_col = paren_pos?;

    // Extract the function name before the paren
    let before_paren = &line_text[..paren_col];
    let name_end = before_paren.trim_end().len();
    if name_end == 0 {
        return None;
    }

    let name_start = before_paren[..name_end]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|p| p + 1)
        .unwrap_or(0);

    let func_name = &before_paren[name_start..name_end];
    if func_name.is_empty() {
        return None;
    }

    // Look up the function signature
    let (params, return_type, doc_comment) = lookup_callable(uri, func_name, &lng_val)?;

    // Build the signature label
    let params_str = if params.is_empty() {
        "nothing".to_string()
    } else {
        params
            .iter()
            .map(|(pname, ptype)| format!("{} {}", ptype, pname))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret = return_type.as_deref().unwrap_or("nothing");
    let label = if lng_val == "jass" {
        format!("function {} takes {} returns {}", func_name, params_str, ret)
    } else {
        // AngelScript style
        let params_str_as = if params.is_empty() {
            String::new()
        } else {
            params
                .iter()
                .map(|(pname, ptype)| format!("{} {}", ptype, pname))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!("{} {}({})", ret, func_name, params_str_as)
    };

    // Build parameter information with label offsets
    let param_infos: Vec<ParameterInformation> = if params.is_empty() {
        vec![]
    } else {
        let mut infos = Vec::with_capacity(params.len());

        if lng_val == "jass" {
            // "function Foo takes <params> returns ..."
            // Find the start of params after "takes "
            let takes_pos = label.find(" takes ").unwrap_or(0) + 7;
            let params_section = &label[takes_pos..];
            // Find end of params section (before " returns")
            let params_end_in_section = params_section
                .find(" returns ")
                .unwrap_or(params_section.len());
            let params_text = &params_section[..params_end_in_section];

            let mut offset = takes_pos;
            for (idx, (pname, ptype)) in params.iter().enumerate() {
                let param_str = format!("{} {}", ptype, pname);
                if let Some(pos) = params_text[offset - takes_pos..].find(&param_str) {
                    let start = offset + pos;
                    let end = start + param_str.len();
                    infos.push(ParameterInformation {
                        label: [start as u32, end as u32],
                        documentation: None,
                    });
                    offset = end;
                } else {
                    // Fallback: calculate from known positions
                    let sep_len = if idx > 0 { 2 } else { 0 }; // ", "
                    let start = offset + sep_len;
                    let end = start + param_str.len();
                    infos.push(ParameterInformation {
                        label: [start as u32, end as u32],
                        documentation: None,
                    });
                    offset = end;
                }
            }
        } else {
            // AngelScript: "ret func(type name, type name)"
            let paren_start = label.find('(').unwrap_or(0) + 1;
            let mut offset = paren_start;
            for (idx, (pname, ptype)) in params.iter().enumerate() {
                if idx > 0 {
                    offset += 2; // ", "
                }
                let param_str = format!("{} {}", ptype, pname);
                let start = offset;
                let end = start + param_str.len();
                infos.push(ParameterInformation {
                    label: [start as u32, end as u32],
                    documentation: None,
                });
                offset = end;
            }
        }

        infos
    };

    let active_parameter = if params.is_empty() {
        None
    } else {
        Some(comma_count.min(params.len().saturating_sub(1) as u32))
    };

    let documentation = doc_comment
        .filter(|d| !d.is_empty())
        .map(|d| MarkupContent {
            kind: MarkupKind::Markdown,
            value: d,
        });

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation,
            parameters: if param_infos.is_empty() {
                None
            } else {
                Some(param_infos)
            },
            active_parameter,
        }],
        active_signature: Some(0),
        active_parameter,
    })
}

/// Look up a callable (function/native) by name across the file and its imports.
/// Returns `(params, return_type, doc_comment)`.
fn lookup_callable(
    uri: &Url,
    name: &str,
    lng: &str,
) -> Option<(Vec<(String, String)>, Option<String>, Option<String>)> {
    // Try current file's FileSymbols first
    if let Some(snap_entry) = FILE_STORE.get(uri) {
        let fs = &snap_entry.value().file_symbols;
        if let Some(f) = fs.find_function(name) {
            let params: Vec<(String, String)> = f
                .params
                .iter()
                .map(|p| (p.name.clone(), p.type_name.clone()))
                .collect();
            return Some((params, f.return_type.clone(), f.doc_comment.clone()));
        }
        if let Some(n) = fs.find_native(name) {
            let params: Vec<(String, String)> = n
                .params
                .iter()
                .map(|p| (p.name.clone(), p.type_name.clone()))
                .collect();
            return Some((params, n.return_type.clone(), n.doc_comment.clone()));
        }
    }

    // Try scope resolver for cross-file lookups
    let mut visible = IMPORT_GRAPH.visible_component(uri);
    visible.insert(uri.clone());
    let entries = SCOPE_RESOLVER.resolve(name, SymbolNS::Func, &visible);
    if let Some(e) = entries.first() {
        return Some((e.params.clone(), e.return_type.clone(), e.doc_comment.clone()));
    }

    // For angelscript, also try the AS-specific symbols
    if lng == "angelscript" {
        // Check all files in component for AS functions
        for peer_uri in &visible {
            if let Some(snap) = FILE_STORE.get(peer_uri) {
                let fs = &snap.value().file_symbols;
                if let Some(f) = fs.find_function(name) {
                    let params: Vec<(String, String)> = f
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.type_name.clone()))
                        .collect();
                    return Some((params, f.return_type.clone(), f.doc_comment.clone()));
                }
                if let Some(n) = fs.find_native(name) {
                    let params: Vec<(String, String)> = n
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.type_name.clone()))
                        .collect();
                    return Some((params, n.return_type.clone(), n.doc_comment.clone()));
                }
            }
        }
    }

    None
}

