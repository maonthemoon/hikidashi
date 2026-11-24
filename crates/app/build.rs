//! Windows向けのときだけ, アイコン, マニフェスト, バージョン情報(hikidashi.rc)をwindresでexeにつなぐ.
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=hikidashi.rc");
    println!("cargo:rerun-if-changed=hikidashi.manifest");
    println!("cargo:rerun-if-changed=../../assets/hikidashi.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // exeのバージョン情報はCargo.tomlの版に合わせる(hikidashi.rcがincludeする)
    let ver = std::env::var("CARGO_PKG_VERSION").unwrap();
    let num = |k: &str| std::env::var(k).unwrap().parse::<u32>().unwrap_or(0);
    let header = format!(
        "#define VER_MAJOR {}\n#define VER_MINOR {}\n#define VER_PATCH {}\n#define VER_STR \"{}\"\n",
        num("CARGO_PKG_VERSION_MAJOR"),
        num("CARGO_PKG_VERSION_MINOR"),
        num("CARGO_PKG_VERSION_PATCH"),
        ver
    );
    std::fs::write(out_dir.join("version.h"), header).unwrap();
    let out = out_dir.join("hikidashi-res.o");
    let windres = if cfg!(windows) { "windres" } else { "x86_64-w64-mingw32-windres" };
    let status = Command::new(windres)
        .current_dir(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .arg("-I")
        .arg(&out_dir)
        .args(["-O", "coff", "-i", "hikidashi.rc", "-o"])
        .arg(&out)
        .status()
        .expect("windres was not found");
    assert!(status.success(), "windres failed");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}
