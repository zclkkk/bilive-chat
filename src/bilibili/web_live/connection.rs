use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use super::auth::{self, AuthError, WebLiveAuth};
use super::commands;
use super::http::HttpClient;
use super::socket::{self, SocketHandle, SocketStatus};
use crate::chat::filter::ChatFilter;
use crate::config::FilterOptions;
use crate::overlay::event::{OverlayEvent, PanelEvent};

const CHANNEL_SIZE: usize = 32;

pub struct LiveConnection {
    cmd_tx: mpsc::Sender<LiveCommand>,
}

enum LiveCommand {
    Start {
        room_id: u64,
        cookie: Option<String>,
        reply: oneshot::Sender<Result<(), StartError>>,
    },
    Stop {
        reply: oneshot::Sender<bool>,
    },
    Status {
        reply: oneshot::Sender<SocketStatus>,
    },
}

enum RuntimeEvent {
    AuthFinished {
        session: u64,
        result: Result<WebLiveAuth, AuthError>,
    },
    RelayExited {
        session: u64,
    },
    SocketStatusChanged {
        session: u64,
        status: SocketStatus,
    },
}

enum RuntimeState {
    Idle,
    Starting {
        session: u64,
        auth_task: tokio::task::JoinHandle<()>,
        start_reply: oneshot::Sender<Result<(), StartError>>,
    },
    Active {
        session: u64,
        socket_handle: SocketHandle,
        relay_task: tokio::task::JoinHandle<()>,
        status_task: tokio::task::JoinHandle<()>,
        latest_status: SocketStatus,
    },
}

struct LiveRuntime {
    state: RuntimeState,
    http_client: HttpClient,
    panel_tx: broadcast::Sender<PanelEvent>,
    overlay_tx: broadcast::Sender<OverlayEvent>,
    filter_rx: watch::Receiver<FilterOptions>,
    cmd_rx: mpsc::Receiver<LiveCommand>,
    event_rx: mpsc::Receiver<RuntimeEvent>,
    event_tx: mpsc::Sender<RuntimeEvent>,
    next_session: u64,
}

impl LiveConnection {
    pub fn new(
        http_client: HttpClient,
        panel_tx: broadcast::Sender<PanelEvent>,
        overlay_tx: broadcast::Sender<OverlayEvent>,
        filter_rx: watch::Receiver<FilterOptions>,
    ) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel(CHANNEL_SIZE);
        let (event_tx, event_rx) = mpsc::channel(CHANNEL_SIZE);

        let runtime = LiveRuntime {
            state: RuntimeState::Idle,
            http_client,
            panel_tx,
            overlay_tx,
            filter_rx,
            cmd_rx,
            event_rx,
            event_tx,
            next_session: 0,
        };

        tokio::spawn(runtime.run());

        Arc::new(Self { cmd_tx })
    }

    pub async fn start(&self, room_id: u64, cookie: Option<String>) -> Result<(), StartError> {
        let (reply, rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(LiveCommand::Start {
                room_id,
                cookie,
                reply,
            })
            .await
            .is_err()
        {
            panic!("live runtime task exited before start command");
        }
        rx.await
            .expect("live runtime dropped start reply before responding")
    }

    pub async fn stop(&self) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.cmd_tx.send(LiveCommand::Stop { reply }).await.is_err() {
            panic!("live runtime task exited before stop command");
        }
        rx.await
            .expect("live runtime dropped stop reply before responding")
    }

    pub async fn status(&self) -> SocketStatus {
        let (reply, rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(LiveCommand::Status { reply })
            .await
            .is_err()
        {
            panic!("live runtime task exited before status command");
        }
        rx.await
            .expect("live runtime dropped status reply before responding")
    }
}

