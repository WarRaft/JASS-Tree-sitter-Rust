#[cfg(test)]
mod tests {
    use crate::lng::jass::symbol::ShortNameGen;

    #[test]
    fn short_names_first_52() {
        let mut g = ShortNameGen::new();
        let first = g.next();
        assert_eq!(first, "a");
        // Collect 51 more
        let mut names: Vec<String> = vec![first];
        for _ in 0..51 {
            names.push(g.next());
        }
        // Should be a..z, A..Z
        assert_eq!(names[0], "a");
        assert_eq!(names[25], "z");
        assert_eq!(names[26], "A");
        assert_eq!(names[51], "Z");
        // All unique
        let set: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(set.len(), 52);
    }

    #[test]
    fn short_names_skip_reserved() {
        let mut g = ShortNameGen::new();
        let mut names: Vec<String> = Vec::new();
        for _ in 0..200 {
            names.push(g.next());
        }
        // None should be a JASS reserved word
        let reserved = [
            "and", "array", "call", "constant", "debug", "else", "elseif",
            "endfunction", "endglobals", "endif", "endloop", "extends",
            "false", "function", "globals", "if", "local", "loop", "native",
            "not", "nothing", "null", "or", "return", "returns", "set",
            "takes", "then", "true", "type",
        ];
        for name in &names {
            assert!(
                !reserved.contains(&name.as_str()),
                "Generated reserved word: {}",
                name
            );
        }
    }

    #[test]
    fn short_names_all_unique() {
        let mut g = ShortNameGen::new();
        let mut set = std::collections::HashSet::new();
        for _ in 0..1000 {
            let name = g.next();
            assert!(set.insert(name.clone()), "Duplicate name: {}", name);
        }
    }

    #[test]
    fn short_names_valid_identifiers() {
        let mut g = ShortNameGen::new();
        for _ in 0..500 {
            let name = g.next();
            assert!(!name.is_empty());
            let first = name.chars().next().unwrap();
            assert!(
                first.is_ascii_alphabetic(),
                "First char of '{}' is not alpha",
                name
            );
            for ch in name.chars().skip(1) {
                assert!(
                    ch.is_ascii_alphanumeric(),
                    "Char '{}' in '{}' is not alphanumeric",
                    ch,
                    name
                );
            }
        }
    }
}

