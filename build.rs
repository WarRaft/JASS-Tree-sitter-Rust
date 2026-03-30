/// Build script — extracts the extension version from `package.json`
/// and makes it available at compile time as `env!("EXT_VERSION")`.
///
/// This is used by `cache_db` to detect binary version changes and
/// auto-purge stale caches so the user doesn't have to rescan manually.
fn main() {
    println!("cargo:rerun-if-changed=package.json");

    let version = std::fs::read_to_string("package.json")
        .ok()
        .and_then(|text| {
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            v.get("version")?.as_str().map(String::from)
        })
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=EXT_VERSION={}", version);
}