impl LiveRuntime {
    async fn run(mut self) {
        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => {
                    match cmd {
                        Some(cmd) => self.handle_command(cmd),
                        None => break,
                    }
                }
                event = self.event_rx.recv() => {
                    match event {
                        Some(event) => self.handle_event(event).await,
                        None => break,
                    }
                }
            }
        }
        self.shutdown();
    }

    fn handle_command(&mut self, cmd: LiveCommand) {
        match cmd {
            LiveCommand::Start {
                room_id,
                cookie,
                reply,
            } => {
                tracing::info!("start requested for room {room_id}");
                match &self.state {
                    RuntimeState::Idle => {
                        self.next_session += 1;
                        let session = self.next_session;

                        let api = auth::LiveBiliApi::new(self.http_client.clone());
                        let wts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let event_tx = self.event_tx.clone();
                        let auth_task = tokio::spawn(async move {
                            let result = auth::prepare(&api, room_id, cookie.as_deref(), wts).await;
                            let _ = event_tx
                                .send(RuntimeEvent::AuthFinished { session, result })
                                .await;
                        });

                        let _ = self.panel_tx.send(PanelEvent::Status {
                            status: SocketStatus::Connecting {},
                        });

                        self.state = RuntimeState::Starting {
                            session,
                            auth_task,
                            start_reply: reply,
                        };
                    }
                    _ => {
                        let _ = reply.send(Err(StartError::AlreadyRunning));
                    }
                }
            }
            LiveCommand::Stop { reply } => {
                let old = std::mem::replace(&mut self.state, RuntimeState::Idle);
                match old {
                    RuntimeState::Idle => {
                        self.state = RuntimeState::Idle;
                        let _ = reply.send(false);
                    }
                    RuntimeState::Starting {
                        auth_task,
                        start_reply,
                        ..
                    } => {
                        Self::cancel_starting(auth_task, start_reply);
                        let _ = self.panel_tx.send(PanelEvent::Status {
                            status: SocketStatus::Disconnected { error: None },
                        });
                        let _ = reply.send(true);
                    }
                    RuntimeState::Active {
                        socket_handle,
                        relay_task,
                        status_task,
                        ..
                    } => {
                        tracing::info!("stopping connection");
                        Self::stop_active(socket_handle, relay_task, status_task);
                        let _ = self.panel_tx.send(PanelEvent::Status {
                            status: SocketStatus::Disconnected { error: None },
                        });
                        let _ = reply.send(true);
                    }
                }
            }
            LiveCommand::Status { reply } => {
                let status = match &self.state {
                    RuntimeState::Idle => SocketStatus::Disconnected { error: None },
                    RuntimeState::Starting { .. } => SocketStatus::Connecting {},
                    RuntimeState::Active { latest_status, .. } => latest_status.clone(),
                };
                let _ = reply.send(status);
            }
        }
    }

    async fn handle_event(&mut self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::AuthFinished { session, result } => {
                let old = std::mem::replace(&mut self.state, RuntimeState::Idle);
                match old {
                    RuntimeState::Starting {
                        session: s,
                        auth_task: _,
                        start_reply,
                    } if s == session => match result {
                        Ok(web_auth) => {
                            let (handle, command_rx) = socket::connect(web_auth);
                            let relay_task = tokio::spawn(relay_loop(
                                command_rx,
                                self.overlay_tx.clone(),
                                self.filter_rx.clone(),
                                self.event_tx.clone(),
                                session,
                            ));
                            let status_rx = handle.status_rx.clone();
                            let status_task = tokio::spawn(status_watcher(
                                status_rx,
                                self.event_tx.clone(),
                                session,
                            ));
                            self.state = RuntimeState::Active {
                                session,
                                socket_handle: handle,
                                relay_task,
                                status_task,
                                latest_status: SocketStatus::Connecting {},
                            };
                            let _ = start_reply.send(Ok(()));
                        }
                        Err(e) => {
                            let err = match e {
                                AuthError::CookieNotLoggedIn => StartError::CookieNotLoggedIn,
                                other => StartError::Auth(other),
                            };
                            let _ = start_reply.send(Err(err));
                        }
                    },
                    other => {
                        self.state = other;
                    }
                }
            }
            RuntimeEvent::RelayExited { session } => {
                let old = std::mem::replace(&mut self.state, RuntimeState::Idle);
                match old {
                    RuntimeState::Active {
                        session: s,
                        socket_handle,
                        relay_task,
                        status_task,
                        ..
                    } if s == session => {
                        tracing::info!("relay loop exited, resetting to idle");
                        Self::stop_active(socket_handle, relay_task, status_task);
                    }
                    other => {
                        self.state = other;
                    }
                }
            }
            RuntimeEvent::SocketStatusChanged { session, status } => match &mut self.state {
                RuntimeState::Active {
                    session: s,
                    latest_status,
                    ..
                } if *s == session => {
                    *latest_status = status.clone();
                    let _ = self.panel_tx.send(PanelEvent::Status { status });
                }
                _ => {}
            },
        }
    }

    fn shutdown(&mut self) {
        let old = std::mem::replace(&mut self.state, RuntimeState::Idle);
        match old {
            RuntimeState::Idle => {}
            RuntimeState::Starting {
                auth_task,
                start_reply,
                ..
            } => {
                Self::cancel_starting(auth_task, start_reply);
            }
            RuntimeState::Active {
                socket_handle,
                relay_task,
                status_task,
                ..
            } => {
                Self::stop_active(socket_handle, relay_task, status_task);
            }
        }
    }

    fn cancel_starting(
        auth_task: tokio::task::JoinHandle<()>,
        start_reply: oneshot::Sender<Result<(), StartError>>,
    ) {
        auth_task.abort();
        let _ = start_reply.send(Err(StartError::Cancelled));
    }

    fn stop_active(
        socket_handle: SocketHandle,
        relay_task: tokio::task::JoinHandle<()>,
        status_task: tokio::task::JoinHandle<()>,
    ) {
        socket_handle.stop();
        relay_task.abort();
        status_task.abort();
    }
}

