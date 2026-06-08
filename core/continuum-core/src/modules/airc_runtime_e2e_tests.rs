//! Runtime proof that Continuum's AIRC module uses typed daemon IPC for
//! realtime publish, attach, and replay. The harness intentionally speaks
//! `airc_ipc` frames directly so the test cannot pass through CLI subprocesses
//! or stdout parsing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use airc_core::{
    ClientId, EventId, PeerId, RoomId, TranscriptCursor, TranscriptEvent, TranscriptKind,
};
use airc_ipc::codec::{read_frame, write_frame};
use airc_ipc::transport::{IpcListener, IpcStream};
use airc_ipc::{
    InboxRequest, InboxResponse, PublishRequest, PublishResponse, Request, ResolveWireResponse,
    Response,
};
use airc_protocol::FrameKind;
use parking_lot::Mutex;
use serde_json::json;
use uuid::Uuid;

use crate::airc::{
    default_socket_path_in, AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef,
    AircRealtimeSchema,
};
use crate::modules::airc::AircModule;
use crate::runtime::{
    CommandResult, MessageBus, ModuleContext, ModuleRegistry, ServiceModule, SharedCompute,
};

const TEST_ROOM_ID: Uuid = Uuid::from_u128(0xA1);
const TEST_AIRC_EVENT_ID: EventId = EventId(Uuid::from_u128(0xB1));

#[tokio::test]
async fn runtime_publish_attach_and_replay_use_daemon_ipc_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let airc_home = temp_dir.path().join(".airc");
    std::fs::create_dir_all(&airc_home).unwrap();

    let daemon = TestAircDaemon::start(&airc_home).await;
    let bus = Arc::new(MessageBus::new());
    let mut receiver = bus.receiver();
    let ctx = ModuleContext::new(
        Arc::new(ModuleRegistry::new()),
        bus,
        Arc::new(SharedCompute::new()),
        tokio::runtime::Handle::current(),
    );
    let module = AircModule::with_daemon_home(&airc_home);
    module.initialize(&ctx).await.unwrap();
    daemon.wait_for_attach().await;

    let envelope = AircRealtimeEnvelope::new(
        "continuum-runtime-e2e".to_string(),
        TEST_ROOM_ID,
        "continuum-runtime-test".to_string(),
        1_000,
        AircRealtimePayload::ExistingSchema {
            payload: AircRealtimePayloadRef::inline(
                AircRealtimeSchema::EventBridgePayload,
                json!({
                    "eventName": "persona:airc:e2e",
                    "data": { "personaId": "helper-ai", "route": "daemon-ipc" }
                }),
            ),
        },
    );

    let publish = module
        .handle_command("airc/realtime-publish", json!({ "envelope": envelope }))
        .await
        .unwrap();
    let CommandResult::Json(publish_value) = publish else {
        panic!("expected JSON publish result");
    };
    assert_eq!(publish_value["ok"], true);
    assert_eq!(publish_value["eventId"], TEST_AIRC_EVENT_ID.to_string());

    let delivered = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivered.name, "persona:airc:e2e");
    assert_eq!(delivered.payload["data"]["personaId"], "helper-ai");
    assert_eq!(delivered.payload["data"]["route"], "daemon-ipc");

    let replay = module
        .handle_command(
            "airc/realtime-replay",
            json!({
                "roomId": TEST_ROOM_ID.to_string(),
                "limit": 10
            }),
        )
        .await
        .unwrap();
    let CommandResult::Json(replay_value) = replay else {
        panic!("expected JSON replay result");
    };
    assert_eq!(replay_value["events"].as_array().unwrap().len(), 1);
    assert_eq!(
        replay_value["events"][0]["eventId"],
        "continuum-runtime-e2e"
    );
    assert_eq!(replay_value["cursor"]["lamport"], 1);
    assert_eq!(
        replay_value["cursor"]["eventId"],
        TEST_AIRC_EVENT_ID.to_string()
    );

    assert_eq!(daemon.resolve_count(), 1);
    assert_eq!(daemon.publish_count(), 1);
    assert_eq!(daemon.inbox_count(), 1);
    assert_eq!(daemon.attach_count(), 1);
}

struct TestAircDaemon {
    state: Arc<TestAircDaemonState>,
    task: tokio::task::JoinHandle<()>,
}

impl TestAircDaemon {
    async fn start(airc_home: &std::path::Path) -> Self {
        let socket_path = default_socket_path_in(airc_home);
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let _ = std::fs::remove_file(&socket_path);
        let listener = IpcListener::bind(&socket_path).await.unwrap();
        let state = Arc::new(TestAircDaemonState::new(airc_home.join("wire")));
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            while let Ok(stream) = listener.accept().await {
                let state = task_state.clone();
                tokio::spawn(async move {
                    state.handle_connection(stream).await;
                });
            }
        });
        Self { state, task }
    }

    async fn wait_for_attach(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while self.attach_count() == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    fn resolve_count(&self) -> usize {
        self.state.resolve_count.load(Ordering::SeqCst)
    }

    fn publish_count(&self) -> usize {
        self.state.publish_count.load(Ordering::SeqCst)
    }

    fn inbox_count(&self) -> usize {
        self.state.inbox_count.load(Ordering::SeqCst)
    }

    fn attach_count(&self) -> usize {
        self.state.attach_count.load(Ordering::SeqCst)
    }
}

