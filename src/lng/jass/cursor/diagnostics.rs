//! Diagnostic checks for code quality issues.
//!
//! This module contains various diagnostic checks performed during AST walking,
//! implemented as `impl Cursor` methods:
//! - Redundant parentheses detection
//! - Bool comparison simplification (`expr == true` → `expr`)
//! - Redundant if-return simplification
//! - Empty else detection
//! - Dead code detection
//! - And/Or-chain collapse detection

use crate::http::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::http::range::Range;
use crate::lng::jass::ast::{Expr, IfStmt, ReturnStmt, Statement};
use crate::lng::jass::kind::Kind;
use crate::util::roper::node::NodeExt;
use tree_sitter::Node;

use super::Cursor;

impl Cursor {
    // ─── Redundant parentheses detection ─────────────────────────────────

    /// If `expr` is wrapped in redundant `(…)` — including multiple nested
    /// layers — emit one `Hint` diagnostic **per layer**.
    ///
    /// Each diagnostic stores `parens_open` / `parens_close` as the 1-char
    /// ranges of the `(` and `)` of that specific layer.  Deleting those
    /// single characters never produces overlapping edits, even for deeply
    /// nested parens like `( (expr) )`.
    pub(super) fn check_redundant_parens(&mut self, expr: &Expr) {
        let mut current = expr;
        loop {
            let (node, inner) = match current {
                Expr::Parens { node, inner } => (node, inner.as_ref()),
                _ => break,
            };
            let sb = node.start_byte();
            let eb = node.end_byte();
            let full_range  = Range::from_byte_offsets(&self.rope, sb, eb);
            let open_range  = Range::from_byte_offsets(&self.rope, sb, sb + 1);
            let close_range = Range::from_byte_offsets(&self.rope, eb.saturating_sub(1), eb);
            self.diagnostics.push(Diagnostic {
                range: full_range,
                message: crate::util::i18n::redundant_parens().to_string(),
                severity: Some(DiagnosticSeverity::Hint),
                source: Some("jass".into()),
                code: Some(DiagnosticCode::String("parens".into())),
                data: Some(serde_json::json!({
                    "parens_open":  open_range,
                    "parens_close": close_range,
                })),
                ..Default::default()
            });
            current = inner;
        }
    }

    /// Detect `expr == true`, `expr == false`, `expr != true`, `expr != false`
    /// and emit a `Warning` diagnostic with the simplified replacement.
    ///
    /// In JASS there is no implicit boolean coercion, so comparing a boolean
    /// expression to a boolean literal is always redundant:
    ///   `a == true`  →  `a`
    ///   `a == false` →  `not(a)`
    ///   `a != true`  →  `not(a)`
    ///   `a != false` →  `a`
    pub(super) fn check_bool_cmp(&mut self, expr: &Expr) {
        // Peel transparent parens — the parens diagnostic handles those.
        let mut current = expr;
        while let Expr::Parens { inner, .. } = current {
            current = inner;
        }
        let (node, left, right) = match current {
            Expr::Binary { node, left, right } => (node, left.as_ref(), right.as_ref()),
            _ => return,
        };
        let op = match Self::binary_op_kind(node) {
            Some(k @ (Kind::EqEq | Kind::Neq)) => k,
            _ => return,
        };
        // Find which side is the boolean literal.
        let (other, bool_val) = if let Some(b) = self.as_bool_literal(right) {
            (left, b)
        } else if let Some(b) = self.as_bool_literal(left) {
            (right, b)
        } else {
            return;
        };
        // == true / != false → keep expr; == false / != true → negate
        let negate = (op == Kind::EqEq) != bool_val;
        let new_text = if negate {
            self.negate_cond_text(other)
        } else {
            self.expr_text(other)
        };
        let cst = Self::expr_node(current);
        let range = Range::from_byte_offsets(&self.rope, cst.start_byte(), cst.end_byte());
        self.diagnostics.push(Diagnostic {
            range,
            message: crate::util::i18n::redundant_bool_cmp().to_string(),
            severity: Some(DiagnosticSeverity::Warning),
            source: Some("jass".into()),
            code: Some(DiagnosticCode::String("bool-cmp".into())),
            data: Some(serde_json::json!({ "bool_cmp_new_text": new_text })),
            ..Default::default()
        });
    }

