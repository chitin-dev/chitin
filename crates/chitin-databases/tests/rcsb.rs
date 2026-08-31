//! Integration coverage for the RCSB PDB database provider.

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use chitin_databases::{
  ArtifactFormat, CancellationToken, Client, ClientConfig, DownloadedArtifact, ProviderId, RetryPolicy, TransportError,
  providers::rcsb::{PdbId, PdbIdError, RcsbEndpoints, RcsbError, StructureFormat},
  test_support::{MockTransport, response, response_with_headers},
};
use http::{
  HeaderMap, HeaderValue, StatusCode,
  header::{CONTENT_LENGTH, ETAG, LAST_MODIFIED},
};

fn test_client(transport: MockTransport) -> Client {
  let config = ClientConfig {
    retry_policy: RetryPolicy {
      max_attempts: 3,
      initial_backoff: Duration::ZERO,
      max_backoff: Duration::ZERO,
    },
    ..ClientConfig::default()
  };
  Client::with_transport(config, Arc::new(transport))
}

fn pdb_id(value: &str) -> PdbId {
  PdbId::new(value).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn pdb_id_new_should_normalize_lowercase_identifier() {
  let id = PdbId::new("4hhb");

  assert_eq!(id.map(|id| id.to_string()), Ok("4HHB".to_string()));
}

#[test]
fn pdb_id_new_should_reject_invalid_length() {
  let id = PdbId::new("4HH");

  assert_eq!(id, Err(PdbIdError::InvalidLength { actual: 3 }));
}

#[test]
fn pdb_id_new_should_reject_non_alphanumeric_character() {
  let id = PdbId::new("4H_B");

  assert_eq!(id, Err(PdbIdError::InvalidCharacter { character: '_' }));
}

#[test]
fn rcsb_structure_download_request_should_target_structure_file() {
  let endpoints = RcsbEndpoints::new();
  let request = endpoints.structure_download_request(&pdb_id("4hhb"), StructureFormat::Mmcif);

  assert_eq!(
    request.map(|request| request.url.to_string()),
    Ok("https://files.rcsb.org/download/4HHB.cif".to_string())
  );
}

#[test]
fn rcsb_metadata_request_should_target_data_api_entry() {
  let endpoints = RcsbEndpoints::new();
  let request = endpoints.entry_metadata_request(&pdb_id("4hhb"));

  assert_eq!(
    request.map(|request| request.url.to_string()),
    Ok("https://data.rcsb.org/rest/v1/core/entry/4HHB".to_string())
  );
}

#[tokio::test]
async fn download_structure_should_return_artifact_from_mock_transport() {
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, b"data_4HHB\n#"))]);
  let client = test_client(transport);

  let artifact = client
    .rcsb()
    .download_structure(pdb_id("4hhb"), StructureFormat::Pdb)
    .await;

  assert_eq!(
    artifact.map(|artifact| (artifact.format, artifact.content)),
    Ok((ArtifactFormat::Pdb, Bytes::from_static(b"data_4HHB\n#")))
  );
}

#[tokio::test]
async fn entry_metadata_should_decode_stable_metadata_subset() {
  let body = br#"{
    "rcsb_id": "4HHB",
    "struct": { "title": "HEMOGLOBIN" },
    "exptl": [{ "method": "X-RAY DIFFRACTION" }],
    "rcsb_accession_info": { "initial_release_date": "1984-03-07T00:00:00Z" }
  }"#;
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, body))]);
  let client = test_client(transport);

  let metadata = client.rcsb().entry_metadata(pdb_id("4hhb")).await;

  assert_eq!(
    metadata.map(|metadata| {
      (
        metadata.id.to_string(),
        metadata.title,
        metadata.experimental_methods,
        metadata.initial_release_date,
      )
    }),
    Ok((
      "4HHB".to_string(),
      Some("HEMOGLOBIN".to_string()),
      vec!["X-RAY DIFFRACTION".to_string()],
      Some("1984-03-07T00:00:00Z".to_string())
    ))
  );
}

