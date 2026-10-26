use cc::Build;
use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/wasm.c");
    if env::var("TARGET").as_deref() == Ok("wasm32-unknown-unknown") {
        Build::new()
            .file("src/wasm.c")
            .compile("clankerdiff_syntax_wasm");
    }
}
