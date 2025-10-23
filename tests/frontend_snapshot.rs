static BUNDLED: &str = include_str!(concat!(env!("OUT_DIR"), "/frontend_index.html"));
static EXPECTED: &str = include_str!("fixtures/frontend_snapshot.html");

#[test]
fn bundled_frontend_matches_fixture() {
    assert_eq!(
        BUNDLED.trim(),
        EXPECTED.trim(),
        "Bundled frontend template diverged from recorded snapshot"
    );
}
