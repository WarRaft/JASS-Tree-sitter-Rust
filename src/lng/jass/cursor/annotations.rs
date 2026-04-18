use std::collections::HashSet;
use lapce_xi_rope::Rope;

/// Annotations extracted from the comment block above a declaration.
pub struct CommentAnnotations {
    pub doc_comment: Option<String>,
    pub ignore_tags: HashSet<String>,
}

/// Strip the `//*` prefix from a single comment line and return the doc text.
fn strip_doc_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("//*") {
        return None;
    }
    let after = &trimmed[3..];
    if after.starts_with(' ') {
        Some(&after[1..])
    } else {
        Some(after)
    }
}

/// Strip the `//@ignore` prefix and return the list of tags.
fn strip_ignore_prefix(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("//@ignore") {
        return None;
    }
    let after = &trimmed["//@ignore".len()..];
    if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
        let tags: Vec<&str> = after.split_whitespace().collect();
        Some(tags)
    } else {
        None
    }
}

/// Extract annotations (`//*` doc comment and `//@ignore` tags) from the
/// comment block directly above a declaration at `row`.
pub fn extract_annotations(rope: &Rope, row: usize) -> CommentAnnotations {
    let mut doc_lines = Vec::new();
    let mut ignore_tags = HashSet::new();

    if row == 0 {
        return CommentAnnotations { doc_comment: None, ignore_tags };
    }
    let line_count = rope.line_of_offset(rope.len()) + 1;
    let mut r = row;
    while r > 0 {
        r -= 1;
        if r >= line_count {
            break;
        }
        let line_start = rope.offset_of_line(r);
        let line_end = if r + 1 < line_count {
            rope.offset_of_line(r + 1)
        } else {
            rope.len()
        };
        let text = rope.slice_to_cow(line_start..line_end);
        let text = text.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(doc) = strip_doc_prefix(text) {
            doc_lines.push(doc.to_string());
        } else if let Some(tags) = strip_ignore_prefix(text) {
            for tag in tags {
                ignore_tags.insert(tag.to_string());
            }
        } else {
            break;
        }
    }

    let doc_comment = if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    };

    CommentAnnotations { doc_comment, ignore_tags }
}

