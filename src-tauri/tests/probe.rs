// Marker integration test: makes `tests` a real cargo target so build.rs
// can attach test-only linker args (the embedded manifest) — see build.rs.
#[test]
fn test_target_exists() {
    assert!(true);
}
