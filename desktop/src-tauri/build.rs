fn main() {
    #[cfg(target_os = "macos")]
    add_swift_compatibility_search_path();
    tauri_build::build()
}

#[cfg(target_os = "macos")]
fn add_swift_compatibility_search_path() {
    use std::path::PathBuf;
    use std::process::Command;

    let Ok(output) = Command::new("xcode-select").arg("-p").output() else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let developer_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let candidates = [
        developer_dir.join("usr/lib/swift/macosx"),
        developer_dir.join("Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"),
    ];
    for path in candidates {
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
}
