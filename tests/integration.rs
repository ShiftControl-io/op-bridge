use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;

use op_bridge::store::SecretStore;

/// Test the real socket handler with a real SecretStore containing a test secret.
#[tokio::test]
async fn test_socket_protocol_with_real_handler() {
    let socket_path = PathBuf::from(format!("/tmp/op-bridge-test-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();

    // Build a store with a test secret
    let store = Arc::new(RwLock::new(SecretStore::new()));
    {
        let mut s = store.write().await;
        s.insert(
            "MY_SECRET".to_string(),
            secrecy::SecretString::from("hunter2".to_string()),
        );
    }

    let handle = tokio::spawn({
        let store = Arc::clone(&store);
        let socket_path = socket_path.clone();
        async move {
            let (stream, _) = listener.accept().await.unwrap();
            op_bridge::socket::handle_client(stream, &store)
                .await
                .unwrap();
            let _ = std::fs::remove_file(&socket_path);
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // PING
    writer.write_all(b"PING\n").await.unwrap();
    let resp = lines.next_line().await.unwrap().unwrap();
    assert_eq!(resp, "OK pong");

    // GET existing secret
    writer.write_all(b"GET MY_SECRET\n").await.unwrap();
    let resp = lines.next_line().await.unwrap().unwrap();
    assert_eq!(resp, "OK hunter2");

    // GET missing secret
    writer.write_all(b"GET MISSING\n").await.unwrap();
    let resp = lines.next_line().await.unwrap().unwrap();
    assert_eq!(resp, "ERR unknown ref: MISSING");

    // Case-insensitive GET
    writer.write_all(b"get MY_SECRET\n").await.unwrap();
    let resp = lines.next_line().await.unwrap().unwrap();
    assert_eq!(resp, "OK hunter2");

    // LIST
    writer.write_all(b"LIST\n").await.unwrap();
    let resp = lines.next_line().await.unwrap().unwrap();
    assert_eq!(resp, "OK MY_SECRET");

    // Unknown command
    writer.write_all(b"BADCMD\n").await.unwrap();
    let resp = lines.next_line().await.unwrap().unwrap();
    assert!(resp.starts_with("ERR unknown command"));

    drop(writer);
    let _ = handle.await;
}

/// Test that discover_refs correctly parses OP_*_REF env vars.
#[test]
fn test_discover_refs_with_env() {
    std::env::set_var("OP_TEST_SECRET_REF", "op://vault/item/field");
    std::env::set_var("OP_ANOTHER_REF", "op://vault/item2/field2");
    std::env::set_var("OP_NO_SUFFIX", "op://vault/item3/field3");
    std::env::set_var("NOT_OP_PREFIX_REF", "op://vault/item4/field4");

    let refs = op_bridge::resolver::discover_refs("OP_", "_REF");

    assert!(
        refs.iter().any(|r| r.name == "TEST_SECRET"),
        "should find TEST_SECRET"
    );
    assert!(
        refs.iter().any(|r| r.name == "ANOTHER"),
        "should find ANOTHER"
    );
    assert!(
        !refs.iter().any(|r| r.name == "NO_SUFFIX"),
        "should not find NO_SUFFIX (missing _REF suffix)"
    );
}
