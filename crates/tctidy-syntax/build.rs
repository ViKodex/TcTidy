#[path = "keyword_codegen.rs"]
mod keyword_codegen;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=keywords.csv");
    println!("cargo:rerun-if-changed=keyword_codegen.rs");
    println!("cargo:rerun-if-changed=src/kind.rs");

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let csv = fs::read_to_string(manifest.join("keywords.csv")).expect("read keywords.csv");
    let kinds = fs::read_to_string(manifest.join("src/kind.rs")).expect("read SyntaxKind");
    let generated = keyword_codegen::generate(&csv, &kinds)
        .unwrap_or_else(|error| panic!("invalid keyword inventory: {error}"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"))
        .join("keywords_generated.rs");
    fs::write(output, generated).expect("write generated keyword table");
}
