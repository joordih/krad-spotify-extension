use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const VENDOR: &str = "vendor/mediaremote-adapter";
const SCRIPT: &str = "bin/mediaremote-adapter.pl";
const DYLIB_NAME: &str = "MediaRemoteAdapter";
const DYLIB_ENV: &str = "MEDIAREMOTE_ADAPTER_DYLIB";
const KEY_ENV: &str = "MEDIAREMOTE_ADAPTER_KEY";
const MACOS: &str = "macos";
const ARM64: &str = "arm64";
const X86_64: &str = "x86_64";
const AARCH64: &str = "aarch64";
const ARCH_FLAG: &str = "-arch";
const TARGET_FLAG: &str = "--target";
const MIN_MACOS: &str = "-mmacosx-version-min=11.0";
const FRAMEWORKS: &[&str] = &["Foundation", "AppKit", "UniformTypeIdentifiers"];
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SOURCES: &[&str] = &[
    "src/adapter/env.m",
    "src/adapter/get.m",
    "src/adapter/globals.m",
    "src/adapter/keys.m",
    "src/adapter/now_playing.m",
    "src/adapter/repeat.m",
    "src/adapter/seek.m",
    "src/adapter/send.m",
    "src/adapter/shuffle.m",
    "src/adapter/stream.m",
    "src/private/MediaRemote.m",
    "src/utility/Debounce.m",
    "src/utility/helpers.m",
];

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok(MACOS) {
        return Ok(());
    }
    println!("cargo:rerun-if-changed={VENDOR}");
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("cargo did not set OUT_DIR")?);
    let dylib = out.join(DYLIB_NAME);
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join(VENDOR);

    let (target_arch, other_arch) = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok(AARCH64) => (ARM64, X86_64),
        _ => (X86_64, ARM64),
    };
    if !link(&vendor, &dylib, &[target_arch, other_arch]) && !link(&vendor, &dylib, &[target_arch])
    {
        return Err("could not build the MediaRemote adapter dylib".into());
    }
    sign(&dylib);

    let key = fnv1a(&[&fs::read(vendor.join(SCRIPT))?, &fs::read(&dylib)?]);
    println!("cargo:rustc-env={DYLIB_ENV}={}", dylib.display());
    println!("cargo:rustc-env={KEY_ENV}={key:016x}");
    Ok(())
}

fn sign(dylib: &Path) {
    match Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(dylib)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => println!("cargo:warning=codesign failed with {status}"),
        Err(error) => println!("cargo:warning=codesign not run: {error}"),
    }
}

fn fnv1a(parts: &[&[u8]]) -> u64 {
    parts
        .iter()
        .flat_map(|part| part.iter())
        .fold(FNV_OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        })
}

fn link(vendor: &Path, dylib: &Path, archs: &[&str]) -> bool {
    let compiler = cc::Build::new()
        .cargo_metadata(false)
        .cargo_warnings(false)
        .get_compiler();
    let mut command = Command::new(compiler.path());
    let mut skip_next = false;
    for arg in compiler.args() {
        if skip_next {
            skip_next = false;
            continue;
        }
        let text = arg.to_string_lossy();
        if text == ARCH_FLAG {
            skip_next = true;
        } else if !text.starts_with(TARGET_FLAG) {
            command.arg(arg);
        }
    }
    for arch in archs {
        command.args([ARCH_FLAG, arch]);
    }
    command
        .args([
            "-dynamiclib",
            "-fobjc-arc",
            "-fvisibility=default",
            MIN_MACOS,
            "-w",
        ])
        .arg("-I")
        .arg(vendor.join("include"))
        .arg("-I")
        .arg(vendor.join("src"))
        .args(SOURCES.iter().map(|source| vendor.join(source)));
    for framework in FRAMEWORKS {
        command.args(["-framework", framework]);
    }
    command.arg("-o").arg(dylib);
    match command.status() {
        Ok(status) => status.success(),
        Err(error) => {
            println!(
                "cargo:warning=could not run {}: {error}",
                compiler.path().display()
            );
            false
        }
    }
}
