#[cfg(test)]
mod tests {
    use crate::lng::map_editor::slk::*;

    #[test]
    fn parse_cliff_variations_embedded() {
        let result = load_cliff_variations();

        // Cliffs.slk: 64 entries
        assert!(!result.cliffs.is_empty(), "cliffs should not be empty");
        assert_eq!(result.cliffs.get("AAAB"), Some(&1));
        assert_eq!(result.cliffs.get("AABB"), Some(&2));
        assert_eq!(result.cliffs.get("AABC"), Some(&0));
        assert_eq!(result.cliffs.get("CCCA"), Some(&1));

        // CityCliffs.slk: 64 entries
        assert!(!result.city_cliffs.is_empty(), "city_cliffs should not be empty");
        assert_eq!(result.city_cliffs.get("AAAB"), Some(&2));
        assert_eq!(result.city_cliffs.get("AABB"), Some(&3));
        assert_eq!(result.city_cliffs.get("AABC"), Some(&0));
        assert_eq!(result.city_cliffs.get("CCCA"), Some(&1));
    }
}

