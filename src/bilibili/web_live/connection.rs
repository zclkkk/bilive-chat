use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, Mutex};

use super::auth::{self, AuthError};
use super::commands;
use super::http::HttpClient;
use super::socket::{self, SocketHandle, SocketStatus};
use crate::chat::filter::ChatFilter;
use crate::config::FilterOptions;
use crate::overlay::event::{OverlayEvent, PanelEvent};

pub struct LiveConnection {
    inner: Mutex<ConnectionInner>,
    http_client: HttpClient,
    panel_tx: broadcast::Sender<PanelEvent>,
    overlay_tx: broadcast::Sender<OverlayEvent>,
    filter_rx: tokio::sync::watch::Receiver<FilterOptions>,
    next_generation: AtomicU64,
}

enum ConnectionInner {
    Idle,
    Starting(u64),
    Active(ActiveConnection),
}

struct ActiveConnection {
    handle: SocketHandle,
    relay_task: tokio::task::JoinHandle<()>,
    generation: u64,
}

impl LiveConnection {
    pub fn new(
        http_client: HttpClient,
        panel_tx: broadcast::Sender<PanelEvent>,
        overlay_tx: broadcast::Sender<OverlayEvent>,
        filter_rx: tokio::sync::watch::Receiver<FilterOptions>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(ConnectionInner::Idle),
            http_client,
            panel_tx,
            overlay_tx,
            filter_rx,
            next_generation: AtomicU64::new(0),
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        room_id: u64,
        cookie: Option<String>,
    ) -> Result<(), StartError> {
        tracing::info!("start requested for room {room_id}");
        let generation = {
            let mut guard = self.inner.lock().await;
            match &*guard {
                ConnectionInner::Active(_) | ConnectionInner::Starting(_) => {
                    return Err(StartError::AlreadyRunning)
                }
                ConnectionInner::Idle => {}
            }
            let gen = self.next_generation.fetch_add(1, Ordering::Relaxed);
            *guard = ConnectionInner::Starting(gen);
            gen
        };

        let api = auth::LiveBiliApi::new(self.http_client.clone());
        let wts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cookie_ref = cookie.as_deref();
        let web_auth = match auth::prepare(&api, room_id, cookie_ref, wts).await {
            Ok(a) => a,
            Err(e) => {
                let mut guard = self.inner.lock().await;
                if let ConnectionInner::Starting(gen) = &*guard {
                    if *gen == generation {
                        *guard = ConnectionInner::Idle;
                        return Err(match e {
                            AuthError::CookieNotLoggedIn => StartError::CookieNotLoggedIn,
                            other => StartError::Auth(other),
                        });
                    }
                }
                return Err(StartError::Cancelled);
            }
        };

        {
            let guard = self.inner.lock().await;
            match &*guard {
                ConnectionInner::Starting(gen) if *gen == generation => {}
                _ => return Err(StartError::Cancelled),
            }
        }

        let (handle, command_rx) = socket::connect(web_auth);

        let conn = Arc::clone(self);
        let status_rx = handle.status_rx.clone();
        let filter_rx = self.filter_rx.clone();
        let relay_task = tokio::spawn(async move {
            conn.relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut guard = self.inner.lock().await;
        match &*guard {
            ConnectionInner::Starting(gen) if *gen == generation => {
                *guard = ConnectionInner::Active(ActiveConnection {
                    handle,
                    relay_task,
                    generation,
                });
            }
            _ => {
                handle.stop();
                relay_task.abort();
                return Err(StartError::Cancelled);
            }
        }

        Ok(())
    }

    pub async fn stop(&self) -> bool {
        let mut guard = self.inner.lock().await;
        match std::mem::replace(&mut *guard, ConnectionInner::Idle) {
            ConnectionInner::Active(active) => {
                tracing::info!("stopping connection");
                active.handle.stop();
                active.relay_task.abort();
                true
            }
            ConnectionInner::Starting(_) => true,
            ConnectionInner::Idle => false,
        }
    }

    pub async fn status(&self) -> SocketStatus {
        let guard = self.inner.lock().await;
        match &*guard {
            ConnectionInner::Idle => SocketStatus::Disconnected { error: None },
            ConnectionInner::Starting(_) => SocketStatus::Connecting {},
            ConnectionInner::Active(active) => active.handle.status_rx.borrow().clone(),
        }
    }
}

impl LiveConnection {
    async fn relay_loop(
        self: Arc<Self>,
        mut status_rx: tokio::sync::watch::Receiver<SocketStatus>,
        mut command_rx: tokio::sync::mpsc::Receiver<serde_json::Value>,
        mut filter_rx: tokio::sync::watch::Receiver<FilterOptions>,
        generation: u64,
    ) {
        let mut filter = ChatFilter::new(&filter_rx.borrow());

        loop {
            tokio::select! {
                biased;
                result = status_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                    let status = status_rx.borrow().clone();
                    let _ = self.panel_tx.send(PanelEvent::Status { status });
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(value) => {
                            if let Some(event) = commands::parse_command(&value) {
                                if !filter.should_block(&event) {
                                    let _ = self.overlay_tx.send(OverlayEvent::from(&event));
                                }
                            }
                        }
                        None => break,
                    }
                }
                result = filter_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                    filter = ChatFilter::new(&filter_rx.borrow());
                }
            }
        }

