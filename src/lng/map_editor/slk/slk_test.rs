#[cfg(test)]
mod tests {
    use crate::lng::map_editor::slk::*;

    #[test]
    fn parse_terrain_slk_fixture() {
        let data = include_bytes!("../../../lng/slk/fixtures/TerrainArt/Terrain.slk");
        let rows = parse_slk(data);
        // The fixture has 177 data rows (Y2..Y178)
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("tileID").map(|s| s.as_str()), Some("Ldrt"));
        assert_eq!(first.get("comment").map(|s| s.as_str()), Some("Dirt"));
        assert_eq!(
            first.get("dir").map(|s| s.as_str()),
            Some("TerrainArt\\LordaeronSummer")
        );
        assert_eq!(
            first.get("file").map(|s| s.as_str()),
            Some("Lords_Dirt")
        );
    }

    #[test]
    fn parse_cliff_types_slk_fixture() {
        let data = include_bytes!("../../../lng/slk/fixtures/TerrainArt/CliffTypes.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some cliff type rows");

        // First entry: CLdi
        let first = &rows[0];
        assert_eq!(first.get("cliffID").map(|s| s.as_str()), Some("CLdi"));
        assert_eq!(first.get("cliffModelDir").map(|s| s.as_str()), Some("Cliffs"));
        assert_eq!(first.get("rampModelDir").map(|s| s.as_str()), Some("CliffTrans"));
        assert_eq!(first.get("cliffClass").map(|s| s.as_str()), Some("c2"));
        assert_eq!(first.get("groundTile").map(|s| s.as_str()), Some("Ldrt"));

        // CityCliffs entry: CYsq
        let city_cliff = rows.iter().find(|r| r.get("cliffID").map(|s| s.as_str()) == Some("CYsq"));
        assert!(city_cliff.is_some(), "should find CYsq cliff type");
        let cy = city_cliff.unwrap();
        assert_eq!(cy.get("cliffModelDir").map(|s| s.as_str()), Some("CityCliffs"));
        assert_eq!(cy.get("rampModelDir").map(|s| s.as_str()), Some("CityCliffTrans"));
    }

    #[test]
    fn parse_unit_strings_fixture() {
        let data = include_bytes!("../../../lng/bni/fixtures/Units/UndeadUnitStrings.txt");
        let sections = parse_unit_strings(data);
        assert!(!sections.is_empty(), "should parse some sections");

        // Check a hero entry
        let ucrl = sections.get("Ucrl").expect("should have [Ucrl] section");
        assert_eq!(ucrl.get("Name").map(|s| s.as_str()), Some("Crypt Lord"));
        assert_eq!(ucrl.get("Hotkey").map(|s| s.as_str()), Some("C"));
        assert!(ucrl.get("Propernames").is_some());
        assert!(ucrl.get("Ubertip").is_some());
        assert!(ucrl.get("Revivetip").is_some());
        assert!(ucrl.get("Awakentip").is_some());

        // Check a regular unit entry
        let ugho = sections.get("ugho").expect("should have [ugho] section");
        assert_eq!(ugho.get("Name").map(|s| s.as_str()), Some("Ghoul"));
        assert_eq!(ugho.get("Hotkey").map(|s| s.as_str()), Some("G"));

        // Check a building entry
        let unpl = sections.get("unpl").expect("should have [unpl] section");
        assert_eq!(unpl.get("Name").map(|s| s.as_str()), Some("Necropolis"));
        assert_eq!(unpl.get("Hotkey").map(|s| s.as_str()), Some("N"));

        // Check an entry with Casterupgradename
        let uban = sections.get("uban").expect("should have [uban] section");
        assert_eq!(uban.get("Name").map(|s| s.as_str()), Some("Banshee"));
        assert!(uban.get("Casterupgradename").is_some());
        assert!(uban.get("Casterupgradetip").is_some());
    }

    #[test]
    fn parse_human_unit_strings_fixture() {
        let data = include_bytes!("../../../lng/bni/fixtures/Units/HumanUnitStrings.txt");
        let sections = parse_unit_strings(data);
        assert!(!sections.is_empty(), "should parse some sections");

        let hamg = sections.get("Hamg").expect("should have [Hamg] section");
        assert_eq!(hamg.get("Name").map(|s| s.as_str()), Some("Archmage"));
        assert_eq!(hamg.get("Hotkey").map(|s| s.as_str()), Some("A"));
        assert!(hamg.get("Propernames").is_some());
    }
}
