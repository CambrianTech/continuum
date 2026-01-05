use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

pub mod inference {
    tonic::include_proto!("inference");
}

use inference::inference_server::{Inference, InferenceServer};
use inference::{
    generate_response, Complete, GenerateRequest, GenerateResponse, PingRequest, PingResponse,
    Progress,
};

#[derive(Default)]
pub struct InferenceService {}

#[tonic::async_trait]
impl Inference for InferenceService {
    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        println!("[GRPC] Ping received");
        Ok(Response::new(PingResponse {
            message: "pong".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        }))
    }

    type GenerateStream = ReceiverStream<Result<GenerateResponse, Status>>;

    async fn generate(
        &self,
        request: Request<GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        let req = request.into_inner();
        let max_tokens = req.max_tokens.max(10) as usize;
        let prompt = req.prompt;

        println!("[GRPC] Generate: prompt='{}', max_tokens={}", prompt, max_tokens);

        let (tx, rx) = mpsc::channel(32);

        // Spawn INSTANT fake generation - no delay, just return immediately
        tokio::spawn(async move {
            let start = Instant::now();

            // INSTANT - no simulation delay
            let fake_text = "This is a hardcoded response from the gRPC inference server. The communication is working!".to_string();
            let duration = start.elapsed().as_millis() as i32;

            let complete = GenerateResponse {
                response: Some(generate_response::Response::Complete(Complete {
                    text: fake_text,
                    tokens: 20,
                    duration_ms: duration,
                })),
            };

            if tx.send(Ok(complete)).await.is_err() {
                println!("[GRPC] Failed to send completion, client gone");
            } else {
                println!("[GRPC] INSTANT response sent ({}ms)", duration);
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    let service = InferenceService::default();

    println!("===========================================");
    println!("  Inference gRPC Server (FAKE)");
    println!("  Listening on: {}", addr);
    println!("===========================================");

    Server::builder()
        .add_service(InferenceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
