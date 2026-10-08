use std::process::Command;
fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    let commit = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "不可用／未嵌入".into());
    println!("cargo:rustc-env=BUILD_COMMIT={commit}");
    let dirty = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "diff",
            "--quiet",
            "HEAD",
            "--",
            "src",
            "assets",
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
        ])
        .status()
        .ok()
        .is_some_and(|s| !s.success());
    println!("cargo:rustc-env=BUILD_DIRTY={dirty}");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/app-icon.ico");
        resource.set("ProductName", "南枫 Codex 额度");
        resource.set("FileDescription", "南枫 Codex 额度 · 只读悬浮窗");
        resource.set("LegalCopyright", "版权所有 © 2026 席瑞");
        resource.set_manifest(r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security></trustInfo><application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness><longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware></windowsSettings></application></assembly>"#);
        resource.compile().expect("compile Windows metadata");
    }
}