        let mut guard = self.inner.lock().await;
        if let ConnectionInner::Active(active) = &*guard {
            if active.generation == generation {
                tracing::info!("relay loop exited, resetting to idle");
                *guard = ConnectionInner::Idle;
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("already running")]
    AlreadyRunning,
    #[error("cancelled")]
    Cancelled,
    #[error("cookie present but not logged in")]
    CookieNotLoggedIn,
    #[error("auth error: {0}")]
    Auth(#[from] AuthError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::event::OverlayEvent;

    fn test_filter_rx() -> tokio::sync::watch::Receiver<FilterOptions> {
        let (_, rx) = tokio::sync::watch::channel(FilterOptions::default());
        rx
    }

    #[tokio::test]
    async fn test_status_disconnected_when_idle() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());
        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_stop_returns_false_when_idle() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());
        assert!(!conn.stop().await);
    }

    #[tokio::test]
    async fn test_start_rejects_zero_room_id() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());
        let result = conn.start(0, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            StartError::Auth(AuthError::InvalidOutput(_))
        ));
    }

    #[tokio::test]
    async fn test_start_rejects_already_running() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let mut inner = conn.inner.lock().await;
        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let cancel = tokio_util::sync::CancellationToken::new();
        let handle = SocketHandle { status_rx, cancel };
        let relay_task = tokio::spawn(async {});
        *inner = ConnectionInner::Active(ActiveConnection {
            handle,
            relay_task,
            generation: 0,
        });
        drop(inner);

        let result = conn.start(12345, None).await;
        assert!(matches!(result.unwrap_err(), StartError::AlreadyRunning));

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_start_rejects_already_starting() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(99);
        drop(inner);

        let result = conn.start(12345, None).await;
        assert!(matches!(result.unwrap_err(), StartError::AlreadyRunning));

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_status_connecting_while_starting() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(1);
        drop(inner);

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Connecting {}));

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_stop_returns_true_when_starting() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(1);
        drop(inner);

        assert!(conn.stop().await);
    }

    #[tokio::test]
    async fn test_stop_during_starting_resets_to_idle() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(5);
        drop(inner);

        assert!(conn.stop().await);

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_start_resets_to_idle_on_auth_failure() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let _ = conn.start(0, None).await;

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_start_rejected_when_already_starting() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(99);
        drop(inner);

        let result = conn.start(12345, None).await;
        assert!(matches!(result.unwrap_err(), StartError::AlreadyRunning));

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_auth_failure_returns_cancelled_if_generation_mismatch() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let _old_gen = {
            let mut inner = conn.inner.lock().await;
            let gen = conn.next_generation.fetch_add(1, Ordering::Relaxed);
            *inner = ConnectionInner::Starting(gen);
            gen
        };

        conn.stop().await;

        {
            let mut inner = conn.inner.lock().await;
            let newer_gen = conn.next_generation.fetch_add(1, Ordering::Relaxed);
            *inner = ConnectionInner::Starting(newer_gen);
        }

        let result = conn.start(0, None).await;
        assert!(
            matches!(result.unwrap_err(), StartError::AlreadyRunning),
            "start should reject immediately when a newer Starting is active"
        );

        conn.stop().await;

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Idle;
        drop(inner);

        let result = conn.start(0, None).await;
        assert!(
            matches!(result.unwrap_err(), StartError::Auth(_)),
            "auth failure with matching generation should return Auth error"
        );

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_cancelled_socket_handle_stopped_on_superseded_start() {
        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let cancel = tokio_util::sync::CancellationToken::new();
        let handle = SocketHandle {
            status_rx,
            cancel: cancel.clone(),
        };

        assert!(!cancel.is_cancelled());

        handle.stop();

        assert!(cancel.is_cancelled());
    }

    #[tokio::test]
    async fn test_relay_loop_resets_state_on_exit() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let (status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 42;
        let filter_rx = conn.filter_rx.clone();
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Active(ActiveConnection {
            handle: SocketHandle {
                status_rx: status_tx.subscribe(),
                cancel: tokio_util::sync::CancellationToken::new(),
            },
            relay_task,
            generation,
        });
        drop(inner);

        drop(command_tx);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_relay_loop_broadcasts_parsed_command_to_overlay() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 100;
        let filter_rx = conn.filter_rx.clone();
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Active(ActiveConnection {
            handle: SocketHandle {
                status_rx: tokio::sync::watch::channel(SocketStatus::Connected {}).1,
                cancel: tokio_util::sync::CancellationToken::new(),
            },
            relay_task,
            generation,
        });
        drop(inner);

        let danmu = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "relay test",
                [55, "RelayUser", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu).await.unwrap();
        drop(command_tx);

        let received =
            tokio::time::timeout(std::time::Duration::from_millis(200), overlay_rx.recv())
                .await
                .expect("timeout waiting for overlay event")
                .expect("overlay channel closed");

        assert!(
            matches!(received, OverlayEvent::Normal { ref sender, ref text, .. } if sender == "RelayUser" && text == "relay test")
        );

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_relay_loop_skips_unknown_command() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 101;
        let filter_rx = conn.filter_rx.clone();
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Active(ActiveConnection {
            handle: SocketHandle {
                status_rx: tokio::sync::watch::channel(SocketStatus::Connected {}).1,
                cancel: tokio_util::sync::CancellationToken::new(),
            },
            relay_task,
            generation,
        });
        drop(inner);

        let unknown = serde_json::json!({"cmd": "ROOM_CHANGE", "data": {"title": "new"}});
        command_tx.send(unknown).await.unwrap();
        drop(command_tx);

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "unknown command should not produce overlay event"
        );

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_relay_loop_blocks_filtered_user() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (filter_tx, filter_rx) = tokio::sync::watch::channel(FilterOptions {
            blocked_users: vec!["BlockedUser".into()],
            blocked_keywords: vec![],
        });
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, filter_rx);

        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 200;
        let filter_rx = conn.filter_rx.clone();
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Active(ActiveConnection {
            handle: SocketHandle {
                status_rx: tokio::sync::watch::channel(SocketStatus::Connected {}).1,
                cancel: tokio_util::sync::CancellationToken::new(),
            },
            relay_task,
            generation,
        });
        drop(inner);

        let danmu = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "should be blocked",
                [99, "BlockedUser", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu).await.unwrap();
        drop(command_tx);
        drop(filter_tx);

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "filtered user should not produce overlay event"
        );

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_relay_loop_blocks_filtered_keyword() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (filter_tx, filter_rx) = tokio::sync::watch::channel(FilterOptions {
            blocked_users: vec![],
            blocked_keywords: vec!["forbidden".into()],
        });
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, filter_rx);

        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 201;
        let filter_rx = conn.filter_rx.clone();
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Active(ActiveConnection {
            handle: SocketHandle {
                status_rx: tokio::sync::watch::channel(SocketStatus::Connected {}).1,
                cancel: tokio_util::sync::CancellationToken::new(),
            },
            relay_task,
            generation,
        });
        drop(inner);

        let danmu = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "this is forbidden text",
                [88, "NormalUser", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu).await.unwrap();
        drop(command_tx);
        drop(filter_tx);

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "filtered keyword should not produce overlay event"
        );

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_filter_update_takes_effect_without_restart() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (filter_tx, filter_rx) = tokio::sync::watch::channel(FilterOptions::default());
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, filter_rx);

        let (_status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 300;
        let filter_rx = conn.filter_rx.clone();
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, filter_rx, generation)
                .await;
        });

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Active(ActiveConnection {
            handle: SocketHandle {
                status_rx: tokio::sync::watch::channel(SocketStatus::Connected {}).1,
                cancel: tokio_util::sync::CancellationToken::new(),
            },
            relay_task,
            generation,
        });
        drop(inner);

        let danmu1 = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "hello world",
                [42, "Alice", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu1).await.unwrap();

        let received =
            tokio::time::timeout(std::time::Duration::from_millis(200), overlay_rx.recv())
                .await
                .expect("timeout waiting for first overlay event")
                .expect("overlay channel closed");

        assert!(
            matches!(received, OverlayEvent::Normal { ref sender, ref text, .. } if sender == "Alice" && text == "hello world")
        );

        filter_tx
            .send(FilterOptions {
                blocked_users: vec!["Alice".into()],
                blocked_keywords: vec![],
            })
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let danmu2 = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "should be blocked now",
                [42, "Alice", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu2).await.unwrap();
        drop(command_tx);
        drop(filter_tx);

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "event from blocked user should not reach overlay after filter update"
        );

        conn.stop().await;
    }
}
