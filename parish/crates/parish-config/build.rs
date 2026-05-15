use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let providers_dir = manifest_dir.join("providers");

    println!("cargo:rerun-if-changed=providers");

    let mut files: Vec<_> = fs::read_dir(&providers_dir)
        .expect("providers/ directory must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .map(|e| e.path())
        .collect();
    files.sort();

    let mut entries = String::new();
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        // Use raw string with enough # to safely embed TOML content
        entries.push_str(&format!("    r####\"{}\"####,\n", content));
    }

    let out = format!("static RAW_PROVIDER_MODS: &[&str] = &[\n{}];\n", entries);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("providers_gen.rs"), out).unwrap();
}
