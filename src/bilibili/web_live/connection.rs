use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, Mutex};

use super::auth::{self, AuthError};
use super::http::HttpClient;
use super::socket::{self, SocketHandle, SocketStatus};

pub struct LiveConnection {
    inner: Mutex<ConnectionInner>,
    http_client: HttpClient,
    panel_tx: broadcast::Sender<String>,
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
    pub fn new(http_client: HttpClient, panel_tx: broadcast::Sender<String>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(ConnectionInner::Idle),
            http_client,
            panel_tx,
            next_generation: AtomicU64::new(0),
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        room_id: u64,
        cookie: Option<String>,
    ) -> Result<(), StartError> {
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
                    }
                }
                return Err(match e {
                    AuthError::CookieNotLoggedIn => StartError::CookieNotLoggedIn,
                    other => StartError::Auth(other),
                });
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
        let relay_task = tokio::spawn(async move {
            conn.relay_loop(status_rx, command_rx, generation).await;
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
        generation: u64,
    ) {
        loop {
            tokio::select! {
                biased;
                result = status_rx.changed() => {
                    if result.is_err() {
                        break;
                    }
                    let status = status_rx.borrow().clone();
                    let msg = serde_json::json!({
                        "type": "status",
                        "status": status,
                    });
                    let _ = self.panel_tx.send(msg.to_string());
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(value) => {
                            tracing::debug!("command: {}", value);
                        }
                        None => break,
                    }
                }
            }
        }

        let mut guard = self.inner.lock().await;
        if let ConnectionInner::Active(active) = &*guard {
            if active.generation == generation {
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

    #[tokio::test]
    async fn test_status_disconnected_when_idle() {
        let http_client = HttpClient::new();
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);
        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_stop_returns_false_when_idle() {
        let http_client = HttpClient::new();
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);
        assert!(!conn.stop().await);
    }

    #[tokio::test]
    async fn test_start_rejects_zero_room_id() {
        let http_client = HttpClient::new();
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);
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
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

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
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

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
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

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
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(1);
        drop(inner);

        assert!(conn.stop().await);
    }

    #[tokio::test]
    async fn test_stop_during_starting_resets_to_idle() {
        let http_client = HttpClient::new();
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

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
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

        let _ = conn.start(0, None).await;

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    #[tokio::test]
    async fn test_start_cancelled_if_state_changed_during_auth() {
        let http_client = HttpClient::new();
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

        let mut inner = conn.inner.lock().await;
        *inner = ConnectionInner::Starting(999);
        drop(inner);

        let result = conn.start(0, None).await;
        assert!(matches!(result.unwrap_err(), StartError::AlreadyRunning));

        conn.stop().await;
    }

    #[tokio::test]
    async fn test_relay_loop_resets_state_on_exit() {
        let http_client = HttpClient::new();
        let (tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, tx);

        let (status_tx, status_rx) = tokio::sync::watch::channel(SocketStatus::Connected {});
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);

        let generation = 42;
        let conn_clone = Arc::clone(&conn);
        let relay_task = tokio::spawn(async move {
            conn_clone
                .relay_loop(status_rx, command_rx, generation)
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
}
