use std::env;
use std::fmt::Write;
use std::fs;
use std::path::Path;

const LANGUAGES: [(&str, &str); 2] = [("en", "EN"), ("pt-BR", "PT_BR")];

fn main() {
    println!("cargo:rerun-if-changed=../../locales");
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let locales = Path::new(&manifest).join("../../locales");
    let mut generated = String::new();
    for (dir, name) in LANGUAGES {
        let mut files: Vec<_> = fs::read_dir(locales.join(dir))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "ftl"))
            .collect();
        files.sort_by_key(|path| path.file_name().map(ToOwned::to_owned));
        writeln!(generated, "pub const {name}: &[(&str, &str)] = &[").unwrap();
        for path in files {
            let file = path.file_name().unwrap().to_str().unwrap();
            let canonical = path.canonicalize().unwrap();
            writeln!(
                generated,
                "    ({file:?}, include_str!({:?})),",
                canonical.display().to_string()
            )
            .unwrap();
        }
        generated.push_str("];\n");
    }
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("catalog.rs");
    fs::write(out, generated).unwrap();
}