async fn relay_loop(
    mut command_rx: mpsc::Receiver<serde_json::Value>,
    overlay_tx: broadcast::Sender<OverlayEvent>,
    mut filter_rx: watch::Receiver<FilterOptions>,
    event_tx: mpsc::Sender<RuntimeEvent>,
    session: u64,
) {
    let mut filter = ChatFilter::new(&filter_rx.borrow());

    loop {
        tokio::select! {
            biased;
            cmd = command_rx.recv() => {
                match cmd {
                    Some(value) => {
                        if let Some(event) = commands::parse_command(&value) {
                            if !filter.should_block(&event) {
                                let _ = overlay_tx.send(OverlayEvent::from(&event));
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

    let _ = event_tx.send(RuntimeEvent::RelayExited { session }).await;
}

async fn status_watcher(
    mut status_rx: watch::Receiver<SocketStatus>,
    event_tx: mpsc::Sender<RuntimeEvent>,
    session: u64,
) {
    while status_rx.changed().await.is_ok() {
        let status = status_rx.borrow().clone();
        if event_tx
            .send(RuntimeEvent::SocketStatusChanged { session, status })
            .await
            .is_err()
        {
            break;
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

    fn test_filter_rx() -> watch::Receiver<FilterOptions> {
        let (_, rx) = watch::channel(FilterOptions::default());
        rx
    }

    fn test_socket_handle() -> SocketHandle {
        let (_status_tx, status_rx) = watch::channel(SocketStatus::Disconnected { error: None });
        test_socket_handle_with_status(status_rx)
    }

    fn test_socket_handle_with_status(status_rx: watch::Receiver<SocketStatus>) -> SocketHandle {
        let cancel = tokio_util::sync::CancellationToken::new();
        let task = tokio::spawn(async {});
        SocketHandle {
            status_rx,
            cancel,
            abort_handle: task.abort_handle(),
        }
    }

    fn test_runtime() -> (
        LiveRuntime,
        mpsc::Sender<LiveCommand>,
        mpsc::Sender<RuntimeEvent>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(16);
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let (_, filter_rx) = watch::channel(FilterOptions::default());

        let runtime = LiveRuntime {
            state: RuntimeState::Idle,
            http_client: HttpClient::new(),
            panel_tx,
            overlay_tx,
            filter_rx,
            cmd_rx,
            event_rx,
            event_tx: event_tx.clone(),
            next_session: 0,
        };

        (runtime, cmd_tx, event_tx)
    }

    struct DropFlag(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn pending_task_with_flags(
        dropped: Arc<std::sync::atomic::AtomicBool>,
    ) -> (
        tokio::task::JoinHandle<()>,
        Arc<std::sync::atomic::AtomicBool>,
    ) {
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started_clone = started.clone();
        let task = tokio::spawn(async move {
            started_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            let _guard = DropFlag(dropped);
            std::future::pending::<()>().await;
        });
        (task, started)
    }

    async fn wait_for_flag(flag: &std::sync::atomic::AtomicBool) {
        for _ in 0..20 {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        panic!("task did not start");
    }

    // Public API tests

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
    async fn test_auth_failure_returns_to_idle() {
        let http_client = HttpClient::new();
        let (panel_tx, _) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let conn = LiveConnection::new(http_client, panel_tx, overlay_tx, test_filter_rx());

        let _ = conn.start(0, None).await;

        let status = conn.status().await;
        assert!(matches!(status, SocketStatus::Disconnected { error: None }));
    }

    // Runtime edge tests

    #[tokio::test]
    async fn test_runtime_start_rejects_already_starting() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, _start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Start {
            room_id: 12345,
            cookie: None,
            reply,
        });

        assert!(matches!(
            reply_rx.await.unwrap().unwrap_err(),
            StartError::AlreadyRunning
        ));
    }

    #[tokio::test]
    async fn test_runtime_start_rejects_already_active() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let handle = test_socket_handle();
        rt.state = RuntimeState::Active {
            session: 1,
            socket_handle: handle,
            relay_task: tokio::spawn(async {}),
            status_task: tokio::spawn(async {}),
            latest_status: SocketStatus::Connected {},
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Start {
            room_id: 12345,
            cookie: None,
            reply,
        });

        assert!(matches!(
            reply_rx.await.unwrap().unwrap_err(),
            StartError::AlreadyRunning
        ));
    }

    #[tokio::test]
    async fn test_runtime_status_connecting_while_starting() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, _start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Status { reply });

        let status = reply_rx.await.unwrap();
        assert!(matches!(status, SocketStatus::Connecting {}));
    }

    #[tokio::test]
    async fn test_runtime_stop_returns_true_when_starting() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, _start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Stop { reply });

        assert!(reply_rx.await.unwrap());
    }

    #[tokio::test]
    async fn test_runtime_stop_during_starting_cancels_pending_start() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Stop { reply });

        assert!(reply_rx.await.unwrap());
        assert!(matches!(
            start_rx.await.unwrap().unwrap_err(),
            StartError::Cancelled
        ));
        assert!(matches!(rt.state, RuntimeState::Idle));
    }

    #[tokio::test]
    async fn test_runtime_stop_during_starting_broadcasts_disconnected() {
        let (panel_tx, mut panel_rx) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let (_cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(16);
        let (_, filter_rx) = watch::channel(FilterOptions::default());
        let (start_reply, _start_rx) = oneshot::channel();

        let mut rt = LiveRuntime {
            state: RuntimeState::Starting {
                session: 1,
                auth_task: tokio::spawn(async {}),
                start_reply,
            },
            http_client: HttpClient::new(),
            panel_tx,
            overlay_tx,
            filter_rx,
            cmd_rx,
            event_rx,
            event_tx,
            next_session: 0,
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Stop { reply });

        assert!(reply_rx.await.unwrap());

        let received = panel_rx.try_recv().unwrap();
        match received {
            PanelEvent::Status { status } => {
                assert!(matches!(status, SocketStatus::Disconnected { error: None }));
            }
        }
    }

    #[tokio::test]
    async fn test_runtime_stop_returns_true_when_active() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let handle = test_socket_handle();
        rt.state = RuntimeState::Active {
            session: 1,
            socket_handle: handle,
            relay_task: tokio::spawn(async {}),
            status_task: tokio::spawn(async {}),
            latest_status: SocketStatus::Connected {},
        };

        let (reply, reply_rx) = oneshot::channel();
        rt.handle_command(LiveCommand::Stop { reply });

        assert!(reply_rx.await.unwrap());
        assert!(matches!(rt.state, RuntimeState::Idle));
    }

    #[tokio::test]
    async fn test_runtime_stale_auth_ignored() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, _start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 5,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        rt.handle_event(RuntimeEvent::AuthFinished {
            session: 99,
            result: Ok(WebLiveAuth {
                uid: Some(1),
                room_id: 12345,
                key: "key".into(),
                buvid3: "b3".into(),
                urls: vec!["wss://example.com:443/sub".into()],
            }),
        })
        .await;

        assert!(matches!(
            rt.state,
            RuntimeState::Starting { session: 5, .. }
        ));
    }

    #[tokio::test]
    async fn test_runtime_auth_failure_idle() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        rt.handle_event(RuntimeEvent::AuthFinished {
            session: 1,
            result: Err(AuthError::InvalidOutput("test".into())),
        })
        .await;

        assert!(matches!(rt.state, RuntimeState::Idle));
        assert!(matches!(
            start_rx.await.unwrap().unwrap_err(),
            StartError::Auth(AuthError::InvalidOutput(_))
        ));
    }

    #[tokio::test]
    async fn test_runtime_auth_cookie_not_logged_in() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let (start_reply, start_rx) = oneshot::channel();
        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task: tokio::spawn(async {}),
            start_reply,
        };

        rt.handle_event(RuntimeEvent::AuthFinished {
            session: 1,
            result: Err(AuthError::CookieNotLoggedIn),
        })
        .await;

        assert!(matches!(rt.state, RuntimeState::Idle));
        assert!(matches!(
            start_rx.await.unwrap().unwrap_err(),
            StartError::CookieNotLoggedIn
        ));
    }

    #[tokio::test]
    async fn test_runtime_relay_exit_returns_to_idle() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let handle = test_socket_handle();
        rt.state = RuntimeState::Active {
            session: 7,
            socket_handle: handle,
            relay_task: tokio::spawn(async {}),
            status_task: tokio::spawn(async {}),
            latest_status: SocketStatus::Connected {},
        };

        rt.handle_event(RuntimeEvent::RelayExited { session: 7 })
            .await;

        assert!(matches!(rt.state, RuntimeState::Idle));
    }

    #[tokio::test]
    async fn test_runtime_stale_relay_exit_ignored() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let handle = test_socket_handle();
        rt.state = RuntimeState::Active {
            session: 7,
            socket_handle: handle,
            relay_task: tokio::spawn(async {}),
            status_task: tokio::spawn(async {}),
            latest_status: SocketStatus::Connected {},
        };

        rt.handle_event(RuntimeEvent::RelayExited { session: 99 })
            .await;

        assert!(matches!(rt.state, RuntimeState::Active { session: 7, .. }));
    }

    #[tokio::test]
    async fn test_runtime_relay_exit_does_not_overwrite_socket_error() {
        let (panel_tx, mut panel_rx) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(16);
        let (_, filter_rx) = watch::channel(FilterOptions::default());

        let mut rt = LiveRuntime {
            state: RuntimeState::Active {
                session: 3,
                socket_handle: test_socket_handle(),
                relay_task: tokio::spawn(async {}),
                status_task: tokio::spawn(async {}),
                latest_status: SocketStatus::Connected {},
            },
            http_client: HttpClient::new(),
            panel_tx,
            overlay_tx,
            filter_rx,
            cmd_rx,
            event_rx,
            event_tx: event_tx.clone(),
            next_session: 0,
        };

        drop(cmd_tx);
        drop(event_tx);

        rt.handle_event(RuntimeEvent::SocketStatusChanged {
            session: 3,
            status: SocketStatus::Disconnected {
                error: Some("read error".into()),
            },
        })
        .await;

        let received = panel_rx.try_recv().unwrap();
        match received {
            PanelEvent::Status { ref status } => {
                assert!(matches!(
                    status,
                    SocketStatus::Disconnected {
                        error: Some(e),
                    } if e == "read error"
                ));
            }
        }

        rt.handle_event(RuntimeEvent::RelayExited { session: 3 })
            .await;

        assert!(matches!(rt.state, RuntimeState::Idle));

        assert!(
            panel_rx.try_recv().is_err(),
            "relay exit should not broadcast a clean disconnected status"
        );
    }

    #[tokio::test]
    async fn test_runtime_shutdown_cleans_up_starting() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (auth_task, auth_started) = pending_task_with_flags(dropped.clone());
        let (start_reply, start_rx) = oneshot::channel();

        rt.state = RuntimeState::Starting {
            session: 1,
            auth_task,
            start_reply,
        };

        wait_for_flag(&auth_started).await;

        rt.shutdown();

        assert!(matches!(rt.state, RuntimeState::Idle));
        assert!(matches!(
            start_rx.await.unwrap().unwrap_err(),
            StartError::Cancelled
        ));

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(dropped.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_runtime_shutdown_cleans_up_active() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();
        let (_status_tx, status_rx) = watch::channel(SocketStatus::Connected {});
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_probe = cancel.clone();
        let socket_task = tokio::spawn(async {});
        let relay_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let status_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (relay_task, relay_started) = pending_task_with_flags(relay_dropped.clone());
        let (status_task, status_started) = pending_task_with_flags(status_dropped.clone());

        rt.state = RuntimeState::Active {
            session: 1,
            socket_handle: SocketHandle {
                status_rx,
                cancel,
                abort_handle: socket_task.abort_handle(),
            },
            relay_task,
            status_task,
            latest_status: SocketStatus::Connected {},
        };

        wait_for_flag(&relay_started).await;
        wait_for_flag(&status_started).await;

        rt.shutdown();

        assert!(matches!(rt.state, RuntimeState::Idle));
        assert!(cancel_probe.is_cancelled());

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(relay_dropped.load(std::sync::atomic::Ordering::Relaxed));
        assert!(status_dropped.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_runtime_socket_status_updates_cached_status() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let handle = test_socket_handle();
        rt.state = RuntimeState::Active {
            session: 3,
            socket_handle: handle,
            relay_task: tokio::spawn(async {}),
            status_task: tokio::spawn(async {}),
            latest_status: SocketStatus::Connecting {},
        };

        rt.handle_event(RuntimeEvent::SocketStatusChanged {
            session: 3,
            status: SocketStatus::Connected {},
        })
        .await;

        match &rt.state {
            RuntimeState::Active { latest_status, .. } => {
                assert!(matches!(latest_status, SocketStatus::Connected {}));
            }
            _ => panic!("expected Active state"),
        }
    }

    #[tokio::test]
    async fn test_runtime_socket_status_broadcasts_panel_event() {
        let (panel_tx, mut panel_rx) = broadcast::channel(16);
        let (overlay_tx, _) = broadcast::channel(16);
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(16);
        let (_, filter_rx) = watch::channel(FilterOptions::default());

        let mut rt = LiveRuntime {
            state: RuntimeState::Active {
                session: 3,
                socket_handle: test_socket_handle(),
                relay_task: tokio::spawn(async {}),
                status_task: tokio::spawn(async {}),
                latest_status: SocketStatus::Connecting {},
            },
            http_client: HttpClient::new(),
            panel_tx,
            overlay_tx,
            filter_rx,
            cmd_rx,
            event_rx,
            event_tx: event_tx.clone(),
            next_session: 0,
        };

        drop(cmd_tx);
        drop(event_tx);

        rt.handle_event(RuntimeEvent::SocketStatusChanged {
            session: 3,
            status: SocketStatus::Connected {},
        })
        .await;

        let received = panel_rx.try_recv().unwrap();
        match received {
            PanelEvent::Status { status } => {
                assert!(matches!(status, SocketStatus::Connected {}));
            }
        }
    }

    #[tokio::test]
    async fn test_runtime_stale_socket_status_ignored() {
        let (mut rt, _cmd_tx, _event_tx) = test_runtime();

        let handle = test_socket_handle();
        rt.state = RuntimeState::Active {
            session: 3,
            socket_handle: handle,
            relay_task: tokio::spawn(async {}),
            status_task: tokio::spawn(async {}),
            latest_status: SocketStatus::Connecting {},
        };

        rt.handle_event(RuntimeEvent::SocketStatusChanged {
            session: 99,
            status: SocketStatus::Disconnected {
                error: Some("stale".into()),
            },
        })
        .await;

        match &rt.state {
            RuntimeState::Active { latest_status, .. } => {
                assert!(matches!(latest_status, SocketStatus::Connecting {}));
            }
            _ => panic!("expected Active state"),
        }
    }

    #[tokio::test]
    async fn test_socket_handle_stop_cancels_and_aborts() {
        let handle = test_socket_handle();

        assert!(!handle.is_cancelled());

        handle.stop();

        assert!(handle.is_cancelled());
    }

    #[tokio::test]
    async fn test_socket_handle_stop_aborts_spawned_task() {
        let (_status_tx, status_rx) = watch::channel(SocketStatus::Connected {});
        let cancel = tokio_util::sync::CancellationToken::new();
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started_clone = started.clone();
        let task = tokio::spawn(async move {
            started_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            std::future::pending::<()>().await;
        });
        let handle = SocketHandle {
            status_rx,
            cancel,
            abort_handle: task.abort_handle(),
        };

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(started.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!task.is_finished());

        handle.stop();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(task.is_finished());
    }

    // Relay loop tests

    #[tokio::test]
    async fn test_relay_loop_broadcasts_parsed_command_to_overlay() {
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let (_filter_tx, filter_rx) = watch::channel(FilterOptions::default());
        let (event_tx, _event_rx) = mpsc::channel(16);

        let relay_task = tokio::spawn(relay_loop(command_rx, overlay_tx, filter_rx, event_tx, 100));

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

        relay_task.abort();
    }

    #[tokio::test]
    async fn test_relay_loop_skips_unknown_command() {
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let (_filter_tx, filter_rx) = watch::channel(FilterOptions::default());
        let (event_tx, _event_rx) = mpsc::channel(16);

        let relay_task = tokio::spawn(relay_loop(command_rx, overlay_tx, filter_rx, event_tx, 101));

        let unknown = serde_json::json!({"cmd": "ROOM_CHANGE", "data": {"title": "new"}});
        command_tx.send(unknown).await.unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "unknown command should not produce overlay event"
        );

        drop(command_tx);
        relay_task.abort();
    }

    #[tokio::test]
    async fn test_relay_loop_blocks_filtered_user() {
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let (filter_tx, filter_rx) = watch::channel(FilterOptions {
            blocked_users: vec!["BlockedUser".into()],
            blocked_keywords: vec![],
        });
        let (event_tx, _event_rx) = mpsc::channel(16);

        let relay_task = tokio::spawn(relay_loop(command_rx, overlay_tx, filter_rx, event_tx, 200));

        let danmu = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "should be blocked",
                [99, "BlockedUser", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu).await.unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "filtered user should not produce overlay event"
        );

        drop(command_tx);
        drop(filter_tx);
        relay_task.abort();
    }

    #[tokio::test]
    async fn test_relay_loop_blocks_filtered_keyword() {
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let (filter_tx, filter_rx) = watch::channel(FilterOptions {
            blocked_users: vec![],
            blocked_keywords: vec!["forbidden".into()],
        });
        let (event_tx, _event_rx) = mpsc::channel(16);

        let relay_task = tokio::spawn(relay_loop(command_rx, overlay_tx, filter_rx, event_tx, 201));

        let danmu = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "this is forbidden text",
                [88, "NormalUser", 0, 0, 0, 0, 1, ""]
            ]
        });
        command_tx.send(danmu).await.unwrap();

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "filtered keyword should not produce overlay event"
        );

        drop(command_tx);
        drop(filter_tx);
        relay_task.abort();
    }

    #[tokio::test]
    async fn test_filter_update_takes_effect_without_restart() {
        let (overlay_tx, mut overlay_rx) = broadcast::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let (filter_tx, filter_rx) = watch::channel(FilterOptions::default());
        let (event_tx, _event_rx) = mpsc::channel(16);

        let relay_task = tokio::spawn(relay_loop(command_rx, overlay_tx, filter_rx, event_tx, 300));

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

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), overlay_rx.recv()).await;

        assert!(
            result.is_err(),
            "event from blocked user should not reach overlay after filter update"
        );

        drop(command_tx);
        drop(filter_tx);
        relay_task.abort();
    }

    #[tokio::test]
    async fn test_relay_loop_sends_relay_exited_on_natural_exit() {
        let (overlay_tx, _) = broadcast::channel(16);
        let (command_tx, command_rx) = mpsc::channel(16);
        let (_filter_tx, filter_rx) = watch::channel(FilterOptions::default());
        let (event_tx, mut event_rx) = mpsc::channel(16);

        let relay_task = tokio::spawn(relay_loop(command_rx, overlay_tx, filter_rx, event_tx, 400));

        drop(command_tx);

        let event = tokio::time::timeout(std::time::Duration::from_millis(200), event_rx.recv())
            .await
            .expect("timeout")
            .expect("event channel closed");

        match event {
            RuntimeEvent::RelayExited { session } => {
                assert_eq!(session, 400);
            }
            _ => panic!("expected RelayExited"),
        }

        relay_task.abort();
    }

    #[tokio::test]
    async fn test_status_watcher_sends_status_changed() {
        let (status_tx, status_rx) = watch::channel(SocketStatus::Connecting {});
        let (event_tx, mut event_rx) = mpsc::channel(16);

        let watcher_task = tokio::spawn(status_watcher(status_rx, event_tx, 99));

        let _ = status_tx.send(SocketStatus::Connected {});

        let event = tokio::time::timeout(std::time::Duration::from_millis(200), event_rx.recv())
            .await
            .expect("timeout")
            .expect("event channel closed");

        match event {
            RuntimeEvent::SocketStatusChanged { session, status } => {
                assert_eq!(session, 99);
                assert!(matches!(status, SocketStatus::Connected {}));
            }
            _ => panic!("expected SocketStatusChanged"),
        }

        watcher_task.abort();
    }
}
