use std::fmt::Write;

fn main() {
    // Embed every Material Symbols icon (pre-rendered to 96 px PNG) so Rust can hand
    // Slint icons chosen at runtime.
    let mut table = String::from("pub fn icon_bytes(name: &str) -> Option<&'static [u8]> {\n    match name {\n");
    let mut names: Vec<_> = std::fs::read_dir("ui/icons").unwrap().flatten().map(|e| e.path()).collect();
    names.sort();
    for p in names {
        if p.extension().is_some_and(|e| e == "png") {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let abs = std::fs::canonicalize(&p).unwrap();
            writeln!(table, "        {stem:?} => Some(include_bytes!({:?})),", abs.display().to_string()).unwrap();
        }
    }
    table.push_str("        _ => None,\n    }\n}\n");
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("icons.rs"), table).unwrap();
    println!("cargo:rerun-if-changed=ui/icons");

    let config = slint_build::CompilerConfiguration::new().with_library_paths(std::collections::HashMap::from([(
        "material".to_string(),
        std::path::Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("material-1.1.0/material.slint"),
    )]));
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
}
