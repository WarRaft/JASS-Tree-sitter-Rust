//! Type system helpers, compile-time expression evaluation, and expression visitor.
//!
//! Extracted from `mod.rs`. Contains:
//! - Type label building and inlay hint emission
//! - Compile-time evaluation (`eval_expr`, `eval_literal`, `eval_binary_comptime`, etc.)
//! - Type inference helpers (`is_handle_type`, `is_type_assignable`, `infer_binary_type`, etc.)
//! - Operator kind extraction helpers
//! - Variable/function type lookups
//! - Expression visitor (`visit_expr`)

use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::highlight::DocumentHighlightKind;
use crate::http::inlay_hint::{InlayHint, InlayHintKind};
use crate::http::position::Position;
use crate::http::range::Range;
use crate::lng::jass::ast::Expr;
use crate::lng::jass::kind::Kind;
use crate::lng::jass::type_map::{ComptimeValue, DeclType, UNKNOWN_TYPE};
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;

use super::Cursor;

impl Cursor {
    // ─── type system helpers ─────────────────────────────────────────────

    /// Emit an `InlayHint` right after `node` showing `label` as a type tag.
    ///
    /// When `value` is `Some`, the hint is formatted as `: type(value)`.
    pub(super) fn emit_type_hint(&mut self, node: &Node, label: &str, value: Option<&ComptimeValue>) {
        let display = match value {
            Some(v) => format!(": {}({})", label, v),
            None => format!(": {}", label),
        };
        let position = Position::from_byte_offset(&self.rope, node.end_byte())
            .unwrap_or_default();
        self.type_hints.push(InlayHint {
            position,
            label: display,
            kind: InlayHintKind::Type,
            byte_offset: node.end_byte(),
        });
    }

    /// Format a human-readable type label with optional modifiers.
    ///
    /// Examples: `integer`, `constant real`, `comptime integer`, `integer array`.
    pub(super) fn build_type_label(
        type_name: &str,
        is_constant: bool,
        is_comptime: bool,
        is_array: bool,
    ) -> String {
        let mut parts = Vec::new();
        if is_comptime {
            parts.push("comptime");
        } else if is_constant {
            parts.push("constant");
        }
        parts.push(type_name);
        if is_array {
            parts.push("array");
        }
        parts.join(" ")
    }

    /// Evaluate an expression at compile time, returning the computed value
    /// if the expression consists exclusively of literals, `comptime` globals,
    /// and pure operators.
    pub(super) fn eval_expr(&self, expr: &Expr) -> Option<ComptimeValue> {
        if let Some(v) = self
            .ast_comptime_values
            .get(&(expr.cst_node().start_byte(), expr.cst_node().end_byte()))
        {
            return Some(v.clone());
        }

        match expr {
            Expr::Literal(node) => self.eval_literal(node),
            Expr::Id(id) => {
                let name = self.node_text(&id.node);
                match name.as_str() {
                    "true" => Some(ComptimeValue::Bool(true)),
                    "false" => Some(ComptimeValue::Bool(false)),
                    "null" => Some(ComptimeValue::Null),
                    _ => self.comptime_values.get(&name).cloned(),
                }
            }
            Expr::Binary { node, left, right } => {
                let lv = self.eval_expr(left)?;
                let rv = self.eval_expr(right)?;
                let op = Self::binary_op_kind(node)?;
                Self::eval_binary_comptime(op, &lv, &rv)
            }
            Expr::Unary { node, operand } => {
                let v = self.eval_expr(operand)?;
                let op = Self::unary_op_kind(node)?;
                Self::eval_unary_comptime(op, &v)
            }
            Expr::Parens { inner, .. } => self.eval_expr(inner),
            // Function calls, array accesses, and function references
            // are never comptime in JASS.
            Expr::Call(_) | Expr::FuncRef(_) | Expr::Index { .. } => None,
        }
    }

