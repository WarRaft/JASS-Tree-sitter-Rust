#[cfg(test)]
mod tests {
    use crate::util::scope_resolver::{GlobalEntry, ScopeResolver, SymbolNS};
    use std::collections::HashSet;
    use url::Url;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn func_entry(uri: &Url, name: &str, decl_key: usize) -> GlobalEntry {
        GlobalEntry {
            uri: uri.clone(),
            name: name.into(),
            namespace: String::new(),
            ns: SymbolNS::Func,
            decl_key,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: None,
        }
    }

    fn var_entry(uri: &Url, name: &str, decl_key: usize) -> GlobalEntry {
        GlobalEntry {
            uri: uri.clone(),
            name: name.into(),
            namespace: String::new(),
            ns: SymbolNS::Var,
            decl_key,
            type_name: Some("integer".into()),
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: None,
        }
    }

    fn type_entry(uri: &Url, name: &str, decl_key: usize) -> GlobalEntry {
        GlobalEntry {
            uri: uri.clone(),
            name: name.into(),
            namespace: String::new(),
            ns: SymbolNS::Var, // types share the var namespace in JASS
            decl_key,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: None,
        }
    }

    // ─── update_file / resolve ───────────────────────────────────────────

    #[test]
    fn update_and_resolve_by_name() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let hash = [0u8; 32];

        sr.update_file(
            &a,
            hash,
            vec![
                func_entry(&a, "Foo", 0),
                var_entry(&a, "bar", 10),
            ],
        );

        let visible = HashSet::from([a.clone()]);

