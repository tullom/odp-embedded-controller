use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    if cfg!(feature = "teleprobe-test") {
        File::create(out.join("memory_teleprobe.x"))
            .unwrap()
            .write_all(include_bytes!("memory_teleprobe.x"))
            .unwrap();
        println!("cargo:rerun-if-changed=memory_teleprobe.x");
        println!("cargo:rustc-link-search={}", out.display());
        std::fs::write(out.join("link_ram.x"), include_bytes!("link_ram_cortex_m.x")).unwrap();
        println!("cargo:rerun-if-changed=link_ram_cortex_m.x");
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tlink_ram.x");
        println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
        println!("cargo:rustc-link-arg-bins=-Tteleprobe.x");
    } else {
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(include_bytes!("memory.x"))
            .unwrap();
        println!("cargo:rerun-if-changed=memory.x");
        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rustc-link-arg-bins=--nmagic");
        println!("cargo:rustc-link-arg-bins=-Tlink.x");
        println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
    }
}