    /// Run all expression-level hint/warning checks on `expr`.
    ///
    /// Call this instead of bare [`check_redundant_parens`] so that every new
    /// expression-level check is automatically applied everywhere.
    #[inline]
    pub(super) fn check_expr_hints(&mut self, expr: &Expr) {
        self.check_redundant_parens(expr);
        self.check_bool_cmp(expr);
    }

    // ─── Simplify if-return detection ────────────────────────────────

    /// Return the CST node backing any [`Expr`].
    pub(super) fn expr_node<'a, 'x>(expr: &'a Expr<'x>) -> &'a Node<'x> {
        match expr {
            Expr::Id(id) => &id.node,
            Expr::Call(fc) => &fc.node,
            Expr::FuncRef(id) => &id.node,
            Expr::Binary { node, .. } => node,
            Expr::Unary { node, .. } => node,
            Expr::Parens { node, .. } => node,
            Expr::Index { node, .. } => node,
            Expr::Literal(node) => node,
        }
    }

    /// Extract the text of any expression.
    pub(super) fn expr_text(&self, expr: &Expr) -> String {
        self.node_text(Self::expr_node(expr))
    }

    /// If `expr` is a boolean literal (`true` / `false`), return its value.
    pub(super) fn as_bool_literal(&self, expr: &Expr) -> Option<bool> {
        if let Expr::Id(id) = expr {
            match self.node_text(&id.node).as_str() {
                "true" => return Some(true),
                "false" => return Some(false),
                _ => {}
            }
        }
        None
    }

    /// Compute the negation of a JASS condition expression as source text.
    ///
    /// * `if(not(inner))…` → `inner` text  (double-not elimination)
    /// * `if(cond)…`       → `not(cond)` text
    pub(super) fn negate_cond_text(&self, cond: &Expr) -> String {
        match cond {
            // Unary `not expr`: strip the `not` (double-negation elimination).
            Expr::Unary { node, operand } => {
                let is_not = node
                    .child(0)
                    .and_then(|c| Kind::try_from(c.grammar_id()).ok())
                    .map_or(false, |k| k == Kind::Not);
                if is_not {
                    self.expr_text(operand)
                } else {
                    format!("not {}", self.node_text(node))
                }
            }
            // Parenthesised expression: `not (expr)` — keep the parens.
            Expr::Parens { .. } => format!("not {}", self.expr_text(cond)),
            // Everything else: `not expr` with a space.
            other => format!("not {}", self.expr_text(other)),
        }
    }

