use faber_engine::project::report::UnknownOr;

#[test]
fn unknown_string_report_metric_round_trips_as_unknown() {
    let metric: UnknownOr<String> = UnknownOr::default();
    let json = serde_json::to_string(&metric).unwrap();
    assert_eq!(json, "\"unknown\"");
    assert_eq!(serde_json::from_str::<UnknownOr<String>>(&json).unwrap(), metric);
}

#[test]
fn known_report_metrics_keep_the_existing_scalar_format() {
    let text = UnknownOr::Known("measured-head".to_string());
    let number = UnknownOr::Known(42_u64);
    assert_eq!(serde_json::to_string(&text).unwrap(), "\"measured-head\"");
    assert_eq!(serde_json::from_str::<UnknownOr<String>>("\"measured-head\"").unwrap(), text);
    assert_eq!(serde_json::from_str::<UnknownOr<u64>>("42").unwrap(), number);
    assert_eq!(serde_json::from_str::<UnknownOr<u64>>("\"unknown\"").unwrap(), UnknownOr::default());
}
