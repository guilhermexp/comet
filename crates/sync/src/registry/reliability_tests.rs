use super::*;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

struct Connector(Mutex<std::collections::VecDeque<TextPipe>>);
impl TextConnector for Connector {
    fn connect(&self) -> BoxFuture<'static, Result<TextPipe, SyncError>> {
        let pipe = lock(&self.0).pop_front();
        Box::pin(async move {
            match pipe {
                Some(pipe) => Ok(pipe),
                None => std::future::pending().await,
            }
        })
    }
}
struct HungConnector;
impl TextConnector for HungConnector {
    fn connect(&self) -> BoxFuture<'static, Result<TextPipe, SyncError>> {
        Box::pin(std::future::pending())
    }
}
struct PendingTransport(Arc<AtomicBool>);
impl RegistryTransport for PendingTransport {
    fn fetch(&self, _: u64) -> BoxFuture<'static, Result<String, SyncError>> {
        struct Flight(Arc<AtomicBool>);
        impl Drop for Flight {
            fn drop(&mut self) {
                self.0.store(false, SeqCst);
            }
        }
        let active = self.0.clone();
        Box::pin(async move {
            active.store(true, SeqCst);
            let _flight = Flight(active);
            std::future::pending().await
        })
    }
    fn push(&self, _: String) -> BoxFuture<'static, Result<String, SyncError>> {
        Box::pin(std::future::pending())
    }
}
fn pair(cap: usize) -> (TextPipe, TextPipe) {
    let (a, b) = mpsc::channel(cap);
    let (c, d) = mpsc::channel(cap);
    (TextPipe { tx: a, rx: d }, TextPipe { tx: c, rx: b })
}
fn connector(pipes: Vec<TextPipe>) -> Arc<dyn TextConnector> {
    Arc::new(Connector(Mutex::new(pipes.into())))
}
fn doc() -> Arc<Mutex<RegistryDoc>> {
    Arc::new(Mutex::new(RegistryDoc::new("device")))
}
async fn hello(server: &mut TextPipe) {
    let text = tokio::time::timeout(Duration::from_secs(1), server.rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap()["t"],
        "hello"
    );
}
async fn state(server: &TextPipe) {
    server
        .tx
        .send(r#"{"t":"state","seq":0,"full":false,"gcFloor":0,"rows":[],"presence":{}}"#.into())
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn shutdown_cancels_dial_and_joins_fallback() {
    let active = Arc::new(AtomicBool::new(false));
    let client = RegistryClient::connect_with_transport(
        Arc::new(HungConnector),
        doc(),
        "device",
        RegistryTuning::default(),
        Some(Arc::new(PendingTransport(active.clone()))),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !active.load(SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_millis(200), client.shutdown())
        .await
        .expect("shutdown must cancel dial");
    assert!(!active.load(SeqCst), "shutdown left HTTP fallback alive");
}
#[tokio::test(start_paused = true)]
async fn shutdown_interrupts_hello_and_backpressured_send() {
    for joined in [false, true] {
        let (pipe, mut server) = pair(1);
        let client = RegistryClient::connect_with_transport(
            connector(vec![pipe]),
            doc(),
            "device",
            RegistryTuning::default(),
            Some(Arc::new(PendingTransport(Arc::new(AtomicBool::new(false))))),
        )
        .await
        .unwrap();
        hello(&mut server).await;
        if joined {
            state(&server).await;
            // Fill the output channel, then block the actor on a second presence send.
            client.set_presence(1);
            tokio::time::timeout(Duration::from_secs(1), async {
                while server.rx.len() != 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            client.set_presence(2);
            tokio::time::timeout(Duration::from_secs(1), async {
                while client.presence_out.capacity() != 4 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(server.rx.len(), 1, "actor should be blocked on full output");
        }
        tokio::time::timeout(Duration::from_millis(200), client.shutdown())
            .await
            .expect("shutdown must interrupt hello / blocked send");
        assert!(server.tx.is_closed());
    }
}
#[tokio::test(start_paused = true)]
async fn cancelling_construction_closes_actor_pipe() {
    let (pipe, mut server) = pair(4);
    let connecting = tokio::spawn(RegistryClient::connect_with_transport(
        connector(vec![pipe]),
        doc(),
        "device",
        RegistryTuning::default(),
        None,
    ));
    hello(&mut server).await;
    connecting.abort();
    let _ = connecting.await;
    tokio::time::timeout(Duration::from_secs(1), server.tx.closed())
        .await
        .expect("cancelled construction detached actor");
}
#[tokio::test(start_paused = true)]
async fn cancelling_shutdown_retains_abort_ownership() {
    let mut client = RegistryClient::connect_with_transport(
        Arc::new(HungConnector),
        doc(),
        "device",
        RegistryTuning::default(),
        Some(Arc::new(PendingTransport(Arc::new(AtomicBool::new(false))))),
    )
    .await
    .unwrap();
    let task = client.task.take().unwrap();
    task.abort();
    let _ = task.await;
    client.task = Some(tokio::spawn(std::future::pending()));
    let abort = client.task.as_ref().unwrap().abort_handle();
    let mut closing = Box::pin(client.shutdown());
    assert!(futures::poll!(&mut closing).is_pending());
    drop(closing);
    tokio::task::yield_now().await;
    assert!(abort.is_finished(), "cancelled shutdown detached actor");
}

#[tokio::test(start_paused = true)]
async fn system_wake_replaces_silent_connection_and_preserves_readiness() {
    for pre_ready in [false, true] {
        let (first, mut old) = pair(4);
        let (second, mut fresh) = pair(4);
        let (events, _) = broadcast::channel(16);
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (_nudge, nudge_rx) = mpsc::channel(1);
        let (_probe, probe_rx) = mpsc::channel(1);
        let (_redial, redial_rx) = mpsc::channel(1);
        let (_presence, presence_rx) = mpsc::channel(4);
        let queued = doc();
        lock(&queued)
            .upsert_device(&zeron_proto::Device {
                id: "device".into(),
                name: "Device".into(),
                platform: "macos".into(),
                last_seen_at: None,
                created_at: None,
                version: None,
                cursor_sdk_version: None,
                capabilities: Vec::new(),
            })
            .unwrap();
        let actor = Actor {
            doc: queued.clone(),
            device_id: "device".into(),
            connector: connector(vec![first, second]),
            tuning: RegistryTuning::default(),
            events,
            shutdown,
            nudge_rx,
            probe_rx,
            redial_rx,
            presence_rx,
            presence: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Stats::default()),
            transport: None,
            offline_task: Arc::new(Mutex::new(None)),
            sync_busy: Arc::new(AtomicBool::new(false)),
        };
        let (ready_tx, mut ready_rx) = oneshot::channel();
        let (wake, wake_rx) = broadcast::channel(4);
        let (_online, online_rx) = broadcast::channel(4);
        let task = tokio::spawn(actor.run_with_signals(ready_tx, wake_rx, online_rx));
        hello(&mut old).await;
        let old_batch = if !pre_ready {
            state(&old).await;
            (&mut ready_rx).await.unwrap().unwrap();
            let push = old.rx.recv().await.unwrap();
            Some(serde_json::from_str::<serde_json::Value>(&push).unwrap()["batch"].clone())
        } else {
            None
        };
        wake.send(()).unwrap();
        hello(&mut fresh).await;
        assert!(old.tx.is_closed(), "old connection survived system wake");
        state(&fresh).await;
        if pre_ready {
            ready_rx.await.unwrap().unwrap();
        }
        let push = fresh.rx.recv().await.unwrap();
        let push: serde_json::Value = serde_json::from_str(&push).unwrap();
        assert_eq!(push["t"], "push");
        if let Some(batch) = old_batch {
            assert_eq!(push["batch"], batch, "wake changed pending batch identity");
        }
        assert_eq!(lock(&queued).pending_len(), 1);
        fresh
            .tx
            .send(
                serde_json::json!({"t":"ack","batch":push["batch"],"seq":1,"applied":1})
                    .to_string(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while lock(&queued).pending_len() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        shutdown_tx.send(true).unwrap();
        task.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn system_wake_cancels_pending_dial_and_preserves_readiness() {
    struct PendingFirst {
        pipe: Mutex<Option<TextPipe>>,
        attempts: std::sync::atomic::AtomicUsize,
        active: Arc<AtomicBool>,
    }
    impl TextConnector for PendingFirst {
        fn connect(&self) -> BoxFuture<'static, Result<TextPipe, SyncError>> {
            if self.attempts.fetch_add(1, SeqCst) == 0 {
                struct Flight(Arc<AtomicBool>);
                impl Drop for Flight {
                    fn drop(&mut self) {
                        self.0.store(false, SeqCst);
                    }
                }
                let active = self.active.clone();
                Box::pin(async move {
                    active.store(true, SeqCst);
                    let _guard = Flight(active);
                    std::future::pending().await
                })
            } else {
                let pipe = lock(&self.pipe).take().unwrap();
                Box::pin(async move { Ok(pipe) })
            }
        }
    }
    let (pipe, mut fresh) = pair(4);
    let active = Arc::new(AtomicBool::new(false));
    let (events, _) = broadcast::channel(16);
    let (shutdown_tx, shutdown) = watch::channel(false);
    let (_nudge, nudge_rx) = mpsc::channel(1);
    let (_probe, probe_rx) = mpsc::channel(1);
    let (_redial, redial_rx) = mpsc::channel(1);
    let (_presence, presence_rx) = mpsc::channel(4);
    let actor = Actor {
        doc: doc(),
        device_id: "device".into(),
        connector: Arc::new(PendingFirst {
            pipe: Mutex::new(Some(pipe)),
            attempts: std::sync::atomic::AtomicUsize::new(0),
            active: active.clone(),
        }),
        tuning: RegistryTuning::default(),
        events,
        shutdown,
        nudge_rx,
        probe_rx,
        redial_rx,
        presence_rx,
        presence: Arc::new(Mutex::new(HashMap::new())),
        stats: Arc::new(Stats::default()),
        transport: None,
        offline_task: Arc::new(Mutex::new(None)),
        sync_busy: Arc::new(AtomicBool::new(false)),
    };
    let (ready_tx, ready_rx) = oneshot::channel();
    let (wake, wake_rx) = broadcast::channel(4);
    let (_online, online_rx) = broadcast::channel(4);
    let task = tokio::spawn(actor.run_with_signals(ready_tx, wake_rx, online_rx));
    tokio::time::timeout(Duration::from_secs(1), async {
        while !active.load(SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    wake.send(()).unwrap();
    hello(&mut fresh).await;
    assert!(!active.load(SeqCst));
    state(&fresh).await;
    ready_rx.await.unwrap().unwrap();
    shutdown_tx.send(true).unwrap();
    task.await.unwrap();
}
