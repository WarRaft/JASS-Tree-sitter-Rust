use super::test_support::*;

    #[test]
    fn set_hint_type_recognized() {
        let src = "//set hint type\nglobals\n    integer x = 5\nendglobals\n";
        with_cursor(src, |c| {
            assert_eq!(c.file_settings.get("hint").map(|v| v.as_str()), Some("type"));
        });
    }

    #[test]
    fn set_hint_ref_type_recognized() {
        let src = "//set hint ref type\nglobals\n    integer x = 5\nendglobals\n";
        with_cursor(src, |c| {
            assert_eq!(c.file_settings.get("hint").map(|v| v.as_str()), Some("ref type"));
        });
    }

    #[test]
    fn set_hint_invalid_value_warns() {
        let src = "//set hint banana\nglobals\n    integer x = 5\nendglobals\n";
        with_cursor(src, |c| {
            // The value is still stored (for forward-compat)
            assert_eq!(c.file_settings.get("hint").map(|v| v.as_str()), Some("banana"));
            // But a warning diagnostic should be emitted
            let has_warning = c.diagnostics.iter().any(|d| {
                d.message.contains("Unknown value") || d.message.contains("unknown")
            });
            assert!(has_warning, "should warn about unknown tag value, diagnostics: {:?}",
                c.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());
        });
    }

    #[test]
    fn set_def_registry_has_hint() {
        let def = crate::lng::directive::find_set_def("hint");
        assert!(def.is_some(), "hint should be in SET_DEFS");
        let def = def.unwrap();
        assert!(matches!(def.kind, crate::lng::directive::SetValueKind::Tags(_)));
        assert_eq!(def.default, "");
    }

    #[test]
    fn set_def_registry_has_all_known_keys() {
        for key in &["hint", "build-jass", "build-as"] {
            assert!(
                crate::lng::directive::find_set_def(key).is_some(),
                "SET_DEFS should contain {:?}",
                key
            );
        }
    }

    #[test]
    fn set_validate_hint_accepts_known_tags() {
        let def = crate::lng::directive::find_set_def("hint").unwrap();
        assert!(crate::lng::directive::validate_set_value(def, "ref").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "type").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "ref type").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "banana").is_some());
    }

    #[test]
    fn set_validate_path_accepts_anything() {
        let def = crate::lng::directive::find_set_def("build-jass").unwrap();
        assert!(crate::lng::directive::validate_set_value(def, "./output.j").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "C:\\build\\out.j").is_none());
    }