impl Drop for TestAircDaemon {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct TestAircDaemonState {
    wire: PathBuf,
    lamport: AtomicU64,
    resolve_count: AtomicUsize,
    publish_count: AtomicUsize,
    inbox_count: AtomicUsize,
    attach_count: AtomicUsize,
    events: Mutex<Vec<TranscriptEvent>>,
    attach_streams: Mutex<Vec<tokio::sync::mpsc::UnboundedSender<Response>>>,
}

impl TestAircDaemonState {
    fn new(wire: PathBuf) -> Self {
        Self {
            wire,
            lamport: AtomicU64::new(0),
            resolve_count: AtomicUsize::new(0),
            publish_count: AtomicUsize::new(0),
            inbox_count: AtomicUsize::new(0),
            attach_count: AtomicUsize::new(0),
            events: Mutex::new(Vec::new()),
            attach_streams: Mutex::new(Vec::new()),
        }
    }

    async fn handle_connection(self: Arc<Self>, mut stream: IpcStream) {
        let Ok(Some(request)) = read_frame::<_, Request>(&mut stream).await else {
            return;
        };
        match request {
            Request::Attach(_) => self.handle_attach(stream).await,
            Request::ResolveWire(_) => {
                self.resolve_count.fetch_add(1, Ordering::SeqCst);
                let response = Response::ResolveWire(ResolveWireResponse {
                    wire: Some(self.wire.clone()),
                });
                let _ = write_frame(&mut stream, &response).await;
            }
            Request::Publish(request) => self.handle_publish(stream, request).await,
            Request::Inbox(request) => self.handle_inbox(stream, request).await,
            Request::Ping => {
                let _ = write_frame(&mut stream, &Response::Pong).await;
            }
            Request::Status
            | Request::AddPeer(_)
            | Request::RemovePeer(_)
            | Request::ListPeers
            | Request::Send(_)
            | Request::Subscribe(_)
            | Request::Stop => {
                let _ = write_frame(&mut stream, &Response::Ok).await;
            }
        }
    }

    async fn handle_attach(&self, mut stream: IpcStream) {
        self.attach_count.fetch_add(1, Ordering::SeqCst);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        self.attach_streams.lock().push(tx);
        let _ = write_frame(&mut stream, &Response::Ok).await;

        while let Some(response) = rx.recv().await {
            if write_frame(&mut stream, &response).await.is_err() {
                return;
            }
        }
    }

    async fn handle_publish(&self, mut stream: IpcStream, request: PublishRequest) {
        self.publish_count.fetch_add(1, Ordering::SeqCst);
        let lamport = self.lamport.fetch_add(1, Ordering::SeqCst) + 1;
        let event = TranscriptEvent {
            event_id: TEST_AIRC_EVENT_ID,
            room_id: RoomId::from_uuid(request.channel),
            peer_id: PeerId::from_u128(0xC1),
            client_id: ClientId::from_u128(0xD1),
            kind: transcript_kind_for_frame(request.kind),
            occurred_at_ms: 1_000 + lamport,
            lamport,
            target: request.target,
            headers: request.headers,
            body: Some(request.body),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        };
        self.events.lock().push(event.clone());
        self.attach_streams.lock().retain(|tx| {
            tx.send(Response::Event {
                event: Box::new(event.clone()),
            })
            .is_ok()
        });
        let response = Response::Publish(PublishResponse {
            event_id: event.event_id,
            lamport: event.lamport,
            occurred_at_ms: event.occurred_at_ms,
            channel_id: event.room_id,
        });
        let _ = write_frame(&mut stream, &response).await;
    }

    async fn handle_inbox(&self, mut stream: IpcStream, request: InboxRequest) {
        self.inbox_count.fetch_add(1, Ordering::SeqCst);
        let limit = request.limit.unwrap_or(32);
        let mut events: Vec<_> = self
            .events
            .lock()
            .iter()
            .filter(|event| {
                request
                    .channel
                    .map(|room| event.room_id == room)
                    .unwrap_or(true)
            })
            .filter(|event| {
                request
                    .since
                    .as_ref()
                    .map(|cursor| event_after_cursor(event, cursor))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        events.sort_by(|left, right| {
            left.lamport
                .cmp(&right.lamport)
                .then_with(|| left.event_id.as_uuid().cmp(&right.event_id.as_uuid()))
        });
        if events.len() > limit {
            events.truncate(limit);
        }
        let newest = events.last().map(TranscriptEvent::cursor);
        let response = Response::Inbox(InboxResponse { events, newest });
        let _ = write_frame(&mut stream, &response).await;
    }
}

fn event_after_cursor(event: &TranscriptEvent, cursor: &TranscriptCursor) -> bool {
    event.lamport > cursor.lamport
        || (event.lamport == cursor.lamport && event.event_id.as_uuid() > cursor.event_id.as_uuid())
}

fn transcript_kind_for_frame(kind: FrameKind) -> TranscriptKind {
    match kind {
        FrameKind::Message => TranscriptKind::Message,
        FrameKind::Event => TranscriptKind::Presence,
        FrameKind::Control => TranscriptKind::SessionControl,
    }
}
