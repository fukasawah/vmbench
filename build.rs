use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let out = Command::new(&rustc).arg("-vV").output();
    let (rustc_version, llvm_version, host) = match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            let mut ver = String::new();
            let mut llvm = String::new();
            let mut host = String::new();
            for line in s.lines() {
                if let Some(v) = line.strip_prefix("rustc ") {
                    ver = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("LLVM version: ") {
                    llvm = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("host: ") {
                    host = v.trim().to_string();
                }
            }
            (ver, llvm, host)
        }
        _ => ("unavailable".to_string(), "unavailable".to_string(), "unavailable".to_string()),
    };

    let revision = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unavailable".to_string());

    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let opt = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "unknown".to_string());
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=VMBENCH_RUSTC_VERSION={rustc_version}");
    println!("cargo:rustc-env=VMBENCH_LLVM_VERSION={llvm_version}");
    println!("cargo:rustc-env=VMBENCH_BUILD_HOST={host}");
    println!("cargo:rustc-env=VMBENCH_SOURCE_REVISION={revision}");
    println!("cargo:rustc-env=VMBENCH_BUILD_TARGET={target}");
    println!("cargo:rustc-env=VMBENCH_BUILD_OPT_LEVEL={opt}");
    println!("cargo:rustc-env=VMBENCH_BUILD_PROFILE={profile}");
}
