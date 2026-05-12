use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, StatusCode, Uri};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::de::DeserializeOwned;

pub type HttpBody = Full<Bytes>;

type InnerClient = Client<HttpsConnector<HttpConnector>, HttpBody>;

#[derive(Clone)]
pub struct HttpClient {
    inner: InnerClient,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(rustls::crypto::ring::default_provider())
            .expect("ring provider available")
            .https_only()
            .enable_http1()
            .build();
        let inner = Client::builder(TokioExecutor::new()).build(https);
        Self { inner }
    }

    pub async fn send(&self, req: Request<HttpBody>) -> Result<Response, HttpError> {
        let resp = self.inner.request(req).await?;
        let status = resp.status();
        let body = resp.into_body().collect().await?.to_bytes();
        Ok(Response { status, body })
    }
}

pub struct Response {
    pub status: StatusCode,
    pub body: Bytes,
}

pub fn build_uri(raw: &str) -> Result<Uri, HttpError> {
    raw.parse::<Uri>().map_err(|source| HttpError::InvalidUri {
        uri: raw.to_string(),
        source,
    })
}

pub fn empty_body() -> HttpBody {
    Full::new(Bytes::new())
}

pub async fn api_get<T: DeserializeOwned>(
    client: &HttpClient,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<T, HttpError> {
    let mut req = Request::builder()
        .method(hyper::Method::GET)
        .uri(build_uri(url)?)
        .header("User-Agent", UA);
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    let resp = client.send(req.body(empty_body())?).await?;
    if !resp.status.is_success() {
        return Err(HttpError::Status {
            status: resp.status,
            body: String::from_utf8_lossy(&resp.body).into_owned(),
        });
    }
    Ok(serde_json::from_slice(&resp.body)?)
}

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("invalid URI {uri}: {source}")]
    InvalidUri {
        uri: String,
        source: hyper::http::uri::InvalidUri,
    },
    #[error("request error: {0}")]
    Request(#[from] hyper::http::Error),
    #[error("HTTP client error: {0}")]
    Client(#[from] hyper_util::client::legacy::Error),
    #[error("response body error: {0}")]
    Body(#[from] hyper::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP {status}: {body}")]
    Status { status: StatusCode, body: String },
}
