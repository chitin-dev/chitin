use std::{
  collections::VecDeque,
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use chitin_databases::{
  ArtifactFormat, Client, ClientConfig, DownloadedArtifact, HttpRequest, HttpResponse, HttpTransport, ProviderId,
  RetryPolicy, TransportError,
  providers::rcsb::{PdbId, PdbIdError, RcsbEndpoints, RcsbError, StructureFormat},
};
use http::{
  HeaderMap, HeaderValue, StatusCode,
  header::{CONTENT_LENGTH, ETAG, LAST_MODIFIED},
};

#[derive(Clone)]
struct MockTransport {
  responses: Arc<Mutex<VecDeque<Result<HttpResponse, TransportError>>>>,
  requests: Arc<Mutex<Vec<HttpRequest>>>,
  active: Arc<AtomicUsize>,
  max_active: Arc<AtomicUsize>,
  delay: Duration,
}

impl MockTransport {
  fn new(responses: Vec<Result<HttpResponse, TransportError>>) -> Self {
    Self {
      responses: Arc::new(Mutex::new(VecDeque::from(responses))),
      requests: Arc::new(Mutex::new(Vec::new())),
      active: Arc::new(AtomicUsize::new(0)),
      max_active: Arc::new(AtomicUsize::new(0)),
      delay: Duration::ZERO,
    }
  }

  fn with_delay(mut self, delay: Duration) -> Self {
    self.delay = delay;
    self
  }

  fn request_count(&self) -> usize {
    self.requests.lock().map(|requests| requests.len()).unwrap_or_default()
  }

  fn first_request_url(&self) -> Option<String> {
    self
      .requests
      .lock()
      .ok()
      .and_then(|requests| requests.first().map(|request| request.url.to_string()))
  }

  fn max_active(&self) -> usize {
    self.max_active.load(Ordering::SeqCst)
  }
}

#[async_trait]
impl HttpTransport for MockTransport {
  async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
    if let Ok(mut requests) = self.requests.lock() {
      requests.push(request);
    }

    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
    self.max_active.fetch_max(active, Ordering::SeqCst);
    if !self.delay.is_zero() {
      tokio::time::sleep(self.delay).await;
    }
    self.active.fetch_sub(1, Ordering::SeqCst);

    match self.responses.lock() {
      Ok(mut responses) => responses
        .pop_front()
        .unwrap_or_else(|| Ok(response(StatusCode::OK, b""))),
      Err(_) => Err(TransportError::Other {
        message: "mock response lock poisoned".to_string(),
      }),
    }
  }
}

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

fn response(status: StatusCode, body: &'static [u8]) -> HttpResponse {
  HttpResponse {
    status,
    headers: HeaderMap::new(),
    body: Bytes::from_static(body),
  }
}

fn response_with_headers(status: StatusCode, body: &'static [u8], headers: HeaderMap) -> HttpResponse {
  HttpResponse {
    status,
    headers,
    body: Bytes::from_static(body),
  }
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
