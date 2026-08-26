//! Architecture check: `ox-core` declares no dependencies (`docs/adr/0001`).

#[test]
fn core_declares_no_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    let (_, after) = manifest
        .split_once("[dependencies]")
        .expect("Cargo.toml has a [dependencies] table");
    let entries: Vec<&str> = after
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(
        entries.is_empty(),
        "ox-core must stay dependency-free, found {entries:?}"
    );
}
