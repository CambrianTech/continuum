//! The one HTTP server: every format's route through the same handler.

use crate::route::{self, RouteError};
use crate::wire::{anthropic::Anthropic, chat::Chat, responses::Responses, BadRequest, ErrorKind, Wire};
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};

pub(crate) fn router() -> Router {
    Router::new()
        .route("/v1/messages", post(serve::<Anthropic>))
        .route("/v1/chat/completions", post(serve::<Chat>))
        .route("/v1/responses", post(serve::<Responses>))
        .route("/v1/models", get(models))
        .route("/health", get(|| async { Json(json!({"status": "ok", "service": "continuum-gateway"})) }))
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
async fn serve<W: Wire>(headers: axum::http::HeaderMap, Json(body): Json<Value>) -> Response {
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
    let mut lease = crate::session::record(session, &id, W::NAME, &call.request);
    if !call.stream {
        let outcome = route::generate(&adapter, call.request).await;
        if let Some(lease) = lease.as_mut() {
            match &outcome {
                Ok(response) => lease.finish(Some(response), None),
                Err(error) => lease.finish(None, Some(&format!("{error:?}"))),
            };
        }
        return match outcome {
            Ok(response) => Json(W::answer(&response, &id)).into_response(),
            Err(error) => refused::<W>(error),
        };
    }
    let (chunks, mut received) = tokio::sync::mpsc::unbounded_channel();
    let (events, out) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut encoder = W::streamer(&model, &id);
        let generation = tokio::spawn(async move { route::stream(&adapter, call.request, chunks).await });
        let send = |batch: Vec<crate::wire::Frame>| batch.into_iter().all(|f| events.send(Ok::<_, std::convert::Infallible>(f.into_event())).is_ok());
        if !send(encoder.opened()) {
            return; // the agent left; the lease records the call as cancelled when dropped
        }
        while let Some(chunk) = received.recv().await {
            if !send(encoder.chunk(&chunk)) {
                return;
            }
        }
        let closing = match generation.await {
            Ok(Ok(response)) => {
                if let Some(lease) = lease.as_mut() {
                    lease.finish(Some(&response), None);
                }
                encoder.finished(&response)
            }
            Ok(Err(RouteError::Engine(why))) => {
                if let Some(lease) = lease.as_mut() {
                    lease.finish(None, Some(&why));
                }
                encoder.failed(&why)
            }
            Ok(Err(RouteError::UnknownModel { model, .. })) => encoder.failed(&format!("model `{model}` is not served here")),
            Err(join) => encoder.failed(&format!("generation task ended: {join}")),
        };
        send(closing);
    });
    Sse::new(tokio_stream::wrappers::UnboundedReceiverStream::new(out)).keep_alive(KeepAlive::default()).into_response()
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
