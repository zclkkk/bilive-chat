use md5::{Digest, Md5};
use serde::Deserialize;

use super::http::{self, HttpClient, HttpError};

const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

pub fn get_mixin_key(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|&i| chars.get(i).copied())
        .take(32)
        .collect()
}

pub fn sign_wbi(params: &serde_json::Value, mixin_key: &str, wts: u64) -> String {
    let mut all = match params {
        serde_json::Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    all.insert("wts".into(), serde_json::Value::Number(wts.into()));

    let mut keys: Vec<&String> = all.keys().collect();
    keys.sort();

    let query: String = keys
        .iter()
        .map(|k| {
            let v = stringify_wbi_param(&all[*k]);
            let v_cleaned: String = v
                .chars()
                .filter(|c| !matches!(c, '\'' | '!' | '(' | ')' | '*'))
                .collect();
            format!(
                "{}={}",
                urlencoding::encode(k),
                urlencoding::encode(&v_cleaned)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    let mut hasher = Md5::new();
    hasher.update(query.as_bytes());
    hasher.update(mixin_key.as_bytes());
    let w_rid = hex::encode(hasher.finalize());

    format!("{query}&w_rid={w_rid}")
}

fn stringify_wbi_param(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => value.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct WebLiveAuth {
    pub uid: Option<u64>,
    pub room_id: u64,
    pub key: String,
    pub buvid3: String,
    pub urls: Vec<String>,
}

fn validate_ws_url(url: &str) -> Result<(), AuthError> {
    let parsed = url
        .parse::<hyper::Uri>()
        .map_err(|_| AuthError::InvalidOutput(format!("unparseable WebSocket URL: {url}")))?;
    if parsed.scheme_str() != Some("wss") {
        return Err(AuthError::InvalidOutput(format!(
            "WebSocket URL must use wss scheme: {url}"
        )));
    }
    let host = parsed.host().unwrap_or("");
    if host.is_empty() {
        return Err(AuthError::InvalidOutput(format!(
            "WebSocket URL has empty host: {url}"
        )));
    }
    if parsed.path() != "/sub" {
        return Err(AuthError::InvalidOutput(format!(
            "WebSocket URL must have path /sub: {url}"
        )));
    }
    Ok(())
}

impl WebLiveAuth {
    pub fn validate(&self) -> Result<(), AuthError> {
        if self.room_id == 0 {
            return Err(AuthError::InvalidOutput("resolved room_id is 0".into()));
        }
        if self.key.is_empty() {
            return Err(AuthError::InvalidOutput("danmu token is empty".into()));
        }
        if self.buvid3.is_empty() {
            return Err(AuthError::InvalidOutput("buvid3 is empty".into()));
        }
        if self.urls.is_empty() {
            return Err(AuthError::InvalidOutput(
                "WebSocket URL list is empty".into(),
            ));
        }
        for url in &self.urls {
            validate_ws_url(url)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] HttpError),
    #[error("API error code {code}: {message}")]
    Api { code: i64, message: String },
    #[error("missing data: {0}")]
    MissingData(String),
    #[error("cookie present but not logged in")]
    CookieNotLoggedIn,
    #[error("invalid output: {0}")]
    InvalidOutput(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct BiliResponse<T> {
    pub code: i64,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

impl<T> BiliResponse<T> {
    pub fn check_code(self) -> Result<Self, AuthError> {
        if self.code != 0 {
            return Err(AuthError::Api {
                code: self.code,
                message: self.message.unwrap_or_default(),
            });
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpiData {
    pub b_3: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoomInitData {
    pub room_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NavData {
    pub wbi_img: WbiImg,
    #[serde(default)]
    pub mid: Option<u64>,
    #[serde(default, rename = "isLogin")]
    pub is_login: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WbiImg {
    pub img_url: String,
    pub sub_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DanmuInfoData {
    pub token: String,
    pub host_list: Vec<HostEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostEntry {
    pub host: String,
    #[serde(default)]
    pub wss_port: Option<u16>,
}

pub fn cookie_value(cookie: &str, name: &str) -> Option<String> {
    cookie.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_string())
    })
}

pub fn cookie_with_buvid3(cookie: Option<&str>, buvid3: &str) -> String {
    match cookie {
        Some(c) if cookie_value(c, "buvid3").is_some() => c.to_string(),
        Some(c) if !c.trim().is_empty() => format!("{}; buvid3={buvid3}", c.trim()),
        _ => format!("buvid3={buvid3}"),
    }
}

#[async_trait::async_trait]
pub trait BiliApi: Send + Sync {
    async fn fetch_spi(&self) -> Result<BiliResponse<SpiData>, AuthError>;
    async fn fetch_room_init(
        &self,
        room_id: u64,
        cookie_header: &str,
    ) -> Result<BiliResponse<RoomInitData>, AuthError>;
    async fn fetch_nav(&self, cookie_header: &str) -> Result<BiliResponse<NavData>, AuthError>;
    async fn fetch_danmu_info(
        &self,
        signed_query: &str,
        cookie_header: &str,
    ) -> Result<BiliResponse<DanmuInfoData>, AuthError>;
}

pub struct LiveBiliApi {
    client: HttpClient,
}

impl LiveBiliApi {
    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl BiliApi for LiveBiliApi {
    async fn fetch_spi(&self) -> Result<BiliResponse<SpiData>, AuthError> {
        Ok(http::api_get(
            &self.client,
            "https://api.bilibili.com/x/frontend/finger/spi",
            &[],
        )
        .await?)
    }

    async fn fetch_room_init(
        &self,
        room_id: u64,
        cookie_header: &str,
    ) -> Result<BiliResponse<RoomInitData>, AuthError> {
        let headers = if cookie_header.is_empty() {
            Vec::new()
        } else {
            vec![("Cookie", cookie_header)]
        };
        Ok(http::api_get(
            &self.client,
            &format!("https://api.live.bilibili.com/room/v1/Room/mobileRoomInit?id={room_id}"),
            &headers,
        )
        .await?)
    }

    async fn fetch_nav(&self, cookie_header: &str) -> Result<BiliResponse<NavData>, AuthError> {
        let headers = if cookie_header.is_empty() {
            Vec::new()
        } else {
            vec![("Cookie", cookie_header)]
        };
        Ok(http::api_get(
            &self.client,
            "https://api.bilibili.com/x/web-interface/nav",
            &headers,
        )
        .await?)
    }

    async fn fetch_danmu_info(
        &self,
        signed_query: &str,
        cookie_header: &str,
    ) -> Result<BiliResponse<DanmuInfoData>, AuthError> {
        Ok(http::api_get(
            &self.client,
            &format!(
                "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo?{signed_query}"
            ),
            &[
                ("Referer", "https://live.bilibili.com/"),
                ("Cookie", cookie_header),
            ],
        )
        .await?)
    }
}

pub async fn prepare(
    api: &dyn BiliApi,
    room_id: u64,
    cookie: Option<&str>,
    wts: u64,
) -> Result<WebLiveAuth, AuthError> {
    if room_id == 0 {
        return Err(AuthError::InvalidOutput("room_id is 0".into()));
    }

    let has_cookie = cookie.is_some_and(|c| !c.trim().is_empty());

    let buvid3 = get_buvid3(api, cookie).await?;
    let cookie_header = cookie_with_buvid3(cookie, &buvid3);

    let long_room_id = resolve_room_id(api, room_id, &cookie_header).await?;

    let (mixin_key, uid, is_login) = fetch_nav_data(api, &cookie_header, has_cookie).await?;

    if has_cookie && (!is_login || uid.is_none()) {
        return Err(AuthError::CookieNotLoggedIn);
    }

    let signed = sign_wbi(
        &serde_json::json!({
            "id": long_room_id,
            "type": 0,
            "web_location": "444.8"
        }),
        &mixin_key,
        wts,
    );

    let danmu_resp = api.fetch_danmu_info(&signed, &cookie_header).await?;
    let danmu_resp = danmu_resp.check_code()?;
    let danmu_data = danmu_resp
        .data
        .ok_or_else(|| AuthError::MissingData("danmuInfo data missing".into()))?;

    let urls: Vec<String> = danmu_data
        .host_list
        .iter()
        .filter(|h| !h.host.is_empty())
        .map(|h| format!("wss://{}:{}/sub", h.host, h.wss_port.unwrap_or(443)))
        .collect();

    let auth = WebLiveAuth {
        uid,
        room_id: long_room_id,
        key: danmu_data.token,
        buvid3,
        urls,
    };

    auth.validate()?;
    Ok(auth)
}

async fn get_buvid3(api: &dyn BiliApi, cookie: Option<&str>) -> Result<String, AuthError> {
    if let Some(buvid3) = cookie.and_then(|c| cookie_value(c, "buvid3")) {
        return Ok(buvid3);
    }

    let resp = api.fetch_spi().await?.check_code()?;
    Ok(resp.data.map(|d| d.b_3).unwrap_or_default())
}

async fn resolve_room_id(
    api: &dyn BiliApi,
    room_id: u64,
    cookie_header: &str,
) -> Result<u64, AuthError> {
    let resp = api.fetch_room_init(room_id, cookie_header).await?;
    let resp = resp.check_code()?;
    Ok(resp.data.map(|d| d.room_id).unwrap_or(room_id))
}

async fn fetch_nav_data(
    api: &dyn BiliApi,
    cookie_header: &str,
    has_cookie: bool,
) -> Result<(String, Option<u64>, bool), AuthError> {
    let nav = api.fetch_nav(cookie_header).await?;

    if has_cookie {
        if nav.code != 0 {
            return Err(AuthError::CookieNotLoggedIn);
        }
        let nav_data = nav.data.ok_or_else(|| AuthError::CookieNotLoggedIn)?;
        if !nav_data.is_login {
            return Err(AuthError::CookieNotLoggedIn);
        }
        let uid = nav_data.mid.filter(|uid| *uid > 0);
        if uid.is_none() {
            return Err(AuthError::CookieNotLoggedIn);
        }
        let mixin_key = extract_mixin_key(&nav_data.wbi_img)?;
        Ok((mixin_key, uid, true))
    } else {
        if nav.code != 0 && nav.data.is_none() {
            return Err(AuthError::MissingData(
                "nav returned error without data".into(),
            ));
        }
        let nav_data = nav
            .data
            .ok_or_else(|| AuthError::MissingData("nav data missing".into()))?;
        let mixin_key = extract_mixin_key(&nav_data.wbi_img)?;
        let uid = nav_data.mid.filter(|uid| *uid > 0);
        Ok((mixin_key, uid, nav_data.is_login))
    }
}

fn extract_mixin_key(wbi_img: &WbiImg) -> Result<String, AuthError> {
    let img_key = wbi_img
        .img_url
        .rsplit('/')
        .next()
        .and_then(|s| s.split('.').next())
        .unwrap_or("");
    if img_key.is_empty() {
        return Err(AuthError::MissingData(
            "nav wbi_img.img_url yielded empty key".into(),
        ));
    }

    let sub_key = wbi_img
        .sub_url
        .rsplit('/')
        .next()
        .and_then(|s| s.split('.').next())
        .unwrap_or("");
    if sub_key.is_empty() {
        return Err(AuthError::MissingData(
            "nav wbi_img.sub_url yielded empty key".into(),
        ));
    }

    let mixin_key = get_mixin_key(&format!("{img_key}{sub_key}"));
    if mixin_key.len() < 32 {
        return Err(AuthError::InvalidOutput(format!(
            "mixin_key too short: {} chars (need 32)",
            mixin_key.len()
        )));
    }

    Ok(mixin_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixin_key_length() {
        let key = get_mixin_key(
            "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789",
        );
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_sign_wbi_produces_query() {
        let params = serde_json::json!({"id": 12345});
        let mixin_key = "0123456789abcdef0123456789abcdef";
        let result = sign_wbi(&params, mixin_key, 1700000000);
        assert!(result.contains("wts=1700000000"));
        assert!(result.contains("w_rid="));
        assert!(result.contains("id=12345"));
    }

    #[test]
    fn test_sign_wbi_string_params_not_json_quoted() {
        let params = serde_json::json!({"id": 12345, "web_location": "444.8"});
        let mixin_key = "0123456789abcdef0123456789abcdef";
        let result = sign_wbi(&params, mixin_key, 1700000000);
        assert!(result.contains("web_location=444.8"));
        assert!(!result.contains("web_location=%22444.8%22"));
    }

    #[test]
    fn test_sign_wbi_strips_special_chars() {
        let params = serde_json::json!({"q": "hello'world!"});
        let mixin_key = "0123456789abcdef0123456789abcdef";
        let result = sign_wbi(&params, mixin_key, 1700000000);
        assert!(result.contains("q=helloworld"));
    }

    #[test]
    fn test_cookie_value_extract() {
        assert_eq!(
            cookie_value("a=1; buvid3=abc; c=3", "buvid3"),
            Some("abc".to_string())
        );
        assert_eq!(cookie_value("a=1; c=3", "buvid3"), None);
        assert_eq!(cookie_value("", "buvid3"), None);
    }

    #[test]
    fn test_cookie_with_buvid3_append() {
        assert_eq!(
            cookie_with_buvid3(Some("SESSDATA=abc"), "xyz"),
            "SESSDATA=abc; buvid3=xyz"
        );
    }

    #[test]
    fn test_cookie_with_buvid3_already_present() {
        assert_eq!(
            cookie_with_buvid3(Some("buvid3=old; SESSDATA=abc"), "new"),
            "buvid3=old; SESSDATA=abc"
        );
    }

    #[test]
    fn test_cookie_with_buvid3_no_cookie() {
        assert_eq!(cookie_with_buvid3(None, "xyz"), "buvid3=xyz");
    }

    #[test]
    fn test_web_live_auth_validate_rejects_zero_room_id() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 0,
            key: "token".into(),
            buvid3: "b3".into(),
            urls: vec!["wss://example.com:443/sub".into()],
        };
        assert!(auth.validate().is_err());
    }

    #[test]
    fn test_web_live_auth_validate_rejects_empty_key() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 1,
            key: String::new(),
            buvid3: "b3".into(),
            urls: vec!["wss://example.com:443/sub".into()],
        };
        assert!(auth.validate().is_err());
    }

    #[test]
    fn test_web_live_auth_validate_rejects_empty_buvid3() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 1,
            key: "token".into(),
            buvid3: String::new(),
            urls: vec!["wss://example.com:443/sub".into()],
        };
        assert!(auth.validate().is_err());
    }

    #[test]
    fn test_web_live_auth_validate_rejects_empty_urls() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 1,
            key: "token".into(),
            buvid3: "b3".into(),
            urls: vec![],
        };
        assert!(auth.validate().is_err());
    }

    #[test]
    fn test_web_live_auth_validate_rejects_non_wss_url() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 1,
            key: "token".into(),
            buvid3: "b3".into(),
            urls: vec!["https://example.com/sub".into()],
        };
        assert!(auth.validate().is_err());
    }

    #[test]
    fn test_web_live_auth_validate_rejects_url_wrong_path() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 1,
            key: "token".into(),
            buvid3: "b3".into(),
            urls: vec!["wss://example.com:443/ws".into()],
        };
        let err = auth.validate().unwrap_err();
        assert!(matches!(err, AuthError::InvalidOutput(_)));
        assert!(err.to_string().contains("/sub"));
    }

    #[test]
    fn test_web_live_auth_validate_rejects_url_empty_host() {
        let auth = WebLiveAuth {
            uid: Some(123),
            room_id: 1,
            key: "token".into(),
            buvid3: "b3".into(),
            urls: vec!["wss://:443/sub".into()],
        };
        assert!(auth.validate().is_err());
    }

    #[test]
    fn test_web_live_auth_validate_accepts_valid() {
        let auth = WebLiveAuth {
            uid: None,
            room_id: 12345,
            key: "token".into(),
            buvid3: "b3".into(),
            urls: vec!["wss://example.com:443/sub".into()],
        };
        assert!(auth.validate().is_ok());
    }

    #[test]
    fn test_bili_response_check_code_success() {
        let resp: BiliResponse<SpiData> = BiliResponse {
            code: 0,
            message: Some("0".into()),
            data: Some(SpiData { b_3: "test".into() }),
        };
        assert!(resp.check_code().is_ok());
    }

    #[test]
    fn test_bili_response_check_code_error() {
        let resp: BiliResponse<SpiData> = BiliResponse {
            code: -401,
            message: Some("access denied".into()),
            data: None,
        };
        match resp.check_code().unwrap_err() {
            AuthError::Api { code, message } => {
                assert_eq!(code, -401);
                assert_eq!(message, "access denied");
            }
            other => panic!("expected Api error, got {other}"),
        }
    }

    struct MockBiliApi {
        spi: BiliResponse<SpiData>,
        room_init: BiliResponse<RoomInitData>,
        nav: BiliResponse<NavData>,
        danmu_info: BiliResponse<DanmuInfoData>,
    }

    #[async_trait::async_trait]
    impl BiliApi for MockBiliApi {
        async fn fetch_spi(&self) -> Result<BiliResponse<SpiData>, AuthError> {
            Ok(self.spi.clone())
        }
        async fn fetch_room_init(
            &self,
            _room_id: u64,
            _cookie_header: &str,
        ) -> Result<BiliResponse<RoomInitData>, AuthError> {
            Ok(self.room_init.clone())
        }
        async fn fetch_nav(
            &self,
            _cookie_header: &str,
        ) -> Result<BiliResponse<NavData>, AuthError> {
            Ok(self.nav.clone())
        }
        async fn fetch_danmu_info(
            &self,
            _signed_query: &str,
            _cookie_header: &str,
        ) -> Result<BiliResponse<DanmuInfoData>, AuthError> {
            Ok(self.danmu_info.clone())
        }
    }

    fn success_response<T: Clone>(data: T) -> BiliResponse<T> {
        BiliResponse {
            code: 0,
            message: Some("0".into()),
            data: Some(data),
        }
    }

    fn api_error_response<T: Clone>(code: i64, message: &str) -> BiliResponse<T> {
        BiliResponse {
            code,
            message: Some(message.into()),
            data: None,
        }
    }

    fn nav_error_with_data(code: i64, message: &str, data: NavData) -> BiliResponse<NavData> {
        BiliResponse {
            code,
            message: Some(message.into()),
            data: Some(data),
        }
    }

    fn fixture_nav_data(logged_in: bool, uid: u64) -> NavData {
        NavData {
            wbi_img: WbiImg {
                img_url: "https://i0.hdslb.com/bfs/wbi/7cd08e575cd84113b7e5a4c2e8a5e9a2.png".into(),
                sub_url: "https://i0.hdslb.com/bfs/wbi/2a0a7c1f6ef4be0b5f486d7e5e4b2e1a.png".into(),
            },
            mid: if logged_in && uid > 0 {
                Some(uid)
            } else {
                None
            },
            is_login: logged_in,
        }
    }

    fn fixture_danmu_data() -> DanmuInfoData {
        DanmuInfoData {
            token: "test-danmu-token".into(),
            host_list: vec![HostEntry {
                host: "hw-bj-live-comet-01.chat.bilibili.com".into(),
                wss_port: Some(443),
            }],
        }
    }

    #[tokio::test]
    async fn test_prepare_guest_success() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await.unwrap();
        assert_eq!(result.uid, None);
        assert_eq!(result.room_id, 12345);
        assert_eq!(result.key, "test-danmu-token");
        assert_eq!(result.buvid3, "test-buvid3");
        assert_eq!(result.urls.len(), 1);
        assert!(result.urls[0].starts_with("wss://"));
    }

    #[tokio::test]
    async fn test_prepare_logged_in_success() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(true, 98765)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, Some("SESSDATA=abc"), 1700000000)
            .await
            .unwrap();
        assert_eq!(result.uid, Some(98765));
        assert_eq!(result.room_id, 12345);
    }

    #[tokio::test]
    async fn test_prepare_cookie_not_logged_in_with_sessdata() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, Some("SESSDATA=abc"), 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::CookieNotLoggedIn));
    }

    #[tokio::test]
    async fn test_prepare_cookie_not_logged_in_without_sessdata() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, Some("bili_jct=xyz"), 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::CookieNotLoggedIn));
    }

    #[tokio::test]
    async fn test_prepare_api_error_on_danmu_info() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: api_error_response(60004, "room not found"),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        match result.unwrap_err() {
            AuthError::Api { code, message } => {
                assert_eq!(code, 60004);
                assert_eq!(message, "room not found");
            }
            other => panic!("expected Api error, got {other}"),
        }
    }

    #[tokio::test]
    async fn test_prepare_api_error_on_spi() {
        let api = MockBiliApi {
            spi: api_error_response(-401, "access denied"),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        match result.unwrap_err() {
            AuthError::Api { code, message } => {
                assert_eq!(code, -401);
                assert_eq!(message, "access denied");
            }
            other => panic!("expected Api error, got {other}"),
        }
    }

    #[tokio::test]
    async fn test_prepare_api_error_on_room_init() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: api_error_response(60004, "room not found"),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        match result.unwrap_err() {
            AuthError::Api { code, message } => {
                assert_eq!(code, 60004);
                assert_eq!(message, "room not found");
            }
            other => panic!("expected Api error, got {other}"),
        }
    }

    #[tokio::test]
    async fn test_prepare_guest_nav_code_minus101_with_data_succeeds() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: nav_error_with_data(-101, "账号未登录", fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await.unwrap();
        assert_eq!(result.uid, None);
        assert_eq!(result.room_id, 12345);
        assert_eq!(result.key, "test-danmu-token");
    }

    #[tokio::test]
    async fn test_prepare_guest_nav_code_minus101_without_data_fails() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: api_error_response(-101, "账号未登录"),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(
            matches!(result.unwrap_err(), AuthError::MissingData(_)),
            "guest nav -101 without data should be MissingData"
        );
    }

    #[tokio::test]
    async fn test_prepare_cookie_nav_code_minus101_returns_not_logged_in() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: nav_error_with_data(-101, "账号未登录", fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, Some("SESSDATA=abc"), 1700000000).await;
        assert!(
            matches!(result.unwrap_err(), AuthError::CookieNotLoggedIn),
            "cookie-present nav -101 should be CookieNotLoggedIn"
        );
    }

    #[tokio::test]
    async fn test_prepare_cookie_nav_code_0_but_not_logged_in() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, Some("SESSDATA=abc"), 1700000000).await;
        assert!(
            matches!(result.unwrap_err(), AuthError::CookieNotLoggedIn),
            "cookie-present nav code=0 isLogin=false should be CookieNotLoggedIn"
        );
    }

    #[tokio::test]
    async fn test_prepare_missing_nav_data() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: BiliResponse {
                code: 0,
                message: None,
                data: None,
            },
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::MissingData(_)));
    }

    #[tokio::test]
    async fn test_prepare_zero_room_id() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 0 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 0, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::InvalidOutput(_)));
    }

    #[tokio::test]
    async fn test_prepare_empty_host_list() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(DanmuInfoData {
                token: "token".into(),
                host_list: vec![],
            }),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::InvalidOutput(_)));
    }

    #[tokio::test]
    async fn test_prepare_empty_host_entry_filtered() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(DanmuInfoData {
                token: "token".into(),
                host_list: vec![HostEntry {
                    host: String::new(),
                    wss_port: Some(443),
                }],
            }),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::InvalidOutput(_)));
    }

    #[tokio::test]
    async fn test_prepare_host_entry_default_port() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(DanmuInfoData {
                token: "token".into(),
                host_list: vec![HostEntry {
                    host: "example.com".into(),
                    wss_port: None,
                }],
            }),
        };
        let result = prepare(&api, 12345, None, 1700000000).await.unwrap();
        assert_eq!(result.urls[0], "wss://example.com:443/sub");
    }

    #[tokio::test]
    async fn test_prepare_buvid3_from_spi_when_no_cookie() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "from-spi".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await.unwrap();
        assert_eq!(result.buvid3, "from-spi");
    }

    #[tokio::test]
    async fn test_prepare_buvid3_cookie_only_rejected_as_not_logged_in() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "from-spi".into(),
            }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, Some("buvid3=from-cookie"), 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::CookieNotLoggedIn));
    }

    #[tokio::test]
    async fn test_prepare_room_init_fallback_on_missing_data() {
        let api = MockBiliApi {
            spi: success_response(SpiData {
                b_3: "test-buvid3".into(),
            }),
            room_init: BiliResponse {
                code: 0,
                message: None,
                data: None,
            },
            nav: success_response(fixture_nav_data(false, 0)),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await.unwrap();
        assert_eq!(result.room_id, 12345);
    }

    #[tokio::test]
    async fn test_prepare_nav_empty_img_url() {
        let mut nav = fixture_nav_data(false, 0);
        nav.wbi_img.img_url = String::new();
        let api = MockBiliApi {
            spi: success_response(SpiData { b_3: "b3".into() }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(nav),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::MissingData(_)));
    }

    #[tokio::test]
    async fn test_prepare_nav_empty_sub_url() {
        let mut nav = fixture_nav_data(false, 0);
        nav.wbi_img.sub_url = String::new();
        let api = MockBiliApi {
            spi: success_response(SpiData { b_3: "b3".into() }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(nav),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::MissingData(_)));
    }

    #[tokio::test]
    async fn test_prepare_nav_key_material_too_short() {
        let mut nav = fixture_nav_data(false, 0);
        nav.wbi_img.img_url = "https://example.com/a.png".into();
        nav.wbi_img.sub_url = "https://example.com/b.png".into();
        let api = MockBiliApi {
            spi: success_response(SpiData { b_3: "b3".into() }),
            room_init: success_response(RoomInitData { room_id: 12345 }),
            nav: success_response(nav),
            danmu_info: success_response(fixture_danmu_data()),
        };
        let result = prepare(&api, 12345, None, 1700000000).await;
        assert!(matches!(result.unwrap_err(), AuthError::InvalidOutput(_)));
    }
}
