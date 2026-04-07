#[cfg(test)]
mod tests {
    use crate::http::file_rename::{
        find_import_edits, find_import_edits_for_target, find_self_move_edits,
        is_absolute_import, pathdiff_relative,
    };
    use std::path::Path;
    use url::Url;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    // ─── pathdiff ────────────────────────────────────────────────────────

    #[test]
    fn pathdiff_same_dir() {
        let base = Path::new("/project/src");
        let target = Path::new("/project/src/file.j");
        let rel = pathdiff_relative(base, target).unwrap();
        assert_eq!(rel.to_string_lossy(), "file.j");
    }

    #[test]
    fn pathdiff_parent() {
        let base = Path::new("/project/src/sub");
        let target = Path::new("/project/src/file.j");
        let rel = pathdiff_relative(base, target).unwrap();
        assert_eq!(rel.to_string_lossy(), "../file.j");
    }

    #[test]
    fn pathdiff_sibling_dir() {
        let base = Path::new("/project/src");
        let target = Path::new("/project/lib/file.j");
        let rel = pathdiff_relative(base, target).unwrap();
        assert_eq!(rel.to_string_lossy(), "../lib/file.j");
    }

    // ─── is_absolute_import ──────────────────────────────────────────────

    #[test]
    fn absolute_unix() {
        assert!(is_absolute_import("/home/user/file.j"));
    }

    #[test]
    fn absolute_windows() {
        assert!(is_absolute_import("C:/project/file.j"));
        assert!(is_absolute_import("D:\\maps\\file.j"));
    }

    #[test]
    fn relative_simple() {
        assert!(!is_absolute_import("lib/file.j"));
        assert!(!is_absolute_import("../lib/file.j"));
        assert!(!is_absolute_import("./file.j"));
    }

    // ─── find_import_edits (backward-compat wrapper) ─────────────────────

    #[test]
    fn find_edits_basic() {
        let text = "//import old/path.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let dep = u("file:///project/src/main.j");
        let old = u("file:///project/src/old/path.j");
        let new_rel = "new/path.j";

        let edits = find_import_edits(text, &dep, &old, new_rel);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "new/path.j");
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].range.start.character, 9); // after "//import "
        assert_eq!(edits[0].range.end.character, 19); // end of "old/path.j"
    }

    #[test]
    fn find_edits_frozen() {
        let text = "//import! old/path.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let dep = u("file:///project/src/main.j");
        let old = u("file:///project/src/old/path.j");
        let new_rel = "new/path.j";

        let edits = find_import_edits(text, &dep, &old, new_rel);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.character, 10); // after "//import! "
    }

    #[test]
    fn find_edits_skips_non_matching() {
        let text = "//import other/file.j\n//import old/path.j\ntype X extends Y\n";
        let dep = u("file:///project/src/main.j");
        let old = u("file:///project/src/old/path.j");
        let new_rel = "new/path.j";

        let edits = find_import_edits(text, &dep, &old, new_rel);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 1); // second line
    }

    #[test]
    fn find_edits_stops_at_code() {
        let text = "//import old/path.j\ntype X extends Y\n//import old/path.j\n";
        let dep = u("file:///project/src/main.j");
        let old = u("file:///project/src/old/path.j");
        let new_rel = "new/path.j";

        let edits = find_import_edits(text, &dep, &old, new_rel);
        // Only the first import should be found; the one after `type` is past code.
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 0);
    }

    #[test]
    fn find_edits_no_match() {
        let text = "//import other/file.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let dep = u("file:///project/src/main.j");
        let old = u("file:///project/src/old/path.j");
        let new_rel = "new/path.j";

        let edits = find_import_edits(text, &dep, &old, new_rel);
        assert!(edits.is_empty());
    }

    // ─── find_import_edits_for_target (absolute path preservation) ───────

    #[test]
    fn dependent_preserves_relative_import_on_move() {
        // dep imports moved file via relative path.
        let text = "//import lib/utils.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let dep = u("file:///project/src/main.j");
        let old_target = u("file:///project/src/lib/utils.j");
        let new_target = u("file:///project/vendor/utils.j");

        let edits = find_import_edits_for_target(text, &dep, &old_target, &new_target);
        assert_eq!(edits.len(), 1);
        // The new path should be relative from /project/src/ to /project/vendor/utils.j
        assert_eq!(edits[0].new_text, "../vendor/utils.j");
    }

    #[test]
    fn dependent_preserves_absolute_import_on_move() {
        // dep imports moved file via absolute path.
        let text =
            "//import /project/src/lib/utils.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let dep = u("file:///project/src/main.j");
        let old_target = u("file:///project/src/lib/utils.j");
        let new_target = u("file:///project/vendor/utils.j");

        let edits = find_import_edits_for_target(text, &dep, &old_target, &new_target);
        assert_eq!(edits.len(), 1);
        // Absolute stays absolute — now points to new location.
        assert_eq!(edits[0].new_text, "/project/vendor/utils.j");
    }

    // ─── find_self_move_edits (the moved file's own imports) ─────────────

    #[test]
    fn self_move_rewrites_relative_imports() {
        // File has a relative import; it's moved to a different directory.
        let text = "//import ../common/defs.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let old_self = u("file:///project/src/main.j");
        let new_self = u("file:///project/src/sub/main.j");

        let edits = find_self_move_edits(text, &old_self, &new_self);
        assert_eq!(edits.len(), 1);
        // From /project/src/sub/ to /project/common/defs.j → ../../common/defs.j
        assert_eq!(edits[0].new_text, "../../common/defs.j");
    }

    #[test]
    fn self_move_skips_absolute_imports() {
        // Absolute import should NOT be rewritten when the file moves.
        let text = "//import /project/common/defs.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let old_self = u("file:///project/src/main.j");
        let new_self = u("file:///project/src/sub/main.j");

        let edits = find_self_move_edits(text, &old_self, &new_self);
        assert!(edits.is_empty());
    }

    #[test]
    fn self_move_no_change_when_same_dir() {
        // Moving file within the same directory (rename) should produce no
        // self-edits because relative paths still resolve the same.
        let text = "//import lib/utils.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let old_self = u("file:///project/src/main.j");
        let new_self = u("file:///project/src/main2.j");

        let edits = find_self_move_edits(text, &old_self, &new_self);
        assert!(edits.is_empty());
    }

    #[test]
    fn self_move_frozen_import() {
        // //import! with relative path also gets rewritten on move.
        let text = "//import! ../common/defs.j\nfunction F takes nothing returns nothing\nendfunction\n";
        let old_self = u("file:///project/src/main.j");
        // Move deeper — from /project/src/ to /project/src/deep/sub/
        let new_self = u("file:///project/src/deep/sub/main.j");

        let edits = find_self_move_edits(text, &old_self, &new_self);
        assert_eq!(edits.len(), 1);
        // Old resolves: /project/src/../common/defs.j → /project/common/defs.j
        // New relative: from /project/src/deep/sub/ to /project/common/defs.j → ../../../common/defs.j
        assert_eq!(edits[0].new_text, "../../../common/defs.j");
    }
}