    /// Try to detect the pattern inside a single statement list:
    ///
    /// ```jass
    /// if (cond) then
    ///     return BOOL
    /// endif
    /// return (not BOOL)
    /// ```
    ///
    /// and emit a `Hint` diagnostic with `source = "simplify"` carrying the
    /// replacement text in `data.simplify_new_text`.
    pub(super) fn check_if_return_pattern(
        &mut self,
        if_stmt: &IfStmt,
        next_ret: &ReturnStmt,
    ) {
        // Must be a plain `if … then … endif` with no elseif/else.
        if !if_stmt.branches.is_empty() || if_stmt.body.len() != 1 {
            return;
        }
        let body_ret = match &if_stmt.body[0] {
            Statement::Return(r) => r,
            _ => return,
        };
        let body_val = match &body_ret.value {
            Some(v) => v,
            None => return,
        };
        let next_val = match &next_ret.value {
            Some(v) => v,
            None => return,
        };
        let body_b = match self.as_bool_literal(body_val) {
            Some(b) => b,
            None => return,
        };
        let next_b = match self.as_bool_literal(next_val) {
            Some(b) => b,
            None => return,
        };
        // The two `return` values must be opposite booleans.
        if body_b == next_b {
            return;
        }
        let cond = match &if_stmt.condition {
            Some(c) => c,
            None => return,
        };

        // Build the replacement text: `return <expr>`.
        let new_text = if body_b {
            // if(cond) then return true endif; return false → return cond
            format!("return {}", self.expr_text(cond))
        } else {
            // if(cond) then return false endif; return true → return not(cond)
            // with double-not elimination when cond is already `not(…)`.
            format!("return {}", self.negate_cond_text(cond))
        };

        let start_byte = if_stmt.node.start_byte();
        let end_byte = next_ret.node.end_byte();
        let range = Range::from_byte_offsets(&self.rope, start_byte, end_byte);

        self.diagnostics.push(Diagnostic {
            range,
            message: crate::util::i18n::simplify_if_return().to_string(),
            severity: Some(DiagnosticSeverity::Hint),
            source: Some("jass".into()),
            code: Some(DiagnosticCode::String("simplify".into())),
            data: Some(serde_json::json!({
                "simplify_new_text": new_text,
            })),
            ..Default::default()
        });
    }

    /// Walk a statement list (and its nested `if`/`loop` bodies) looking for
    /// redundant if-return patterns.
    pub(super) fn check_redundant_if_return(&mut self, stmts: &[Statement]) {
        let n = stmts.len();
        for i in 0..n {
            // Check the pair (stmts[i], stmts[i+1]).
            if i + 1 < n {
                if let Statement::If(if_stmt) = &stmts[i] {
                    if let Statement::Return(next_ret) = &stmts[i + 1] {
                        // Clone the data we need because check_if_return_pattern
                        // takes `&mut self` and we have borrows on `stmts`.
                        let if_stmt = if_stmt.clone();
                        let next_ret = next_ret.clone();
                        self.check_if_return_pattern(&if_stmt, &next_ret);
                    }
                }
            }
            // Recurse into nested bodies.
            match &stmts[i] {
                Statement::If(if_stmt) => {
                    let if_stmt = if_stmt.clone();
                    self.check_redundant_if_return(&if_stmt.body.clone());
                    for branch in &if_stmt.branches {
                        self.check_redundant_if_return(&branch.body.clone());
                    }
                }
                Statement::Loop(l) => {
                    let body = l.body.clone();
                    self.check_redundant_if_return(&body);
                }
                _ => {}
            }
        }
    }

    // ─── Empty else detection ─────────────────────────────────────────

