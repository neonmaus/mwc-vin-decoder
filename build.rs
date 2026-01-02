use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Only run when building for a Windows target (including cross-compiles).
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let icon_src = Path::new("assets/icon.ico");
    if !icon_src.exists() {
        println!("cargo:warning=icon.ico not found, skipping icon embed");
        return;
    }

    // If building on a Windows host natively, prefer the `winres` crate.
    let host = env::var("HOST").unwrap_or_default();
    if host.contains("windows") {
        match (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.compile()?;
            Ok(())
        })() {
            Ok(()) => println!("cargo:warning=winres succeeded (icon embedded)"),
            Err(e) => println!("cargo:warning=winres failed: {}", e),
        }
        return;
    }

    // Use OUT_DIR to avoid spaces and ensure writable location.
    let out_dir =
        env::var("OUT_DIR").unwrap_or_else(|_| env::temp_dir().to_string_lossy().into_owned());
    let rc_path = Path::new(&out_dir).join("mwc_icon.rc");
    let res_path = Path::new(&out_dir).join("mwc_icon.res");
    let icon_tmp = Path::new(&out_dir).join("mwc_icon.ico");

    if let Err(e) = fs::copy(icon_src, &icon_tmp) {
        println!("cargo:warning=Failed to copy icon.ico to OUT_DIR: {}", e);
        return;
    }

    // Write a minimal .rc referencing the copied ico.
    let rc_contents = format!("1 ICON \"{}\"\n", icon_tmp.to_string_lossy());
    if let Err(e) = fs::write(&rc_path, rc_contents) {
        println!("cargo:warning=Failed to write {}: {}", rc_path.display(), e);
        return;
    }

    // Locate windres: prefer WINDRES env, otherwise common names on PATH.
    let windres_exec = env::var("WINDRES").ok().or_else(|| {
        let candidates = ["x86_64-w64-mingw32-windres", "windres"];
        env::var("PATH").ok().and_then(|p| {
            for dir in env::split_paths(&p) {
                for cand in &candidates {
                    let pth = dir.join(cand);
                    if pth.exists() {
                        return Some(pth.to_string_lossy().into_owned());
                    }
                }
            }
            None
        })
    });

    let windres_exec = match windres_exec {
        Some(p) => p,
        None => {
            println!("cargo:warning=windres not found; cannot embed icon");
            return;
        }
    };

    println!(
        "cargo:warning=Invoking windres: {} -i {} -O coff -o {}",
        windres_exec,
        rc_path.display(),
        res_path.display()
    );
    match Command::new(&windres_exec)
        .arg("-i")
        .arg(rc_path.as_os_str())
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(res_path.as_os_str())
        .status()
    {
        Ok(s) if s.success() => {
            println!(
                "cargo:warning=windres succeeded, .res at {}",
                res_path.display()
            );
            if let Some(res_str) = res_path.to_str() {
                println!("cargo:rustc-link-arg-bins={}", res_str);
            }
        }
        Ok(s) => println!("cargo:warning=windres exited with status: {}", s),
        Err(e) => println!("cargo:warning=Failed to spawn windres: {}", e),
    }
}
