//! Replica acceptance for the cloud worker. The test itself is in `linux.rs`: it needs
//! Linux root and the gated sandbox, and uses Unix-only APIs, so it compiles only on
//! Linux. Elsewhere this builds a stub, which keeps `cargo clippy --all-targets` working
//! on macOS and Windows.

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    linux::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("replica acceptance requires Linux");
    std::process::exit(1);
}