    /// Evaluate a literal CST node at compile time.
    pub(super) fn eval_literal(&self, node: &Node) -> Option<ComptimeValue> {
        let kind = Kind::try_from(node.kind_id()).ok()?;
        match kind {
            Kind::Number => {
                let text = self.node_text(node);
                Self::parse_integer_literal(&text).map(ComptimeValue::Integer)
            }
            Kind::Float => {
                let text = self.node_text(node);
                text.parse::<f64>().ok().map(ComptimeValue::Real)
            }
            Kind::StringLiteral => {
                let text = self.node_text(node);
                let inner = if text.len() >= 2 {
                    &text[1..text.len() - 1]
                } else {
                    ""
                };
                Some(ComptimeValue::Str(Self::unescape_jass_string(inner)))
            }
            Kind::Rawcode => {
                let text = self.node_text(node);
                let inner = if text.len() >= 2 {
                    &text[1..text.len() - 1]
                } else {
                    ""
                };
                let mut val: i64 = 0;
                for b in inner.bytes() {
                    val = (val << 8) | (b as i64);
                }
                Some(ComptimeValue::Integer(val))
            }
            _ => None,
        }
    }

    /// Parse a JASS integer literal (decimal, hex `0x…`, octal `0…`).
    pub(super) fn parse_integer_literal(text: &str) -> Option<i64> {
        if text.len() > 2 && (text.starts_with("0x") || text.starts_with("0X")) {
            i64::from_str_radix(&text[2..], 16).ok()
        } else if text.starts_with('$') && text.len() > 1 {
            // Alternate hex prefix used in some JASS dialects
            i64::from_str_radix(&text[1..], 16).ok()
        } else if text.starts_with('0') && text.len() > 1 && text.chars().all(|c| c.is_ascii_digit()) {
            i64::from_str_radix(&text[1..], 8).ok()
        } else {
            text.parse::<i64>().ok()
        }
    }