#[tokio::test]
async fn entry_metadata_should_map_404_to_entry_not_found() {
  let transport = MockTransport::new(vec![Ok(response(StatusCode::NOT_FOUND, b""))]);
  let client = test_client(transport);

  let error = client.rcsb().entry_metadata(pdb_id("4hhb")).await;

  assert_eq!(error, Err(RcsbError::EntryNotFound { id: "4HHB".to_string() }));
}

#[tokio::test]
async fn entry_metadata_should_map_429_to_rate_limited_after_retries() {
  let transport = MockTransport::new(vec![
    Ok(response(StatusCode::TOO_MANY_REQUESTS, b"")),
    Ok(response(StatusCode::TOO_MANY_REQUESTS, b"")),
    Ok(response(StatusCode::TOO_MANY_REQUESTS, b"")),
  ]);
  let client = test_client(transport);

  let error = client.rcsb().entry_metadata(pdb_id("4hhb")).await;

  assert_eq!(error, Err(RcsbError::RateLimited));
}

#[tokio::test]
async fn download_structure_should_retry_transient_failure_then_succeed() {
  let transport = MockTransport::new(vec![
    Err(TransportError::Connection),
    Ok(response(StatusCode::OK, b"retried")),
  ]);
  let probe = transport.clone();
  let client = test_client(transport);

  let artifact = client
    .rcsb()
    .download_structure(pdb_id("4hhb"), StructureFormat::Pdb)
    .await;

  assert_eq!(
    artifact.map(|artifact: DownloadedArtifact| artifact.content),
    Ok(Bytes::from_static(b"retried"))
  );
  assert_eq!(probe.request_count(), 2);
}

#[tokio::test]
async fn entry_metadata_should_not_retry_404() {
  let transport = MockTransport::new(vec![
    Ok(response(StatusCode::NOT_FOUND, b"")),
    Ok(response(StatusCode::OK, b"{}")),
  ]);
  let probe = transport.clone();
  let client = test_client(transport);

  let _ = client.rcsb().entry_metadata(pdb_id("4hhb")).await;

  assert_eq!(probe.request_count(), 1);
}

#[tokio::test]
async fn download_structure_should_enforce_response_size_limit() {
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, b"12345"))]);
  let config = ClientConfig {
    max_response_bytes: 4,
    retry_policy: RetryPolicy {
      max_attempts: 1,
      initial_backoff: Duration::ZERO,
      max_backoff: Duration::ZERO,
    },
    ..ClientConfig::default()
  };
  let client = Client::with_transport(config, Arc::new(transport));

  let error = client
    .rcsb()
    .download_structure(pdb_id("4hhb"), StructureFormat::Pdb)
    .await;

  assert_eq!(
    error,
    Err(RcsbError::Transport(TransportError::ResponseTooLarge {
      limit: 4,
      observed: Some(5),
    }))
  );
}

