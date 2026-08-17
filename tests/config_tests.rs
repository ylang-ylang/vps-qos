use vps_bandwidth_observer::config::RuntimeConfig;

#[test]
fn checked_in_json_exactly_matches_rust_defaults() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/default.json");
    let checked_in = RuntimeConfig::load(path).unwrap();
    assert_eq!(checked_in, RuntimeConfig::default());
}

#[test]
fn optional_groups_default_when_required_input_is_present() {
    let parsed: RuntimeConfig =
        serde_json::from_str(r#"{"required":{"nominal_ceiling_bps":200000000.0}}"#).unwrap();
    assert_eq!(parsed, RuntimeConfig::default());
}

#[test]
fn nominal_ceiling_is_required() {
    let error = serde_json::from_str::<RuntimeConfig>("{}").unwrap_err();
    assert!(error.to_string().contains("required"));
}

#[test]
fn unknown_factor_name_is_rejected() {
    let error = serde_json::from_str::<RuntimeConfig>(
        r#"{"required":{"nominal_ceiling_bps":200000000.0},"factors":{"retransmision":{"enabled":true,"min_delta":1}}}"#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown field"));
}