        let funcs = sr.resolve("Foo", SymbolNS::Func, &visible);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "Foo");
        assert_eq!(funcs[0].decl_key, 0);

        let vars = sr.resolve("bar", SymbolNS::Var, &visible);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "bar");

        // Wrong namespace → empty
        assert!(sr.resolve("Foo", SymbolNS::Var, &visible).is_empty());
        assert!(sr.resolve("bar", SymbolNS::Func, &visible).is_empty());
    }

    #[test]
    fn resolve_filters_by_visible_uris() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let hash = [0u8; 32];

        sr.update_file(&a, hash, vec![func_entry(&a, "Foo", 0)]);
        sr.update_file(&b, hash, vec![func_entry(&b, "Foo", 5)]);

        // Only a is visible
        let visible_a = HashSet::from([a.clone()]);
        let result = sr.resolve("Foo", SymbolNS::Func, &visible_a);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].uri, a);

        // Both visible
        let visible_both = HashSet::from([a.clone(), b.clone()]);
        let result = sr.resolve("Foo", SymbolNS::Func, &visible_both);
        assert_eq!(result.len(), 2);
    }

    // ─── remove_file ─────────────────────────────────────────────────────

    #[test]
    fn remove_file_cleans_up() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let hash = [0u8; 32];

        sr.update_file(
            &a,
            hash,
            vec![func_entry(&a, "Foo", 0), var_entry(&a, "bar", 10)],
        );
        assert_eq!(sr.file_count(), 1);
        assert_eq!(sr.symbol_count(), 2);

        sr.remove_file(&a);
        assert_eq!(sr.file_count(), 0);
        assert_eq!(sr.symbol_count(), 0);

        let visible = HashSet::from([a.clone()]);
        assert!(sr.resolve("Foo", SymbolNS::Func, &visible).is_empty());
    }

    #[test]
    fn remove_file_preserves_other_files() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let hash = [0u8; 32];

        sr.update_file(&a, hash, vec![func_entry(&a, "Foo", 0)]);
        sr.update_file(&b, hash, vec![func_entry(&b, "Foo", 5)]);
        assert_eq!(sr.symbol_count(), 2);

        sr.remove_file(&a);
        assert_eq!(sr.symbol_count(), 1);

        let visible = HashSet::from([b.clone()]);
        let result = sr.resolve("Foo", SymbolNS::Func, &visible);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].uri, b);
    }

    // ─── update replaces old entries ─────────────────────────────────────

    #[test]
    fn update_replaces_old_entries() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];

        sr.update_file(&a, hash1, vec![func_entry(&a, "Foo", 0)]);
        assert_eq!(sr.symbol_count(), 1);

        // Update with different symbols
        sr.update_file(
            &a,
            hash2,
            vec![func_entry(&a, "Bar", 10), var_entry(&a, "baz", 20)],
        );
        assert_eq!(sr.symbol_count(), 2);

        let visible = HashSet::from([a.clone()]);
        assert!(sr.resolve("Foo", SymbolNS::Func, &visible).is_empty());
        assert_eq!(sr.resolve("Bar", SymbolNS::Func, &visible).len(), 1);
        assert_eq!(sr.resolve("baz", SymbolNS::Var, &visible).len(), 1);
    }

    // ─── is_stale ────────────────────────────────────────────────────────

    #[test]
    fn is_stale_unknown_uri() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        assert!(sr.is_stale(&a, &[0u8; 32]));
    }

    #[test]
    fn is_stale_matching_hash() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let hash = [42u8; 32];

        sr.update_file(&a, hash, vec![func_entry(&a, "Foo", 0)]);
        assert!(!sr.is_stale(&a, &hash));
    }

    #[test]
    fn is_stale_different_hash() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];

        sr.update_file(&a, hash1, vec![func_entry(&a, "Foo", 0)]);
        assert!(sr.is_stale(&a, &hash2));
    }

    // ─── all_visible ─────────────────────────────────────────────────────

    #[test]
    fn all_visible_returns_filtered_entries() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        let hash = [0u8; 32];

        sr.update_file(&a, hash, vec![func_entry(&a, "Foo", 0)]);
        sr.update_file(
            &b,
            hash,
            vec![
                func_entry(&b, "Bar", 5),
                var_entry(&b, "x", 10),
            ],
        );
        sr.update_file(&c, hash, vec![type_entry(&c, "unit", 20)]);

        // Only b and c visible
        let visible = HashSet::from([b.clone(), c.clone()]);
        let result = sr.all_visible(&visible);
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|e| e.uri != a));
    }

    // ─── gc ──────────────────────────────────────────────────────────────

    #[test]
    fn gc_removes_unlisted_files() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let hash = [0u8; 32];

        sr.update_file(&a, hash, vec![func_entry(&a, "Foo", 0)]);
        sr.update_file(&b, hash, vec![func_entry(&b, "Bar", 5)]);
        assert_eq!(sr.file_count(), 2);

        let keep = HashSet::from([a.clone()]);
        sr.gc(&keep);
        assert_eq!(sr.file_count(), 1);
        assert_eq!(sr.symbol_count(), 1);

        let visible = HashSet::from([a.clone(), b.clone()]);
        assert_eq!(sr.resolve("Foo", SymbolNS::Func, &visible).len(), 1);
        assert!(sr.resolve("Bar", SymbolNS::Func, &visible).is_empty());
    }

    // ─── overloaded names across files ───────────────────────────────────

    #[test]
    fn same_name_different_files_both_visible() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let hash = [0u8; 32];

        sr.update_file(&a, hash, vec![func_entry(&a, "Init", 0)]);
        sr.update_file(&b, hash, vec![func_entry(&b, "Init", 0)]);

        let visible = HashSet::from([a.clone(), b.clone()]);
        let result = sr.resolve("Init", SymbolNS::Func, &visible);
        assert_eq!(result.len(), 2);
    }

    // ─── same name, different namespaces ─────────────────────────────────

    #[test]
    fn same_name_different_ns() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");
        let hash = [0u8; 32];

        sr.update_file(
            &a,
            hash,
            vec![
                func_entry(&a, "A", 0),
                var_entry(&a, "A", 10),
            ],
        );

        let visible = HashSet::from([a.clone()]);
        assert_eq!(sr.resolve("A", SymbolNS::Func, &visible).len(), 1);
        assert_eq!(sr.resolve("A", SymbolNS::Var, &visible).len(), 1);
    }

    // ─── export_fingerprint ──────────────────────────────────────────────

    #[test]
    fn fingerprint_unknown_uri() {
        let sr = ScopeResolver::new_empty();
        assert!(sr.export_fingerprint(&u("file:///unknown.j")).is_none());
    }

    #[test]
    fn fingerprint_stable_for_same_exports() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");

        sr.update_file(&a, [1u8; 32], vec![func_entry(&a, "Foo", 0)]);
        let fp1 = sr.export_fingerprint(&a).unwrap();

        // Same exports, different hash → fingerprint should be the same
        sr.update_file(&a, [2u8; 32], vec![func_entry(&a, "Foo", 100)]);
        let fp2 = sr.export_fingerprint(&a).unwrap();
        assert_eq!(fp1, fp2, "fingerprint should not change if exports are the same");
    }

    #[test]
    fn fingerprint_changes_on_add() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");

        sr.update_file(&a, [1u8; 32], vec![func_entry(&a, "Foo", 0)]);
        let fp1 = sr.export_fingerprint(&a).unwrap();

        sr.update_file(
            &a,
            [2u8; 32],
            vec![func_entry(&a, "Foo", 0), func_entry(&a, "Bar", 10)],
        );
        let fp2 = sr.export_fingerprint(&a).unwrap();
        assert_ne!(fp1, fp2, "adding a symbol should change fingerprint");
    }

    #[test]
    fn fingerprint_changes_on_remove() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");

        sr.update_file(
            &a,
            [1u8; 32],
            vec![func_entry(&a, "Foo", 0), func_entry(&a, "Bar", 10)],
        );
        let fp1 = sr.export_fingerprint(&a).unwrap();

        sr.update_file(&a, [2u8; 32], vec![func_entry(&a, "Foo", 0)]);
        let fp2 = sr.export_fingerprint(&a).unwrap();
        assert_ne!(fp1, fp2, "removing a symbol should change fingerprint");
    }

    #[test]
    fn fingerprint_changes_on_ns_change() {
        let sr = ScopeResolver::new_empty();
        let a = u("file:///a.j");

        sr.update_file(&a, [1u8; 32], vec![func_entry(&a, "A", 0)]);
        let fp1 = sr.export_fingerprint(&a).unwrap();

        sr.update_file(&a, [2u8; 32], vec![var_entry(&a, "A", 0)]);
        let fp2 = sr.export_fingerprint(&a).unwrap();
        assert_ne!(fp1, fp2, "changing namespace should change fingerprint");
    }
}