#[tokio::test]
async fn download_structure_should_extract_provenance_headers() {
  let mut headers = HeaderMap::new();
  headers.insert(ETAG, HeaderValue::from_static("\"abc\""));
  headers.insert(LAST_MODIFIED, HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"));
  headers.insert(CONTENT_LENGTH, HeaderValue::from_static("4"));
  let transport = MockTransport::new(vec![Ok(response_with_headers(StatusCode::OK, b"1234", headers))]);
  let client = test_client(transport);

  let artifact = client
    .rcsb()
    .download_structure(pdb_id("4hhb"), StructureFormat::Pdb)
    .await;

  assert_eq!(
    artifact.map(|artifact| {
      (
        artifact.provenance.provider,
        artifact.provenance.resource_identifier,
        artifact.provenance.etag,
        artifact.provenance.last_modified,
        artifact.provenance.content_length,
      )
    }),
    Ok((
      ProviderId::Rcsb,
      "4HHB".to_string(),
      Some("\"abc\"".to_string()),
      Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
      Some(4)
    ))
  );
}

#[tokio::test]
async fn client_should_share_concurrency_limit_across_provider_requests() {
  let transport = MockTransport::new(vec![
    Ok(response(StatusCode::OK, b"first")),
    Ok(response(StatusCode::OK, b"second")),
  ])
  .with_delay(Duration::from_millis(25));
  let probe = transport.clone();
  let config = ClientConfig {
    max_concurrent_requests: 1,
    retry_policy: RetryPolicy {
      max_attempts: 1,
      initial_backoff: Duration::ZERO,
      max_backoff: Duration::ZERO,
    },
    ..ClientConfig::default()
  };
  let rcsb = Client::with_transport(config, Arc::new(transport)).rcsb();
  let first = rcsb.download_structure(pdb_id("4hhb"), StructureFormat::Pdb);
  let second = rcsb.download_structure(pdb_id("1cbs"), StructureFormat::Pdb);

  let _ = tokio::join!(first, second);

  assert_eq!(probe.max_active(), 1);
}

#[tokio::test]
async fn download_structure_should_send_expected_url_to_transport() {
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, b""))]);
  let probe = transport.clone();
  let client = test_client(transport);

  let _ = client
    .rcsb()
    .download_structure(pdb_id("4hhb"), StructureFormat::Mmcif)
    .await;

  assert_eq!(
    probe.first_request_url(),
    Some("https://files.rcsb.org/download/4HHB.cif".to_string())
  );
}

#[tokio::test]
async fn download_structure_to_path_should_return_persisted_metadata_and_checksum() {
  let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
  let destination = directory.path().join("4HHB.pdb");
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, b"streamed"))]);
  let client = test_client(transport);

  let artifact = client
    .rcsb()
    .download_structure_to_path(pdb_id("4hhb"), StructureFormat::Pdb, &destination, |_, _| {})
    .await
    .unwrap_or_else(|error| panic!("artifact should be persisted: {error}"));

  assert_eq!(artifact.path, destination);
  assert_eq!(artifact.bytes, 8);
  assert_eq!(
    artifact.checksum,
    "97a78c00831554f7cc9745e8f6732edcfb571cf548a8d12b48a6e3fc31e5e3e6"
  );
  assert_eq!(std::fs::read(&artifact.path).unwrap_or_default(), b"streamed");
}

#[tokio::test]
async fn download_structure_to_path_should_replace_existing_destination() {
  let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
  let destination = directory.path().join("4HHB.pdb");
  tokio::fs::write(&destination, b"old content")
    .await
    .unwrap_or_else(|error| panic!("existing destination should be created: {error}"));
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, b"new content"))]);
  let client = test_client(transport);

  client
    .rcsb()
    .download_structure_to_path(pdb_id("4hhb"), StructureFormat::Pdb, &destination, |_, _| {})
    .await
    .unwrap_or_else(|error| panic!("existing destination should be replaced: {error}"));

  assert_eq!(
    tokio::fs::read(&destination)
      .await
      .unwrap_or_else(|error| panic!("replaced destination should be readable: {error}")),
    b"new content"
  );
}

#[tokio::test]
async fn cancelled_download_should_not_leave_a_partial_destination() {
  let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory should be created: {error}"));
  let destination = directory.path().join("cancelled.pdb");
  let transport = MockTransport::new(vec![Ok(response(StatusCode::OK, b"streamed"))]);
  let client = test_client(transport);
  let cancellation = CancellationToken::new();
  cancellation.cancel();

  let result = client
    .rcsb()
    .download_structure_to_path_with_cancellation(
      pdb_id("4hhb"),
      StructureFormat::Pdb,
      &destination,
      |_, _| {},
      cancellation,
    )
    .await;

  assert!(matches!(
    result,
    Err(chitin_databases::providers::rcsb::RcsbDownloadError::Provider(
      chitin_databases::providers::rcsb::RcsbError::Transport(TransportError::Cancelled)
    ))
  ));
  assert!(!destination.exists());
}
