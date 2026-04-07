#[cfg(test)]
mod tests {
    use crate::lng::map_editor::slk::*;
    use std::collections::HashMap;

    #[test]
    fn parse_units_slk_fixture() {
        let data = include_bytes!("../../../lng/slk/fixtures/Units/UnitData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("unitID").map(|s| s.as_str()), Some("Hamg"));
        assert_eq!(first.get("comment(s)").map(|s| s.as_str()), Some("HeroArchMage"));
        assert_eq!(first.get("race").map(|s| s.as_str()), Some("human"));
    }

    /// Dump all column names from all 4 unit SLK files.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust map_editor::slk::unit::tests::dump_unit_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_unit_field_names() {
        use std::collections::BTreeSet;

        let files: &[(&str, &[u8])] = &[
            ("UnitData.slk", include_bytes!("../../../lng/slk/fixtures/Units/UnitData.slk")),
            ("UnitBalance.slk", include_bytes!("../../../lng/slk/fixtures/Units/UnitBalance.slk")),
            ("unitUI.slk", include_bytes!("../../../lng/slk/fixtures/Units/unitUI.slk")),
            ("UnitWeapons.slk", include_bytes!("../../../lng/slk/fixtures/Units/UnitWeapons.slk")),
        ];

        let mut all_fields = BTreeSet::new();

        for (name, data) in files {
            let rows = parse_slk(data);
            assert!(!rows.is_empty(), "should parse rows from {}", name);

            let mut fields = BTreeSet::new();
            for row in &rows {
                for key in row.keys() {
                    fields.insert(key.clone());
                }
            }

            println!("\n── {} ({} fields, {} rows) ──", name, fields.len(), rows.len());
            for f in &fields {
                println!("  {}", f);
                all_fields.insert(format!("{}:{}", name, f));
            }
        }

        let out_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/lng/slk/fixtures/Units/unit_field_names.txt"
        );
        let content = all_fields.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(out_path, &content).expect("failed to write unit_field_names.txt");

        println!("\nWrote {} field names to {}", all_fields.len(), out_path);
    }

    /// Dump races, types, and unit names from unit SLK files.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust map_editor::slk::unit::tests::dump_unit_races_and_types -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_unit_races_and_types() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../../lng/slk/fixtures/Units/UnitData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some unit rows");

        let bal_data = include_bytes!("../../../lng/slk/fixtures/Units/UnitBalance.slk");
        let bal_rows = parse_slk(bal_data);
        let mut bal_map: BTreeMap<String, HashMap<String, String>> = BTreeMap::new();
        for row in bal_rows {
            if let Some(id) = row.get("unitBalanceID") {
                bal_map.insert(id.clone(), row);
            }
        }

        let ui_data = include_bytes!("../../../lng/slk/fixtures/Units/unitUI.slk");
        let ui_rows = parse_slk(ui_data);
        let mut ui_map: BTreeMap<String, HashMap<String, String>> = BTreeMap::new();
        for row in ui_rows {
            if let Some(id) = row.get("unitUIID") {
                ui_map.insert(id.clone(), row);
            }
        }

        let mut races: BTreeMap<String, usize> = BTreeMap::new();
        let mut types: BTreeMap<String, usize> = BTreeMap::new();

        for row in &rows {
            let unit_id = row.get("unitID").cloned().unwrap_or_default();
            if unit_id.is_empty() { continue; }

            let race = row.get("race").cloned().unwrap_or_default();
            if !race.is_empty() {
                *races.entry(race).or_insert(0) += 1;
            }

            if let Some(bal) = bal_map.get(&unit_id) {
                let t = bal.get("type").cloned().unwrap_or_default();
                if !t.is_empty() {
                    *types.entry(t).or_insert(0) += 1;
                }
            }
        }

        println!("\n══════════════════════════════════════════");
        println!("  Unit SLK — {} entries", rows.len());
        println!("══════════════════════════════════════════\n");

        println!("── Races ({}) ──", races.len());
        for (r, count) in &races {
            println!("  {:<20} {:>4} units", r, count);
        }

        println!("\n── Types ({}) ──", types.len());
        for (t, count) in &types {
            println!("  {:<20} {:>4} units", t, count);
        }

        println!("\n── Unit names (first 30) ──");
        for row in rows.iter().take(30) {
            let id = row.get("unitID").cloned().unwrap_or_default();
            let name = ui_map.get(&id).and_then(|u| u.get("name")).cloned().unwrap_or_default();
            let race = row.get("race").cloned().unwrap_or_default();
            let comment = row.get("comment(s)").cloned().unwrap_or_default();
            println!("  {} | {:<35} | race={:<12} | {}", id, name, race, comment);
        }

        println!("\nTotal races: {}", races.len());
        println!("Total types: {}", types.len());
        println!("Total unit entries: {}", rows.len());
    }
}