    /// Walk a statement list looking for `if … else <nothing> endif` patterns.
    ///
    /// Emits a `Hint` diagnostic with `source = "empty_else"` on the `else`
    /// keyword.  The diagnostic carries `data.empty_else_delete_range` — the
    /// LSP range covering the `else` line (from its first character to the
    /// start of the next non-blank line) that the quick-fix should delete.
    pub(super) fn check_empty_else(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            if let Statement::If(if_stmt) = stmt {
                for branch in &if_stmt.branches {
                    // Only plain `else` (condition == None) with an empty body.
                    if branch.condition.is_some() || !branch.body.is_empty() {
                        continue;
                    }

                    let else_node = &branch.node;
                    let else_row = else_node.start_position().row;

                    // Walk CST siblings of `else` to find the `endif` keyword.
                    let mut endif_row = None;
                    let mut sib = else_node.next_sibling();
                    while let Some(n) = sib {
                        if n.grammar_id() == crate::lng::jass::kind::Kind::Endif as u16 {
                            endif_row = Some(n.start_position().row);
                            break;
                        }
                        sib = n.next_sibling();
                    }
                    let endif_row = match endif_row {
                        Some(r) => r,
                        None => continue,
                    };

                    // Diagnostic range: highlight the `else` keyword itself.
                    let diag_range = Range::from_byte_offsets(
                        &self.rope,
                        else_node.start_byte(),
                        else_node.end_byte(),
                    );

                    // Delete range: from start of `else` line to start of `endif` line.
                    let delete_start = self.rope.offset_of_line(else_row);
                    let line_count = self.rope.line_of_offset(self.rope.len()) + 1;
                    let delete_end = if endif_row < line_count {
                        self.rope.offset_of_line(endif_row)
                    } else {
                        self.rope.len()
                    };
                    let delete_range = Range::from_byte_offsets(
                        &self.rope,
                        delete_start,
                        delete_end,
                    );

                    self.diagnostics.push(Diagnostic {
                        range: diag_range,
                        message: crate::util::i18n::empty_else().to_string(),
                        severity: Some(DiagnosticSeverity::Hint),
                        tags: Some(vec![crate::http::diagnostic::DiagnosticTag::Unnecessary]),
                        source: Some("jass".into()),
                        code: Some(DiagnosticCode::String("empty-else".into())),
                        data: Some(serde_json::json!({
                            "empty_else_delete_range": delete_range,
                        })),
                        ..Default::default()
                    });
                }

                // Recurse into if body and branches.
                let if_stmt = if_stmt.clone();
                self.check_empty_else(&if_stmt.body);
                for branch in &if_stmt.branches {
                    self.check_empty_else(&branch.body);
                }
            } else if let Statement::Loop(l) = stmt {
                let body = l.body.clone();
                self.check_empty_else(&body);
            }
        }
    }

    // ─── Dead code detection ──────────────────────────────────────────

    /// Walk a statement list looking for code after an unconditional `return`.
    ///
    /// Emits a `Hint` diagnostic with `Unnecessary` tag on each unreachable
    /// statement, and recurses into if/loop bodies.
    pub(super) fn check_dead_code(&mut self, stmts: &[Statement]) {
        let mut found_return = false;
        for stmt in stmts {
            if found_return {
                // Skip comments/directives — don't flag them as dead.
                match stmt {
                    Statement::Comment(_) | Statement::Import(_)
                    | Statement::SetDir(_) | Statement::IgnoreDir(_)
                    | Statement::UjapiImport(_) | Statement::EntryDir(_) => continue,
                    _ => {}
                }
                let node = Self::stmt_node(stmt);
                self.diagnostics.push(Diagnostic {
                    range: node.to_range(&self.rope),
                    message: crate::util::i18n::dead_code().to_string(),
                    severity: Some(DiagnosticSeverity::Hint),
                    tags: Some(vec![crate::http::diagnostic::DiagnosticTag::Unnecessary]),
                    ..Diagnostic::new("jass", "dead-code")
                });
                // Still recurse so we don't miss nested checks,
                // but don't set found_return again.
                continue;
            }
            if matches!(stmt, Statement::Return(_)) {
                found_return = true;
            }
            // Recurse into nested bodies.
            match stmt {
                Statement::If(if_stmt) => {
                    let if_stmt = if_stmt.clone();
                    self.check_dead_code(&if_stmt.body);
                    for branch in &if_stmt.branches {
                        self.check_dead_code(&branch.body);
                    }
                }
                Statement::Loop(l) => {
                    let body = l.body.clone();
                    self.check_dead_code(&body);
                }
                _ => {}
            }
        }
    }

    // ─── Collapse and-chain detection ─────────────────────────────────

    /// Detect chains of `if not(cond) then return false endif` followed
    /// by a final `return <expr>` at the end of a statement list.
    ///
    /// Replacement: `return cond1 and cond2 and … and exprN`
    pub(super) fn check_and_chain_pattern(&mut self, stmts: &[Statement]) {
        let n = stmts.len();
        if n < 2 {
            return;
        }

        // The last statement must be a `return <expr>`.
        let tail_ret = match &stmts[n - 1] {
            Statement::Return(r) => r,
            _ => return,
        };
        let tail_expr = match &tail_ret.value {
            Some(e) => e,
            None => return,
        };

        let mut cond_texts: Vec<String> = Vec::new();
        let mut first_if_idx: Option<usize> = None;

        let mut idx = n - 2;
        loop {
            let if_stmt = match &stmts[idx] {
                Statement::If(s) => s,
                _ => break,
            };
            if !if_stmt.branches.is_empty() || if_stmt.body.len() != 1 {
                break;
            }
            let body_ret = match &if_stmt.body[0] {
                Statement::Return(r) => r,
                _ => break,
            };
            let body_val = match &body_ret.value {
                Some(v) => v,
                None => break,
            };
            if self.as_bool_literal(body_val) != Some(false) {
                break;
            }
            let cond = match &if_stmt.condition {
                Some(c) => c,
                None => break,
            };
            let inner_text = match self.try_strip_not(cond) {
                Some(t) => t,
                None => break,
            };

            cond_texts.push(inner_text);
            first_if_idx = Some(idx);

            if idx == 0 {
                break;
            }
            idx -= 1;
        }

        if cond_texts.is_empty() {
            return;
        }

        if cond_texts.len() == 1 && self.as_bool_literal(tail_expr).is_some() {
            return;
        }

        cond_texts.reverse();

        let tail_text = self.expr_text(&Self::unwrap_parens(tail_expr));
        let mut parts = cond_texts;
        parts.push(tail_text);
        let new_text = format!("return {}", parts.join(" and "));

        let first_idx = first_if_idx.unwrap();
        let start_byte = match &stmts[first_idx] {
            Statement::If(s) => s.node.start_byte(),
            _ => return,
        };
        let end_byte = tail_ret.node.end_byte();
        let range = Range::from_byte_offsets(&self.rope, start_byte, end_byte);

        self.diagnostics.push(Diagnostic {
            range,
            message: crate::util::i18n::collapse_and_chain().to_string(),
            severity: Some(DiagnosticSeverity::Hint),
            source: Some("jass".into()),
            code: Some(DiagnosticCode::String("collapse-and".into())),
            data: Some(serde_json::json!({
                "collapse_and_new_text": new_text,
            })),
            ..Default::default()
        });
    }

    /// If `expr` is `not <inner>`, return the source text of `<inner>`
    /// with redundant outer parentheses stripped.
    pub(super) fn try_strip_not(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Unary { node, operand } => {
                let is_not = node
                    .child(0)
                    .and_then(|c| Kind::try_from(c.grammar_id()).ok())
                    .map_or(false, |k| k == Kind::Not);
                if is_not {
                    Some(self.expr_text(&Self::unwrap_parens(operand)))
                } else {
                    None
                }
            }
            Expr::Parens { inner, .. } => self.try_strip_not(inner),
            _ => None,
        }
    }

    /// Recursively strip outer `Parens` wrappers from an expression.
    pub(super) fn unwrap_parens<'a, 'x>(expr: &'a Expr<'x>) -> &'a Expr<'x> {
        match expr {
            Expr::Parens { inner, .. } => Self::unwrap_parens(inner),
            other => other,
        }
    }

    /// Walk a statement list looking for and-chain patterns.
    /// Also recurses into nested `if`/`loop` bodies.
    pub(super) fn check_and_chains(&mut self, stmts: &[Statement]) {
        self.check_and_chain_pattern(stmts);

        for stmt in stmts {
            match stmt {
                Statement::If(if_stmt) => {
                    let if_stmt = if_stmt.clone();
                    self.check_and_chains(&if_stmt.body);
                    for branch in &if_stmt.branches {
                        self.check_and_chains(&branch.body);
                    }
                }
                Statement::Loop(loop_stmt) => {
                    let body = loop_stmt.body.clone();
                    self.check_and_chains(&body);
                }
                _ => {}
            }
        }
    }

    // ─── Collapse or-chain detection ──────────────────────────────────

    /// Check whether an expression (after unwrapping parens) contains `and`
    /// at the top level of its binary tree.
    pub(super) fn expr_contains_and(expr: &Expr) -> bool {
        let expr = Self::unwrap_parens(expr);
        match expr {
            Expr::Binary { node, left, right } => {
                let op = Self::binary_op_kind(node);
                if op == Some(Kind::And) {
                    return true;
                }
                Self::expr_contains_and(left) || Self::expr_contains_and(right)
            }
            _ => false,
        }
    }

    /// Return the source text of `expr` suitable for use as an `or` operand.
    pub(super) fn or_operand_text(&self, expr: &Expr) -> String {
        let inner = Self::unwrap_parens(expr);
        let text = self.expr_text(inner);
        if Self::expr_contains_and(inner) {
            format!("({})", text)
        } else {
            text
        }
    }

    /// Detect chains of `if (cond) then return true endif` followed
    /// by a final `return <expr>` at the end of a statement list.
    ///
    /// Replacement: `return cond1 or cond2 or … or exprN`
    pub(super) fn check_or_chain_pattern(&mut self, stmts: &[Statement]) {
        let n = stmts.len();
        if n < 2 {
            return;
        }

        let tail_ret = match &stmts[n - 1] {
            Statement::Return(r) => r,
            _ => return,
        };
        let tail_expr = match &tail_ret.value {
            Some(e) => e,
            None => return,
        };

        let mut cond_texts: Vec<String> = Vec::new();
        let mut first_if_idx: Option<usize> = None;

        let mut idx = n - 2;
        loop {
            let if_stmt = match &stmts[idx] {
                Statement::If(s) => s,
                _ => break,
            };
            if !if_stmt.branches.is_empty() || if_stmt.body.len() != 1 {
                break;
            }
            let body_ret = match &if_stmt.body[0] {
                Statement::Return(r) => r,
                _ => break,
            };
            let body_val = match &body_ret.value {
                Some(v) => v,
                None => break,
            };
            if self.as_bool_literal(body_val) != Some(true) {
                break;
            }
            let cond = match &if_stmt.condition {
                Some(c) => c,
                None => break,
            };

            let cond_text = self.or_operand_text(cond);
            cond_texts.push(cond_text);
            first_if_idx = Some(idx);

            if idx == 0 {
                break;
            }
            idx -= 1;
        }

        if cond_texts.is_empty() {
            return;
        }

        if cond_texts.len() == 1 && self.as_bool_literal(tail_expr).is_some() {
            return;
        }

        cond_texts.reverse();

        let tail_text = self.or_operand_text(tail_expr);
        let mut parts = cond_texts;
        parts.push(tail_text);
        let new_text = format!("return {}", parts.join(" or "));

        let first_idx = first_if_idx.unwrap();
        let start_byte = match &stmts[first_idx] {
            Statement::If(s) => s.node.start_byte(),
            _ => return,
        };
        let end_byte = tail_ret.node.end_byte();
        let range = Range::from_byte_offsets(&self.rope, start_byte, end_byte);

        self.diagnostics.push(Diagnostic {
            range,
            message: crate::util::i18n::collapse_or_chain().to_string(),
            severity: Some(DiagnosticSeverity::Hint),
            source: Some("jass".into()),
            code: Some(DiagnosticCode::String("collapse-or".into())),
            data: Some(serde_json::json!({
                "collapse_or_new_text": new_text,
            })),
            ..Default::default()
        });
    }

    /// Walk a statement list looking for or-chain patterns.
    /// Also recurses into nested `if`/`loop` bodies.
    pub(super) fn check_or_chains(&mut self, stmts: &[Statement]) {
        self.check_or_chain_pattern(stmts);

        for stmt in stmts {
            match stmt {
                Statement::If(if_stmt) => {
                    let if_stmt = if_stmt.clone();
                    self.check_or_chains(&if_stmt.body);
                    for branch in &if_stmt.branches {
                        self.check_or_chains(&branch.body);
                    }
                }
                Statement::Loop(loop_stmt) => {
                    let body = loop_stmt.body.clone();
                    self.check_or_chains(&body);
                }
                _ => {}
            }
        }
    }
}
