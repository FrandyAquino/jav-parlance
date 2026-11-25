//! Download verification against a local HTTP server.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test helpers may panic

use std::sync::{Arc, Mutex};

use parlance_models::{CatalogEntry, DownloadError, ModelStore, download};
use parlance_protocol::ModelKind;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Serves `body` to every request on an ephemeral port.
async fn serve(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                let mut request = [0u8; 1024];
                let _ = socket.read(&mut request).await;
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            });
        }
    });
    format!("http://{address}/model.bin")
}

fn entry(url: String, sha256: String, size: u64) -> CatalogEntry {
    CatalogEntry {
        id: "test-model",
        kind: ModelKind::Speech,
        name: "Test",
        description: "",
        license: "MIT",
        file_name: "model.bin",
        url: Box::leak(url.into_boxed_str()),
        sha256: Box::leak(sha256.into_boxed_str()),
        size_bytes: size,
        is_default: false,
        archive: None,
    }
}

fn body() -> Vec<u8> {
    (0..3 * 1024 * 1024).map(|i| (i % 251) as u8).collect()
}

#[tokio::test]
async fn installs_file_when_checksum_matches() {
    let body = body();
    let digest = hex::encode(Sha256::digest(&body));
    let entry = entry(serve(body.clone()).await, digest, body.len() as u64);
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::new(dir.path());
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);

    let client = reqwest::Client::new();
    let path = download(&client, &store, &entry, CancellationToken::new(), move |done, total| {
        sink.lock().unwrap().push((done, total))
    })
    .await
    .unwrap();

    assert_eq!(std::fs::read(path).unwrap(), body);
    assert!(store.is_installed(&entry));
    assert_eq!(reports.lock().unwrap().last(), Some(&(body.len() as u64, body.len() as u64)));
}

#[tokio::test]
async fn discards_file_when_checksum_differs() {
    let body = body();
    let entry = entry(serve(body.clone()).await, "0".repeat(64), body.len() as u64);
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::new(dir.path());

    let error =
        download(&reqwest::Client::new(), &store, &entry, CancellationToken::new(), |_, _| {}).await.unwrap_err();

    assert!(matches!(error, DownloadError::ChecksumMismatch { .. }));
    assert!(!store.is_installed(&entry));
    assert!(!store.path(&entry).exists());
}

#[tokio::test]
async fn rejects_unexpected_size_without_hashing_everything() {
    let body = body();
    let entry = entry(serve(body).await, "0".repeat(64), 1024);
    let dir = tempfile::tempdir().unwrap();
    let error =
        download(&reqwest::Client::new(), &ModelStore::new(dir.path()), &entry, CancellationToken::new(), |_, _| {})
            .await
            .unwrap_err();
    assert!(matches!(error, DownloadError::SizeMismatch { .. }));
}

#[tokio::test]
async fn honours_cancellation() {
    let body = body();
    let entry = entry(serve(body.clone()).await, "0".repeat(64), body.len() as u64);
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error =
        download(&reqwest::Client::new(), &ModelStore::new(dir.path()), &entry, cancel, |_, _| {}).await.unwrap_err();
    assert!(matches!(error, DownloadError::Cancelled));
}
