use op_bridge::watcher::parse_watch_spec;

#[test]
fn test_parse_watch_spec_path_and_uri() {
    let entry = parse_watch_spec("/tmp/creds.json=op://vault/item/field").unwrap();
    assert_eq!(entry.path.to_str().unwrap(), "/tmp/creds.json");
    assert_eq!(entry.name, "CREDS_JSON");
    assert_eq!(entry.uri, "op://vault/item/field");
}

#[test]
fn test_parse_watch_spec_with_explicit_name() {
    let entry = parse_watch_spec("/tmp/creds.json=MY_CREDS=op://vault/item/field").unwrap();
    assert_eq!(entry.path.to_str().unwrap(), "/tmp/creds.json");
    assert_eq!(entry.name, "MY_CREDS");
    assert_eq!(entry.uri, "op://vault/item/field");
}

#[test]
fn test_parse_watch_spec_invalid() {
    assert!(parse_watch_spec("just-a-path").is_err());
    assert!(parse_watch_spec("/tmp/file=not-a-uri").is_err());
}