    /// Basic JASS string unescape: `\\` → `\`, `\"` → `"`, `\n` → newline.
    pub(super) fn unescape_jass_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Evaluate a binary operation on two compile-time values.
    pub(super) fn eval_binary_comptime(op: Kind, left: &ComptimeValue, right: &ComptimeValue) -> Option<ComptimeValue> {
        use ComptimeValue::*;
        match op {
            Kind::Plus => match (left, right) {
                (Integer(a), Integer(b)) => Some(Integer(a.wrapping_add(*b))),
                (Real(a), Real(b)) => Some(Real(a + b)),
                (Integer(a), Real(b)) => Some(Real(*a as f64 + b)),
                (Real(a), Integer(b)) => Some(Real(a + *b as f64)),
                // String concatenation — JASS converts the other operand.
                (Str(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
                (Str(a), Integer(b)) => Some(Str(format!("{}{}", a, b))),
                (Str(a), Real(b)) => Some(Str(format!("{}{}", a, b))),
                (Integer(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
                (Real(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
                _ => None,
            },
            Kind::Minus => match (left, right) {
                (Integer(a), Integer(b)) => Some(Integer(a.wrapping_sub(*b))),
                (Real(a), Real(b)) => Some(Real(a - b)),
                (Integer(a), Real(b)) => Some(Real(*a as f64 - b)),
                (Real(a), Integer(b)) => Some(Real(a - *b as f64)),
                _ => None,
            },
            Kind::Star => match (left, right) {
                (Integer(a), Integer(b)) => Some(Integer(a.wrapping_mul(*b))),
                (Real(a), Real(b)) => Some(Real(a * b)),
                (Integer(a), Real(b)) => Some(Real(*a as f64 * b)),
                (Real(a), Integer(b)) => Some(Real(a * *b as f64)),
                _ => None,
            },
            Kind::Slash => match (left, right) {
                (Integer(a), Integer(b)) if *b != 0 => Some(Integer(a / b)),
                (Real(a), Real(b)) if *b != 0.0 => Some(Real(a / b)),
                (Integer(a), Real(b)) if *b != 0.0 => Some(Real(*a as f64 / b)),
                (Real(a), Integer(b)) if *b != 0 => Some(Real(a / *b as f64)),
                _ => None,
            },
            Kind::And => match (left, right) {
                (Bool(a), Bool(b)) => Some(Bool(*a && *b)),
                _ => None,
            },
            Kind::Or => match (left, right) {
                (Bool(a), Bool(b)) => Some(Bool(*a || *b)),
                _ => None,
            },
            Kind::EqEq => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a == b)),
                (Real(a), Real(b)) => Some(Bool(a == b)),
                (Str(a), Str(b)) => Some(Bool(a == b)),
                (Bool(a), Bool(b)) => Some(Bool(a == b)),
                _ => None,
            },
            Kind::Neq => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a != b)),
                (Real(a), Real(b)) => Some(Bool(a != b)),
                (Str(a), Str(b)) => Some(Bool(a != b)),
                (Bool(a), Bool(b)) => Some(Bool(a != b)),
                _ => None,
            },
            Kind::Lt => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a < b)),
                (Real(a), Real(b)) => Some(Bool(a < b)),
                (Str(a), Str(b)) => Some(Bool(a < b)),
                _ => None,
            },
            Kind::Gt => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a > b)),
                (Real(a), Real(b)) => Some(Bool(a > b)),
                (Str(a), Str(b)) => Some(Bool(a > b)),
                _ => None,
            },
            Kind::Le => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a <= b)),
                (Real(a), Real(b)) => Some(Bool(a <= b)),
                (Str(a), Str(b)) => Some(Bool(a <= b)),
                _ => None,
            },
            Kind::Ge => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a >= b)),
                (Real(a), Real(b)) => Some(Bool(a >= b)),
                (Str(a), Str(b)) => Some(Bool(a >= b)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Evaluate a unary operation on a compile-time value.
    pub(super) fn eval_unary_comptime(op: Kind, val: &ComptimeValue) -> Option<ComptimeValue> {
        use ComptimeValue::*;
        match op {
            Kind::Minus => match val {
                Integer(v) => Some(Integer(-v)),
                Real(v) => Some(Real(-v)),
                _ => None,
            },
            Kind::Not => match val {
                Bool(v) => Some(Bool(!v)),
                _ => None,
            },
            _ => None,
        }
    }

    // ─── Expression type helpers ─────────────────────────────────────

    /// Check if a type name belongs to the handle family.
    ///
    /// Handle family = `handle` itself + any custom type that is not a
    /// built-in primitive (`integer`, `real`, `boolean`, `string`, `code`,
    /// `nothing`, `null`, `unknown`).
    pub(super) fn is_handle_type(type_name: &str) -> bool {
        !matches!(
            type_name,
            "integer" | "real" | "boolean" | "string" | "code" | "nothing" | "null" | "unknown"
        )
    }

    /// Check if `expr_type` can be assigned to a variable of `declared_type`
    /// according to JASS type rules.
    ///
    /// Allowed implicit conversions:
    /// - same type → OK
    /// - `integer` → `real` (I2R)
    /// - `null` → any handle-based type, `string`, or `code`
    /// - any handle subtype → any other handle subtype (JASS allows implicit
    ///   handle casts)
    pub(super) fn is_type_assignable(declared: &str, expr: &str) -> bool {
        // Same type is always OK.
        if declared == expr {
            return true;
        }

        // Unknown on either side → can't determine, assume OK.
        // The relevant "Undeclared" or operator diagnostics are emitted elsewhere.
        if declared == UNKNOWN_TYPE || expr == UNKNOWN_TYPE {
            return true;
        }

        // `nothing` is not assignable to/from anything.
        if expr == "nothing" || declared == "nothing" {
            return false;
        }

        // integer → real: OK (implicit I2R conversion).
        if expr == "integer" && declared == "real" {
            return true;
        }

        // null → string, code, or handle-based type: OK.
        if expr == "null" {
            return declared != "integer" && declared != "real" && declared != "boolean";
        }

        // Both handle-based → OK (JASS allows implicit handle casts).
        if Self::is_handle_type(declared) && Self::is_handle_type(expr) {
            return true;
        }

        // Everything else is a mismatch.
        false
    }

    /// Emit a diagnostic on the `=` operator when the inferred expression
    /// type is incompatible with the declared variable type.
    pub(super) fn check_type_mismatch(
        &mut self,
        declared_type: &str,
        expr_type: Option<&str>,
        stmt_node: &Node,
    ) {
        if let Some(et) = expr_type {
            if !Self::is_type_assignable(declared_type, et) {
                let range = Self::find_equal_range(stmt_node, &self.rope)
                    .unwrap_or_else(|| stmt_node.to_range(&self.rope));
                self.diagnostics.push(Diagnostic {
                    range,
                    message: crate::util::i18n::cannot_assign_type(et, declared_type),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Diagnostic::new("jass", "type-mismatch")
                });
            }
        }
    }

    /// Look up the type of a variable by name via highlight scopes + type map,
    /// falling back to imported variable types.
    pub(super) fn lookup_var_type(&self, name: &str) -> Option<String> {
        // Local scope lookup
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name).copied());
        if let Some(key) = decl_key {
            if let Some(DeclType::Var(vt)) = self.type_map.get(&key) {
                return Some(vt.name.clone());
            }
        }
        // Imported variable fallback
        self.imported_var_types
            .get(name)
            .and_then(|t| t.clone())
    }

    /// Check whether a variable name refers to an array declaration.
    pub(super) fn is_var_array(&self, name: &str) -> bool {
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name).copied());
        if let Some(key) = decl_key {
            if let Some(DeclType::Var(vt)) = self.type_map.get(&key) {
                return vt.is_array;
            }
        }
        false
    }

    /// Look up the return type of a function by name via highlight scopes + type map,
    /// falling back to imported function return types.
    pub(super) fn lookup_func_return_type(&self, name: &str) -> Option<String> {
        // Local scope lookup
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.funcs.get(name).copied());
        if let Some(key) = decl_key {
            if let Some(DeclType::Func(ft)) = self.type_map.get(&key) {
                return ft.return_type.clone();
            }
        }
        // Imported function fallback
        self.imported_func_returns
            .get(name)
            .and_then(|t| t.clone())
    }

    /// Find the operator token kind inside a binary expression CST node.
    pub(super) fn binary_op_kind(node: &Node) -> Option<Kind> {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                let k = Kind::try_from(child.grammar_id()).ok();
                match k {
                    Some(Kind::Plus) | Some(Kind::Minus) | Some(Kind::Star) | Some(Kind::Slash)
                    | Some(Kind::Lt) | Some(Kind::Gt) | Some(Kind::Le) | Some(Kind::Ge)
                    | Some(Kind::EqEq) | Some(Kind::Neq) | Some(Kind::And) | Some(Kind::Or) => {
                        return k;
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Find the operator **node** inside a binary expression CST node.
    pub(super) fn binary_op_range(node: &Node, rope: &Rope) -> Option<(Kind, Range, String)> {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                let k = Kind::try_from(child.grammar_id()).ok();
                match k {
                    Some(Kind::Plus) | Some(Kind::Minus) | Some(Kind::Star) | Some(Kind::Slash)
                    | Some(Kind::Lt) | Some(Kind::Gt) | Some(Kind::Le) | Some(Kind::Ge)
                    | Some(Kind::EqEq) | Some(Kind::Neq) | Some(Kind::And) | Some(Kind::Or) => {
                        let text_bytes = &rope.slice_to_cow(child.start_byte()..child.end_byte());
                        return Some((k.unwrap(), child.to_range(rope), text_bytes.to_string()));
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Find the `=` token range inside a CST node (declaration / set statement).
    pub(super) fn find_equal_range(node: &Node, rope: &Rope) -> Option<Range> {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                if Kind::try_from(child.grammar_id()).ok() == Some(Kind::Equal) {
                    return Some(child.to_range(rope));
                }
            }
        }
        None
    }

    /// Find the operator token kind for a unary expression CST node.
    pub(super) fn unary_op_kind(node: &Node) -> Option<Kind> {
        node.child(0)
            .and_then(|c| Kind::try_from(c.grammar_id()).ok())
    }

    /// Infer the result type of a binary operation from operator and operand types.
    ///
    /// Returns `Some(UNKNOWN_TYPE)` when both operand types are known but
    /// the combination is invalid (e.g. `string * integer`, `boolean - boolean`),
    /// or when either operand is already `unknown`.
    pub(super) fn infer_binary_type(
        op: Option<Kind>,
        left: Option<&str>,
        right: Option<&str>,
    ) -> Option<String> {
        let op = op?;
        let l = left?;
        let r = right?;

        // unknown propagates: any operation with unknown yields unknown.
        if l == UNKNOWN_TYPE || r == UNKNOWN_TYPE {
            return Some(UNKNOWN_TYPE.to_string());
        }

        let is_numeric = |t: &str| t == "integer" || t == "real";

        match op {
            // Comparison and logical operators always produce boolean.
            Kind::And | Kind::Or | Kind::Lt | Kind::Gt | Kind::Le | Kind::Ge
            | Kind::EqEq | Kind::Neq => Some("boolean".to_string()),

            Kind::Plus => {
                // string + anything ⇒ string (concatenation via I2S/R2S)
                if l == "string" || r == "string" {
                    Some("string".to_string())
                } else if is_numeric(l) && is_numeric(r) {
                    if l == "real" || r == "real" {
                        Some("real".to_string())
                    } else {
                        Some("integer".to_string())
                    }
                } else {
                    Some(UNKNOWN_TYPE.to_string())
                }
            }

            Kind::Minus | Kind::Star | Kind::Slash => {
                if is_numeric(l) && is_numeric(r) {
                    if l == "real" || r == "real" {
                        Some("real".to_string())
                    } else {
                        Some("integer".to_string())
                    }
                } else {
                    Some(UNKNOWN_TYPE.to_string())
                }
            }

            _ => None,
        }
    }

    // ─── Expression visitor ────────────────────────────────────────────

    /// Visit an expression, emitting type hints on leaf sub-expressions
    /// and returning the inferred type of the whole expression.
    pub(super) fn visit_expr(&mut self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Id(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
                let name = self.node_text(&id.node);
                self.hl_reference_var(&name, &id.node, DocumentHighlightKind::Read);

                // Infer type: built-in constants or variable lookup.
                // If the variable is not declared locally, return `unknown`
                // so the type propagates correctly through expressions.
                // A diagnostic will be emitted in Phase 2 if the name
                // is not resolved by imports either.
                let ty = match name.as_str() {
                    "true" | "false" => Some("boolean".to_string()),
                    "null" => Some("null".to_string()),
                    _ => self.lookup_var_type(&name)
                        .or_else(|| Some(UNKNOWN_TYPE.to_string())),
                };
                if let Some(ref t) = ty {
                    self.emit_type_hint(&id.node, t, None);
                }
                ty
            }
            Expr::Call(fc) => {
                self.register_id(&fc.name);
                let mut ret_type = None;
                if let Some(name_id) = &fc.name {
                    let fname = self.node_text(&name_id.node);
                    self.record_callee(&fname);
                    self.hl_reference_func(&fname, &name_id.node, DocumentHighlightKind::Read);
                    ret_type = self.lookup_func_return_type(&fname);
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
                if let Some(ref t) = ret_type {
                    self.emit_type_hint(&fc.node, t, None);
                }
                ret_type
            }
            Expr::FuncRef(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
                let fname = self.node_text(&id.node);
                self.record_callee(&fname);
                self.record_func_ref(&fname);
                self.hl_reference_func(&fname, &id.node, DocumentHighlightKind::Read);
                let ty = "code".to_string();
                self.emit_type_hint(&id.node, &ty, None);
                Some(ty)
            }
            Expr::Binary { node, left, right } => {
                let lt = self.visit_expr(left);
                let rt = self.visit_expr(right);
                let op = Self::binary_op_kind(node);
                let result = Self::infer_binary_type(op, lt.as_deref(), rt.as_deref());

                // Type error → diagnostic on the operator token
                if result.as_deref() == Some(UNKNOWN_TYPE) {
                    if let (Some(l), Some(r)) = (&lt, &rt) {
                        if let Some((_kind, op_range, op_text)) = Self::binary_op_range(node, &self.rope) {
                            self.diagnostics.push(Diagnostic {
                                range: op_range,
                                message: crate::util::i18n::operator_binary_error(&op_text, l, r),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Diagnostic::new("jass", "operator-type")
                            });
                        }
                    }
                }

                // type hint: show inferred type + compile-time value
                if let Some(ref t) = result {
                    let cv = self.eval_expr(expr);
                    self.emit_type_hint(node, t, cv.as_ref());
                }

                result
            }
            Expr::Unary { node, operand, .. } => {
                let ot = self.visit_expr(operand);
                let op = Self::unary_op_kind(node);
                let ot_type = ot.as_deref().map(|s| s.to_string());

                // unknown propagates through unary operations.
                let result = if ot.as_deref() == Some(UNKNOWN_TYPE) {
                    Some(UNKNOWN_TYPE.to_string())
                } else {
                    match (op, ot.as_deref()) {
                        (Some(Kind::Not), Some("boolean")) => Some("boolean".to_string()),
                        (Some(Kind::Not), Some(_)) => Some(UNKNOWN_TYPE.to_string()),
                        (Some(Kind::Minus), Some(t)) if t == "integer" || t == "real" => Some(t.to_string()),
                        (Some(Kind::Minus), Some(_)) => Some(UNKNOWN_TYPE.to_string()),
                        _ => ot,
                    }
                };

                // Type error → diagnostic on the operator token
                if result.as_deref() == Some(UNKNOWN_TYPE) {
                    if let Some(ref t) = ot_type {
                        if let Some(op_n) = node.child(0) {
                            let op_text = self.node_text(&op_n);
                            self.diagnostics.push(Diagnostic {
                                range: op_n.to_range(&self.rope),
                                message: crate::util::i18n::operator_unary_error(&op_text, t),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Diagnostic::new("jass", "operator-type")
                            });
                        }
                    }
                }

                // type hint: show inferred type + compile-time value
                if let Some(ref t) = result {
                    let cv = self.eval_expr(expr);
                    self.emit_type_hint(node, t, cv.as_ref());
                }

                result
            }
            Expr::Parens { inner, .. } => {
                self.visit_expr(inner)
            }
            Expr::Index { array, index, .. } => {
                let arr_type = self.visit_expr(array);
                self.visit_expr(index);
                // Element type is the array variable's base type.
                if let Some(ref t) = arr_type {
                    self.emit_type_hint(expr.cst_node(), t, None);
                }
                arr_type
            }
            Expr::Literal(node) => {
                let kind = Kind::try_from(node.kind_id()).ok();
                let ty = match kind {
                    Some(Kind::Number) | Some(Kind::Rawcode) => Some("integer".to_string()),
                    Some(Kind::Float) => Some("real".to_string()),
                    Some(Kind::StringLiteral) => Some("string".to_string()),
                    _ => None,
                };
                if let Some(ref t) = ty {
                    // Show comptime value only when it differs from the source
                    // text (rawcodes, hex/octal literals). Plain decimals,
                    // reals, and strings are already visible as-is.
                    let cv = match kind {
                        Some(Kind::Rawcode) => self.eval_literal(node),
                        Some(Kind::Number) => {
                            let text = self.node_text(node);
                            if text.starts_with("0x") || text.starts_with("0X")
                                || text.starts_with('$')
                                || (text.starts_with('0') && text.len() > 1
                                    && text.chars().all(|c| c.is_ascii_digit()))
                            {
                                self.eval_literal(node)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    self.emit_type_hint(node, t, cv.as_ref());
                }
                ty
            }
        }
    }
}

