use vps_bandwidth_observer::config::RuntimeConfig;

#[test]
fn checked_in_json_exactly_matches_rust_defaults() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/default.json");
    let checked_in = RuntimeConfig::load(path).unwrap();
    assert_eq!(checked_in, RuntimeConfig::default());
}

#[test]
fn serde_defaults_fill_omitted_fields() {
    let parsed: RuntimeConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed, RuntimeConfig::default());
}
