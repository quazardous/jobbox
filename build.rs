//! WHY A BUILD SCRIPT EXISTS FOR A PROGRAM THIS SMALL.
//!
//! A Windows executable carries a `VERSIONINFO` resource — the block a
//! person sees under *Properties → Details*, and the block a signing
//! service reads to check that the binary claims to be what the
//! application said it was. **Rust does not emit one.** Without it,
//! `jbx.exe` is a nameless, versionless file that happens to be called
//! jbx, which is exactly what a code-signing review refuses.
//!
//! So this file exists to say, inside the binary, what the repository
//! already says outside it.
//!
//! IT IS DELIBERATELY SILENT EVERYWHERE ELSE. Linux and macOS have no
//! such resource, and a Linux box cross-compiling to Windows has no
//! resource compiler — `Cargo.toml` keeps the dependency to Windows
//! hosts, and the `cfg` below keeps the code to them too. A build that
//! cannot write the metadata WARNS rather than fails: breaking someone's
//! `cargo build` over a field they did not ask for would be rude.
//!
//! THE RELEASE IS WHERE IT IS NOT OPTIONAL, and the workflow checks the
//! built `.exe` really carries it. A warning nobody reads is the same as
//! no metadata at all, so the guard lives where it can stop a release
//! rather than where it can only mention one.

fn main() {
    // Re-run when the version moves, not on every file touched.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    #[cfg(windows)]
    windows_version_block();
}

#[cfg(windows)]
fn windows_version_block() {
    // FileVersion and ProductVersion come from CARGO_PKG_VERSION on their
    // own, so the one number that must not drift is not restated here.
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "JobBox")
        .set("InternalName", "jbx")
        .set("OriginalFilename", "jbx.exe")
        .set(
            "FileDescription",
            "jbx — run a line, and detach it if it turns out to be long",
        )
        // COMPANYNAME SHOULD MATCH THE CERTIFICATE SUBJECT once there is
        // a certificate. Today there is none, so this names the account
        // that publishes the releases rather than inventing a legal
        // entity — see CODE-SIGNING-POLICY.md.
        .set("CompanyName", "quazardous")
        .set("LegalCopyright", "MIT licensed — see LICENSE");
    if let Err(e) = res.compile() {
        println!("cargo:warning=no Windows version metadata: {e}");
        println!("cargo:warning=a release built like this will fail its metadata check");
    }
}
