/// Search Worker - Rust-based search algorithm execution
///
/// Pattern: Same as data-daemon
/// - Unix socket listener
/// - Algorithm registry with factory pattern
/// - OpenCV-style algorithm interface
mod algorithms;

use algorithms::cosine::{CosineAlgorithm, VectorSearchInput};
use algorithms::{AlgorithmRegistry, SearchAlgorithm, SearchInput, SearchOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::{fs, thread};

// ============================================================================
// Protocol Types
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "command")]
enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "search")]
    Search {
        algorithm: String,
        query: String,
        corpus: Vec<String>,
        #[serde(default)]
        params: HashMap<String, Value>,
    },

    #[serde(rename = "list-algorithms")]
    ListAlgorithms,

    #[serde(rename = "algorithm-params")]
    AlgorithmParams { algorithm: String },

    /// Vector-based semantic search (primary use case for memory recall)
    #[serde(rename = "vector-search")]
    VectorSearch {
        /// Query embedding vector
        query_vector: Vec<f64>,
        /// Corpus embedding vectors
        corpus_vectors: Vec<Vec<f64>>,
        /// Optional: normalize vectors before comparison
        #[serde(default = "default_true")]
        normalize: bool,
        /// Optional: minimum similarity threshold
        #[serde(default)]
        threshold: f64,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "ok")]
    Ok { data: Value },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "pong")]
    Pong { algorithms: Vec<String> },
}

// ============================================================================
// Worker
// ============================================================================

struct SearchWorker {
    registry: AlgorithmRegistry,
}

impl SearchWorker {
    fn new() -> Self {
        Self {
            registry: AlgorithmRegistry::new(),
        }
    }

    fn handle_request(&self, request: Request) -> Response {
        match request {
            Request::Ping => Response::Pong {
                algorithms: self.registry.list().iter().map(|s| s.to_string()).collect(),
            },

            Request::ListAlgorithms => Response::Ok {
                data: json!({
                    "algorithms": self.registry.list()
                }),
            },

            Request::AlgorithmParams { algorithm } => match self.registry.create(&algorithm) {
                Some(algo) => Response::Ok {
                    data: json!({
                        "algorithm": algorithm,
                        "params": algo.param_names()
                    }),
                },
                None => Response::Error {
                    message: format!("Unknown algorithm: {algorithm}"),
                },
            },

            Request::Search {
                algorithm,
                query,
                corpus,
                params,
            } => {
                // Create algorithm instance
                let algo_result = if params.is_empty() {
                    self.registry
                        .create(&algorithm)
                        .ok_or_else(|| format!("Unknown algorithm: {algorithm}"))
                } else {
                    self.registry.create_with_params(&algorithm, &params)
                };

                match algo_result {
                    Ok(algo) => {
                        let input = SearchInput { query, corpus };
                        let output: SearchOutput = algo.execute(&input);

                        Response::Ok {
                            data: json!({
                                "algorithm": algorithm,
                                "scores": output.scores,
                                "ranked_indices": output.ranked_indices
                            }),
                        }
                    }
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::VectorSearch {
                query_vector,
                corpus_vectors,
                normalize,
                threshold,
            } => {
                // Create cosine algorithm with parameters
                let mut alg = CosineAlgorithm::default();
                let _ = alg.set_param("normalize", json!(normalize));
                let _ = alg.set_param("threshold", json!(threshold));

                // Execute vector search
                let input = VectorSearchInput {
                    query_vector,
                    corpus_vectors,
                };
                let output = alg.vector_search(&input);

                Response::Ok {
                    data: json!({
                        "algorithm": "cosine",
                        "scores": output.scores,
                        "ranked_indices": output.ranked_indices
                    }),
                }
            }
        }
    }
}

// ============================================================================
// Connection Handler
// ============================================================================

fn handle_connection(stream: UnixStream, worker: Arc<SearchWorker>) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }

        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let err_response = Response::Error {
                    message: format!("Parse error: {e}"),
                };
                let json = serde_json::to_string(&err_response)?;
                writeln!(writer, "{json}")?;
                writer.flush()?;
                continue;
            }
        };

        let response = worker.handle_request(request);
        let response_json = serde_json::to_string(&response)?;
        writeln!(writer, "{response_json}")?;
        writer.flush()?;
    }

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let socket_path = if args.len() >= 2 {
        args[1].clone()
    } else {
        "/tmp/jtag-search-worker.sock".to_string()
    };

    // Remove existing socket
    if fs::metadata(&socket_path).is_ok() {
        fs::remove_file(&socket_path)?;
    }

    println!("🔍 Search Worker starting...");
    println!("📡 Socket: {socket_path}");

    let worker = Arc::new(SearchWorker::new());
    println!("✅ Algorithm registry initialized");
    println!("   Algorithms: {:?}", worker.registry.list());

    let listener = UnixListener::bind(&socket_path)?;
    println!("✅ Listening for connections\n");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let worker_clone = worker.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, worker_clone) {
                        eprintln!("Connection error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {e}"),
        }
    }

    Ok(())
}
