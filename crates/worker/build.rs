//! Gives the worker an 8 MiB wasm stack (the default is 1 MiB), like a native
//! main thread: Scryer recurses on the Rust stack when it reads and compiles
//! deeply nested terms, such as long `a => b => c => ...` chains.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo::rustc-link-arg-bins=-zstack-size=8388608");
    }
}
