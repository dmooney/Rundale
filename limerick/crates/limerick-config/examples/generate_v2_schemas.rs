use std::path::Path;

use limerick_config::{generated_project_schema_v2, generated_user_schema_v2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let configured = std::env::var_os("LIMERICK_SCHEMA_OUTPUT").map(std::path::PathBuf::from);
    let default = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/schemas");
    let output = configured.as_deref().unwrap_or(&default);
    std::fs::create_dir_all(output)?;
    write_schema(
        &output.join("limerick-project-config-v2.schema.json"),
        generated_project_schema_v2(),
    )?;
    write_schema(
        &output.join("limerick-user-config-v2.schema.json"),
        generated_user_schema_v2(),
    )?;
    Ok(())
}

fn write_schema(path: &Path, value: serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&value)?))?;
    Ok(())
}
