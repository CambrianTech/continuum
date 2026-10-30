//! The one HTTP server: every format's route through the same handler.

use crate::stop::{Signal, STOPPING};
use crate::route::{self, RouteError};
use crate::wire::{anthropic::Anthropic, chat::Chat, responses::Responses, BadRequest, ErrorKind, Wire};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};

/// The routes, serving until the module raises `abandoned` (`crate::listener`): then every
/// call still in flight is stopped and its agent told why.
pub(crate) fn router(abandoned: Signal) -> Router {
    Router::new()
        .route("/v1/messages", post(serve::<Anthropic>))
        .route("/v1/chat/completions", post(serve::<Chat>))
        .route("/v1/responses", post(serve::<Responses>))
        .route("/v1/models", get(models))
        .route("/health", get(|| async { Json(json!({"status": "ok", "service": "continuum-gateway"})) }))
        .with_state(abandoned)
}

/// `GET /v1/models`: what this node serves, in the shape every format's clients read.
async fn models() -> Json<Value> {
    let data: Vec<Value> = route::served_models()
        .await
        .into_iter()
        .map(|id| json!({"id": id, "object": "model", "type": "model", "display_name": id, "owned_by": "continuum"}))
        .collect();
    Json(json!({"object": "list", "data": data}))
}

/// Decode, select, generate, encode: the same for every format.
async fn serve<W: Wire>(State(abandoned): State<Signal>, headers: axum::http::HeaderMap, Json(body): Json<Value>) -> Response {
    let mut call = match W::decode(body) {
        Ok(call) => call,
        Err(BadRequest(why)) => return refuse::<W>(StatusCode::BAD_REQUEST, ErrorKind::InvalidRequest, &why),
    };
    let model = call.request.model.clone().unwrap_or_default(); // unwrap_or_default: every decoder requires a model
    let adapter = match route::select(&model).await {
        Ok(adapter) => adapter,
        Err(error) => return refused::<W>(error),
    };
    let id = format!("{}_{}", prefix::<W>(), uuid::Uuid::new_v4().simple());
    let session = crate::session::session_of(&call.request);
    // The session is a mind: its id keys a warm page the engine pins, saves and restores.
    // Opt-in (`x-continuum-session: on`) until a driver mind is seated by MindMemory instead of
    // queueing behind resident pins: on by default it measured a 61 s steady-state step against
    // 18 s without (card e3ce65f9, M5 2026-10-27).
    let remembered = headers.get("x-continuum-session").and_then(|v| v.to_str().ok()) == Some("on");
    if remembered {
        call.request.persona_id = Some(session.to_string());
    }
    continuum_base::probe!(class = "gateway.call", wire = W::NAME, model = %model, stream = call.stream, session = %session, remembered = remembered,
        tools = call.request.tools.as_ref().map_or(0, Vec::len) as u64, messages = call.request.messages.len() as u64,
        "an agent's call reached the engine through the gateway");
    // Recorded or refused: an agent's call is never served outside the one capture.
    let opened = continuum_capture::prompt_capture::JsonlPromptCaptureSink::open_for_persona(session);
    let mut lease = match crate::session::record(opened, session, &id, W::NAME, &call.request) {
        Ok(lease) => lease,
        Err(unavailable) => return capture_refused::<W>(&unavailable),
    };
    if !call.stream {
        // Abandoned: the generation is dropped (the engine's cancellation path) and so is the
        // lease, which records the call as cancelled.
        let outcome = tokio::select! {
            outcome = route::generate(&adapter, call.request) => outcome,
            () = abandoned.raised() => return refuse::<W>(StatusCode::SERVICE_UNAVAILABLE, ErrorKind::Engine, STOPPING),
        };
        crate::session::settle(&mut lease, &outcome);
        return match outcome {
            Ok(response) => Json(W::answer(&response, &id)).into_response(),
            Err(error) => refused::<W>(error),
        };
    }
    // The body owns the generation and its lease: the agent leaving drops both (`answer`).
    let (chunks, received) = tokio::sync::mpsc::unbounded_channel();
    let generation = async move {
        let outcome = route::stream(&adapter, call.request, chunks).await;
        crate::session::settle(&mut lease, &outcome);
        outcome
    };
    streamed(crate::answer::Answer::new(W::streamer(&model, &id), generation, received, abandoned.raised()))
}

/// A streamed answer as the SSE response its body is.
pub(crate) fn streamed<W: Wire>(answer: crate::answer::Answer<W>) -> Response {
    let events = tokio_stream::StreamExt::map(answer, |frame| Ok::<_, std::convert::Infallible>(frame.into_event()));
    Sse::new(events).keep_alive(KeepAlive::default()).into_response()
}

/// The capture could not be opened: the call is refused, since serving it would leave a
/// generation the act inspector, replay and Dreaming can never read.
pub(crate) fn capture_refused<W: Wire>(unavailable: &crate::session::CaptureUnavailable) -> Response {
    refuse::<W>(StatusCode::SERVICE_UNAVAILABLE, ErrorKind::CaptureUnavailable, &unavailable.to_string())
}

fn refused<W: Wire>(error: RouteError) -> Response {
    match error {
        RouteError::UnknownModel { model, served } => refuse::<W>(
            StatusCode::NOT_FOUND,
            ErrorKind::NotFound,
            &format!("model `{model}` is not served here; served: {}", served.join(", ")),
        ),
        RouteError::Engine(why) => refuse::<W>(StatusCode::BAD_GATEWAY, ErrorKind::Engine, &why),
    }
}

fn refuse<W: Wire>(status: StatusCode, kind: ErrorKind, message: &str) -> Response {
    continuum_base::probe!(class = "gateway.refused", wire = W::NAME, status = status.as_u16() as u64, reason = message,
        "the gateway refused an agent's call");
    (status, Json(W::error(kind, message))).into_response()
}

fn prefix<W: Wire>() -> &'static str {
    match W::NAME {
        "anthropic_messages" => "msg",
        "openai_responses" => "resp",
        _ => "chatcmpl",
    }
}
