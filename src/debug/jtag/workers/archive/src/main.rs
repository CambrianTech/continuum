/// Archive Worker - MINIMAL SKELETON TEMPLATE
///
/// PURPOSE: Prove bidirectional TypeScript ↔ Rust communication
///
/// FLOW:
/// 1. TypeScript → Rust: Queue archive task
/// 2. Rust → TypeScript: Call Commands.execute('data/list', {limit: 0}) to get count
/// 3. TypeScript → Rust: Return count
/// 4. Rust → TypeScript: Return success
///
/// This is a TEMPLATE - kept minimal for generator to copy.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{mpsc, Arc, Mutex};
use std::{fs, thread};

// ============================================================================
// Message Types (TEMPLATE)
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command")]
enum Request {
    #[serde(rename = "archive")]
    Archive {
        task_id: String,
        collection: String,
    },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "queued")]
    Queued {
        task_id: String,
        queue_position: usize,
    },
    #[serde(rename = "complete")]
    Complete {
        task_id: String,
        rows_found: usize,
    },
    #[serde(rename = "pong")]
    Pong {
        uptime_seconds: u64,
    },
}

#[derive(Debug, Clone)]
struct Task {
    task_id: String,
    collection: String,
}

// ============================================================================
// Command Client (TEMPLATE - calls TypeScript)
// ============================================================================

struct CommandClient {
    socket_path: String,
}

impl CommandClient {
    fn new(socket_path: String) -> Self {
        Self { socket_path }
    }

    /// Execute a TypeScript command and get result
    fn execute(&self, command: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        // Connect to TypeScript command router
        let stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to command router: {}", e))?;

        let mut reader = BufReader::new(&stream);
        let mut writer = stream.try_clone().map_err(|e| e.to_string())?;

        // Send request
        let request = json!({
            "command": command,
            "params": params
        });

        let request_line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        writeln!(writer, "{}", request_line).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        // Read response
        let mut response_line = String::new();
        reader.read_line(&mut response_line).map_err(|e| e.to_string())?;

        let response: serde_json::Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(response)
    }

    /// Get row count from collection (proves data command works)
    fn get_row_count(&self, collection: &str) -> Result<usize, String> {
        let result = self.execute("data/list", json!({
            "collection": collection,
            "limit": 0
        }))?;

        let count = result.get("count")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing count in response".to_string())?;

        Ok(count as usize)
    }
}

// ============================================================================
// Main Entry Point (TEMPLATE)
// ============================================================================

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <worker-socket> <command-router-socket>", args[0]);
        eprintln!("Example: {} /tmp/archive-worker.sock /tmp/command-router.sock", args[0]);
        std::process::exit(1);
    }

    let worker_socket = &args[1];
    let command_router_socket = args[2].clone();

    // Remove socket if exists
    if fs::metadata(worker_socket).is_ok() {
        fs::remove_file(worker_socket)?;
    }

    println!("🦀 Archive Worker (Skeleton) starting...");
    println!("📡 Worker socket: {}", worker_socket);
    println!("📡 Command router: {}", command_router_socket);

    // Shared state
    let queue: Arc<Mutex<VecDeque<Task>>> = Arc::new(Mutex::new(VecDeque::new()));
    let (task_tx, task_rx) = mpsc::channel::<Task>();

    // Spawn worker thread (TEMPLATE)
    let worker_queue = queue.clone();
    let worker_router = command_router_socket.clone();
    thread::spawn(move || {
        println!("🔥 Worker thread started");
        let commands = CommandClient::new(worker_router);

        for task in task_rx.iter() {
            println!("📦 Processing task: {}", task.task_id);

            // PROOF: Call TypeScript to get row count
            match commands.get_row_count(&task.collection) {
                Ok(count) => {
                    println!("✅ Task {} complete: Found {} rows in {}",
                        task.task_id, count, task.collection);

                    // Remove from queue
                    let mut q = worker_queue.lock().unwrap();
                    q.retain(|t| t.task_id != task.task_id);
                }
                Err(e) => {
                    println!("❌ Task {} failed: {}", task.task_id, e);
                }
            }
        }
    });

    // Bind socket (TEMPLATE)
    let listener = UnixListener::bind(worker_socket)?;
    println!("✅ Ready to accept connections\n");

    // Accept connections (TEMPLATE)
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let queue_clone = queue.clone();
                let task_tx_clone = task_tx.clone();

                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, queue_clone, task_tx_clone) {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }

    Ok(())
}

// ============================================================================
// Connection Handler (TEMPLATE)
// ============================================================================

fn handle_connection(
    stream: UnixStream,
    queue: Arc<Mutex<VecDeque<Task>>>,
    task_tx: mpsc::Sender<Task>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 { break; }

        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Parse error: {}", e);
                continue;
            }
        };

        let response = match request {
            Request::Archive { task_id, collection } => {
                let task = Task { task_id: task_id.clone(), collection };

                // Queue task
                let mut q = queue.lock().unwrap();
                q.push_back(task.clone());
                let position = q.len();
                drop(q);

                // Send to worker thread
                task_tx.send(task).ok();

                Response::Queued {
                    task_id,
                    queue_position: position,
                }
            }
            Request::Ping => {
                Response::Pong {
                    uptime_seconds: 0, // TODO: Track actual uptime
                }
            }
        };

        let response_json = serde_json::to_string(&response)?;
        writeln!(writer, "{}", response_json)?;
        writer.flush()?;
    }

    Ok(())
}
