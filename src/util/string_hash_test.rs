#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::util::string_hash::{
        blizzard_string_hash, collect_constants, eval_const_expr, fold_string_hash,
        fold_string_integer_args, find_matching_paren, split_call_args, ConstValue,
    };

    fn empty_map() -> HashMap<String, ConstValue> { HashMap::new() }

    // ─── Hash correctness ────────────────────────────────────────────────────

    #[test]
    fn case_insensitive() {
        assert_eq!(blizzard_string_hash("abc"), blizzard_string_hash("ABC"));
        assert_eq!(blizzard_string_hash("Hello"), blizzard_string_hash("HELLO"));
    }

    #[test]
    fn slash_normalized() {
        assert_eq!(
            blizzard_string_hash("Units/Human"),
            blizzard_string_hash("Units\\Human")
        );
    }

    /// Verify exact values against the C `SStrHash2` reference implementation.
    #[test]
    fn matches_c_reference() {
        assert_eq!(blizzard_string_hash("Hello"), -1801350911_i32);
        assert_eq!(blizzard_string_hash("abc"), 1043745117_i32);
        assert_eq!(blizzard_string_hash("ABC"), 1043745117_i32);
        assert_eq!(blizzard_string_hash(""), -1119235827_i32);
        assert_eq!(blizzard_string_hash("test"), -310027398_i32);
        assert_eq!(
            blizzard_string_hash("SomeVeryLongStringThatExceeds12Bytes"),
            -657707918_i32
        );
    }

    // ─── Expression evaluator ────────────────────────────────────────────────

    #[test]
    fn eval_string_literal() {
        let v = eval_const_expr(r#""hello""#, &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Str(s) if s == "hello"));
    }

    #[test]
    fn eval_integer_literal() {
        let v = eval_const_expr("42", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(42)));
    }

    #[test]
    fn eval_hex_literal() {
        let v = eval_const_expr("0xFF", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(255)));
    }

    #[test]
    fn eval_jass_hex_literal() {
        let v = eval_const_expr("$FF", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(255)));
    }

    #[test]
    fn eval_fourcc() {
        // 'ABCD' = 0x41424344 = 1094861636
        let v = eval_const_expr("'ABCD'", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(1094861636)));
    }

    #[test]
    fn eval_octal_literal() {
        // 012 in octal = 10 in decimal
        let v = eval_const_expr("012", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(10)));
    }

    #[test]
    fn eval_octal_literal_larger() {
        // 0777 in octal = 511 in decimal
        let v = eval_const_expr("0777", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(511)));
    }

    #[test]
    fn eval_integer_arithmetic() {
        let v = eval_const_expr("2 * 3 + 1", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(7)));
    }

    #[test]
    fn eval_string_concat() {
        let v = eval_const_expr(r#""hello" + " " + "world""#, &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Str(s) if s == "hello world"));
    }

    #[test]
    fn eval_string_plus_int() {
        let v = eval_const_expr(r#""val=" + 42"#, &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Str(s) if s == "val=42"));
    }

    #[test]
    fn eval_constant_reference() {
        let mut m = HashMap::new();
        m.insert("A".to_string(), ConstValue::Str("foo".to_string()));
        let v = eval_const_expr("A", &m).unwrap();
        assert!(matches!(v, ConstValue::Str(s) if s == "foo"));
    }

    #[test]
    fn eval_constant_concat() {
        let mut m = HashMap::new();
        m.insert("A".to_string(), ConstValue::Str("foo".to_string()));
        m.insert("B".to_string(), ConstValue::Str("bar".to_string()));
        let v = eval_const_expr("A + B", &m).unwrap();
        assert!(matches!(v, ConstValue::Str(s) if s == "foobar"));
    }

    #[test]
    fn eval_i2s() {
        let v = eval_const_expr("I2S(2 * 3)", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Str(s) if s == "6"));
    }

    #[test]
    fn eval_unary_minus() {
        let v = eval_const_expr("-5", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(-5)));
    }

    #[test]
    fn eval_parens() {
        let v = eval_const_expr("(2 + 3) * 4", &empty_map()).unwrap();
        assert!(matches!(v, ConstValue::Int(20)));
    }

    // ─── collect_constants ───────────────────────────────────────────────────

    #[test]
    fn collect_string_constant() {
        let globals = vec![r#"constant string A = "hello""#.to_string()];
        let m = collect_constants(&globals);
        assert!(matches!(m.get("A"), Some(ConstValue::Str(s)) if s == "hello"));
    }

    #[test]
    fn collect_chained_constants() {
        let globals = vec![
            r#"constant string A = "hello""#.to_string(),
            r#"constant string B = A + " world""#.to_string(),
        ];
        let m = collect_constants(&globals);
        assert!(matches!(m.get("B"), Some(ConstValue::Str(s)) if s == "hello world"));
    }

    #[test]
    fn collect_integer_constant() {
        let globals = vec!["constant integer N = 2 * 3".to_string()];
        let m = collect_constants(&globals);
        assert!(matches!(m.get("N"), Some(ConstValue::Int(6))));
    }

    #[test]
    fn collect_skips_non_constant() {
        let globals = vec!["integer x = 0".to_string()];
        let m = collect_constants(&globals);
        assert!(m.is_empty());
    }

    // ─── fold_string_hash ────────────────────────────────────────────────────

    #[test]
    fn fold_simple_literal() {
        let input = r#"set h = StringHash("Hello")"#;
        let expected_hash = blizzard_string_hash("Hello");
        assert_eq!(fold_string_hash(input, &empty_map()), format!("set h = {}", expected_hash));
    }

    #[test]
    fn fold_multiple() {
        let input = r#"set a = StringHash("X") + StringHash("Y")"#;
        let hx = blizzard_string_hash("X");
        let hy = blizzard_string_hash("Y");
        assert_eq!(fold_string_hash(input, &empty_map()), format!("set a = {} + {}", hx, hy));
    }

    #[test]
    fn fold_no_match_variable() {
        let input = "set h = StringHash(myVar)";
        assert_eq!(fold_string_hash(input, &empty_map()), input);
    }

    #[test]
    fn fold_word_boundary() {
        let input = r#"set h = MyStringHash("abc")"#;
        assert_eq!(fold_string_hash(input, &empty_map()), input);
    }

    #[test]
    fn fold_with_escape() {
        let input = r#"set h = StringHash("a\\b")"#;
        let expected_hash = blizzard_string_hash("a\\b");
        assert_eq!(fold_string_hash(input, &empty_map()), format!("set h = {}", expected_hash));
    }

    #[test]
    fn fold_constant_variable() {
        let mut m = HashMap::new();
        m.insert("MY_KEY".to_string(), ConstValue::Str("some".to_string()));
        let input = "set h = StringHash(MY_KEY)";
        let expected = format!("set h = {}", blizzard_string_hash("some"));
        assert_eq!(fold_string_hash(input, &m), expected);
    }

    #[test]
    fn fold_constant_concat() {
        let mut m = HashMap::new();
        m.insert("A".to_string(), ConstValue::Str("hello".to_string()));
        let input = r#"set h = StringHash(A + " world")"#;
        let expected = format!("set h = {}", blizzard_string_hash("hello world"));
        assert_eq!(fold_string_hash(input, &m), expected);
    }

    #[test]
    fn fold_string_plus_int_arithmetic() {
        let mut m = HashMap::new();
        m.insert("A".to_string(), ConstValue::Str("some".to_string()));
        let input = "set h = StringHash(A + 2 * 3)";
        let expected = format!("set h = {}", blizzard_string_hash("some6"));
        assert_eq!(fold_string_hash(input, &m), expected);
    }

    #[test]
    fn fold_i2s_inside() {
        let input = r#"set h = StringHash("n=" + I2S(10))"#;
        let expected = format!("set h = {}", blizzard_string_hash("n=10"));
        assert_eq!(fold_string_hash(input, &empty_map()), expected);
    }

    // ─── find_matching_paren ──────────────────────────────────────────────────

    #[test]
    fn paren_simple() {
        // "foo(bar)" — start=4, closing ')' at 7
        assert_eq!(find_matching_paren("foo(bar)", 4), Some(7));
    }

    #[test]
    fn paren_nested() {
        // "f(g(x))" — start=2, inner pair at 4..5, closing ')' at 6
        assert_eq!(find_matching_paren("f(g(x))", 2), Some(6));
    }

    #[test]
    fn paren_with_string() {
        // String containing ')' should be skipped.
        let s = r#"f("a)b")"#;
        // f( starts at 1, content starts at 2, closing ')' at 8
        assert_eq!(find_matching_paren(s, 2), Some(7));
    }

    #[test]
    fn paren_unmatched() {
        assert_eq!(find_matching_paren("f(bar", 2), None);
    }

    // ─── split_call_args ──────────────────────────────────────────────────────

    #[test]
    fn split_single_arg() {
        // "foo(42)" — args between indices 4..6
        let ranges = split_call_args("foo(42)", 4, 6);
        assert_eq!(ranges, vec![(4, 6)]);
    }

    #[test]
    fn split_two_args() {
        // "foo(1,2)" — args_start=4, args_end=7
        let ranges = split_call_args("foo(1,2)", 4, 7);
        assert_eq!(ranges, vec![(4, 5), (6, 7)]);
    }

    #[test]
    fn split_nested_call() {
        // "foo(g(1,2),3)" — args_start=4, args_end=12
        let ranges = split_call_args("foo(g(1,2),3)", 4, 12);
        assert_eq!(ranges, vec![(4, 10), (11, 12)]);
    }

    #[test]
    fn split_string_with_comma() {
        // r#"foo("a,b",2)"# — comma inside string should not split
        let s = r#"foo("a,b",2)"#;
        let ranges = split_call_args(s, 4, 11);
        assert_eq!(ranges, vec![(4, 9), (10, 11)]);
    }

    #[test]
    fn split_empty_args() {
        // "foo()" — args_start=4, args_end=4, no args
        let ranges = split_call_args("foo()", 4, 4);
        assert!(ranges.is_empty());
    }

    // ─── fold_string_integer_args ─────────────────────────────────────────────

    #[test]
    fn fold_int_arg_simple() {
        let mut sigs = HashMap::new();
        sigs.insert("MyFunc".to_string(), vec!["integer".to_string()]);
        let input = r#"call MyFunc("hello")"#;
        let expected_hash = blizzard_string_hash("hello");
        let result = fold_string_integer_args(input, &empty_map(), &sigs);
        assert_eq!(result, format!("call MyFunc({})", expected_hash));
    }

    #[test]
    fn fold_int_arg_second_param() {
        let mut sigs = HashMap::new();
        sigs.insert("TwoArg".to_string(), vec!["string".to_string(), "integer".to_string()]);
        let input = r#"call TwoArg("keep", "hash_me")"#;
        let expected_hash = blizzard_string_hash("hash_me");
        let result = fold_string_integer_args(input, &empty_map(), &sigs);
        assert_eq!(result, format!(r#"call TwoArg("keep", {})"#, expected_hash));
    }

    #[test]
    fn fold_int_arg_no_match_when_string_param() {
        let mut sigs = HashMap::new();
        sigs.insert("MyFunc".to_string(), vec!["string".to_string()]);
        let input = r#"call MyFunc("hello")"#;
        let result = fold_string_integer_args(input, &empty_map(), &sigs);
        assert_eq!(result, input);
    }

    #[test]
    fn fold_int_arg_unknown_func() {
        let sigs = HashMap::new();
        let input = r#"call Unknown("hello")"#;
        let result = fold_string_integer_args(input, &empty_map(), &sigs);
        assert_eq!(result, input);
    }

    #[test]
    fn fold_int_arg_with_constant() {
        let mut consts = HashMap::new();
        consts.insert("MY_KEY".to_string(), ConstValue::Str("key".to_string()));
        let mut sigs = HashMap::new();
        sigs.insert("Save".to_string(), vec!["integer".to_string(), "integer".to_string()]);
        let input = "call Save(MY_KEY, 42)";
        let expected_hash = blizzard_string_hash("key");
        let result = fold_string_integer_args(input, &consts, &sigs);
        // MY_KEY → hash, 42 stays as-is (it's already an integer)
        assert_eq!(result, format!("call Save({}, 42)", expected_hash));
    }

    #[test]
    fn fold_int_arg_no_change_for_int_literal() {
        let mut sigs = HashMap::new();
        sigs.insert("MyFunc".to_string(), vec!["integer".to_string()]);
        let input = "call MyFunc(42)";
        let result = fold_string_integer_args(input, &empty_map(), &sigs);
        // 42 doesn't evaluate to a string, so no replacement.
        assert_eq!(result, input);
    }
}
