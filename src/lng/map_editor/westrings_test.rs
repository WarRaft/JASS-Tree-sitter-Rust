#[cfg(test)]
mod tests {
    use crate::lng::map_editor::westrings::*;
    use std::collections::HashMap;

    #[test]
    fn basic_parse() {
        let data = b"[WorldEditStrings]\nWESTRING_FOO=Bar\nWESTRING_BAZ=Hello World\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("WESTRING_FOO").map(|s| s.as_str()), Some("Bar"));
        assert_eq!(
            map.get("WESTRING_BAZ").map(|s| s.as_str()),
            Some("Hello World")
        );
    }

    #[test]
    fn skip_comments_and_sections() {
        let data = b"// comment\n[Section]\nKEY=val\n//another\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("KEY").map(|s| s.as_str()), Some("val"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn bom_handling() {
        let data = b"\xEF\xBB\xBF[WorldEditStrings]\nWESTRING_X=Y\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("WESTRING_X").map(|s| s.as_str()), Some("Y"));
    }

    #[test]
    fn strip_surrounding_quotes() {
        let data = b"[WorldEditStrings]\nWESTRING_Q=\"City Building (Diagonal 1, Red)\"\nWESTRING_P=Plain\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("WESTRING_Q").map(|s| s.as_str()), Some("City Building (Diagonal 1, Red)"));
        assert_eq!(map.get("WESTRING_P").map(|s| s.as_str()), Some("Plain"));
    }

    #[test]
    fn resolve_chain() {
        // Reset cache
        invalidate();
        {
            let mut guard = WESTRINGS.lock().unwrap();
            let mut map = HashMap::new();
            map.insert("WESTRING_A".into(), ("WESTRING_B".into(), "file1.txt".into()));
            map.insert("WESTRING_B".into(), ("Final Value".into(), "file2.txt".into()));
            *guard = Some(map);
        }
        assert_eq!(resolve_value("WESTRING_A"), "Final Value");
        assert_eq!(resolve_value("plain text"), "plain text");

        let gs = resolve_game_string("WESTRING_A");
        assert_eq!(gs.value, "Final Value");
        assert_eq!(gs.original, "WESTRING_A");
        assert_eq!(gs.source, "file2.txt");
        assert!(gs.is_resolved());

        let gs_plain = resolve_game_string("plain text");
        assert_eq!(gs_plain.value, "plain text");
        assert!(!gs_plain.is_resolved());

        // Cleanup
        invalidate();
    }

    #[test]
    fn game_string_json_plain() {
        let gs = GameString::plain("hello".into());
        let json = serde_json::to_string(&gs).unwrap();
        assert_eq!(json, "\"hello\"");
    }

    #[test]
    fn game_string_json_resolved() {
        let gs = GameString {
            value: "Bridge".into(),
            original: "WESTRING_GE_BRIDGE".into(),
            source: "WorldEditStrings.txt".into(),
        };
        let json = serde_json::to_string(&gs).unwrap();
        assert!(json.contains("\"value\":\"Bridge\""));
        assert!(json.contains("\"original\":\"WESTRING_GE_BRIDGE\""));
        assert!(json.contains("\"source\":\"WorldEditStrings.txt\""));
    }
}

