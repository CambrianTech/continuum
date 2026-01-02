/// Embedding Worker - Native Rust Embedding Generation
///
/// Generates text embeddings using fastembed (ONNX-based) instead of Ollama HTTP.
///
/// Benefits over Ollama HTTP:
/// - No network overhead (~5ms vs ~80ms per embedding)
/// - Batch processing (100 texts in ~100ms vs ~8s)
/// - No external service dependency
/// - True parallelism via ONNX Runtime
///
/// Protocol: Unix socket + newline-delimited JSON (same as other workers)

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{fs, thread};

// ============================================================================
// Model Registry - Lazy-loaded models
// ============================================================================

/// Global model cache - models loaded on demand
static MODEL_CACHE: OnceCell<Arc<Mutex<HashMap<String, TextEmbedding>>>> = OnceCell::new();

fn get_model_cache() -> &'static Arc<Mutex<HashMap<String, TextEmbedding>>> {
    MODEL_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

/// Get cache directory for fastembed models
fn get_cache_dir() -> PathBuf {
    if let Ok(path) = std::env::var("FASTEMBED_CACHE_PATH") {
        PathBuf::from(path)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".continuum/models/fastembed")
    }
}

/// Map string model name to fastembed EmbeddingModel enum
fn parse_model_name(name: &str) -> Result<EmbeddingModel, String> {
    // All patterns must be lowercase since we call .to_lowercase() on input
    match name.to_lowercase().as_str() {
        "allminilml6v2" | "all-minilm-l6-v2" | "default" => Ok(EmbeddingModel::AllMiniLML6V2),
        "allminilml6v2q" | "all-minilm-l6-v2-q" => Ok(EmbeddingModel::AllMiniLML6V2Q),
        "bgesmallenv15" | "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        "bgebaseenv15" | "bge-base-en-v1.5" => Ok(EmbeddingModel::BGEBaseENV15),
        "bgelargeenv15" | "bge-large-en-v1.5" => Ok(EmbeddingModel::BGELargeENV15),
        "nomicembedtextv1" | "nomic-embed-text-v1" => Ok(EmbeddingModel::NomicEmbedTextV1),
        "nomicembedtextv15" | "nomic-embed-text-v1.5" => Ok(EmbeddingModel::NomicEmbedTextV15),
        _ => Err(format!("Unknown model: {}. Use 'embedding/model/list' to see available models.", name)),
    }
}

/// Get or load a model by name
fn get_or_load_model(model_name: &str) -> Result<Arc<Mutex<HashMap<String, TextEmbedding>>>, String> {
    let cache = get_model_cache();
    let mut models = cache.lock().map_err(|e| format!("Lock error: {}", e))?;

    if !models.contains_key(model_name) {
        println!("📥 Loading model: {} (first use - may download)", model_name);
        let start = Instant::now();

        let model_enum = parse_model_name(model_name)?;
        let cache_dir = get_cache_dir();

        // Ensure cache directory exists
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create cache dir: {}", e))?;

        let model = TextEmbedding::try_new(
            InitOptions::new(model_enum)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )
        .map_err(|e| format!("Failed to load model: {}", e))?;

        let elapsed = start.elapsed();
        println!("✅ Model loaded in {:.2}s: {}", elapsed.as_secs_f64(), model_name);

        models.insert(model_name.to_string(), model);
    }

    drop(models); // Release lock
    Ok(cache.clone())
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "command")]
enum Request {
    #[serde(rename = "ping")]
    Ping,

    /// Generate embeddings for a batch of texts
    #[serde(rename = "embedding/generate")]
    Generate {
        texts: Vec<String>,
        #[serde(default = "default_model")]
        model: String,
    },

    /// Pre-load a model into memory
    #[serde(rename = "embedding/model/load")]
    ModelLoad { model: String },

    /// List available models
    #[serde(rename = "embedding/model/list")]
    ModelList,

    /// Get info about a loaded model
    #[serde(rename = "embedding/model/info")]
    ModelInfo { model: String },

    /// Unload a model from memory
    #[serde(rename = "embedding/model/unload")]
    ModelUnload { model: String },
}

fn default_model() -> String {
    "AllMiniLML6V2".to_string()
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "ok")]
    Ok { data: Value },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "pong")]
    Pong { uptime_seconds: u64 },
}

// ============================================================================
// Model Info
// ============================================================================

#[derive(Serialize)]
struct ModelInfo {
    name: String,
    dimensions: usize,
    description: String,
    size_mb: usize,
    loaded: bool,
}

fn get_model_info_list() -> Vec<ModelInfo> {
    let cache = get_model_cache();
    let loaded_models: Vec<String> = cache
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    vec![
        ModelInfo {
            name: "AllMiniLML6V2".to_string(),
            dimensions: 384,
            description: "Fast, good quality, default".to_string(),
            size_mb: 90,
            loaded: loaded_models.contains(&"AllMiniLML6V2".to_string()),
        },
        ModelInfo {
            name: "AllMiniLML6V2Q".to_string(),
            dimensions: 384,
            description: "Quantized, fastest, smallest".to_string(),
            size_mb: 25,
            loaded: loaded_models.contains(&"AllMiniLML6V2Q".to_string()),
        },
        ModelInfo {
            name: "BGESmallENV15".to_string(),
            dimensions: 384,
            description: "Better quality than MiniLM".to_string(),
            size_mb: 130,
            loaded: loaded_models.contains(&"BGESmallENV15".to_string()),
        },
        ModelInfo {
            name: "BGEBaseENV15".to_string(),
            dimensions: 768,
            description: "High quality, larger embeddings".to_string(),
            size_mb: 440,
            loaded: loaded_models.contains(&"BGEBaseENV15".to_string()),
        },
        ModelInfo {
            name: "NomicEmbedTextV15".to_string(),
            dimensions: 768,
            description: "Nomic model, same as Ollama nomic-embed-text".to_string(),
            size_mb: 550,
            loaded: loaded_models.contains(&"NomicEmbedTextV15".to_string()),
        },
    ]
}

// ============================================================================
// Request Handler
// ============================================================================

fn handle_request(request: Request, start_time: Instant) -> Response {
    match request {
        Request::Ping => {
            let uptime = start_time.elapsed().as_secs();
            Response::Pong {
                uptime_seconds: uptime,
            }
        }

        Request::Generate { texts, model } => {
            if texts.is_empty() {
                return Response::Error {
                    message: "No texts provided".to_string(),
                };
            }

            let gen_start = Instant::now();

            // Get or load model
            let cache = match get_or_load_model(&model) {
                Ok(c) => c,
                Err(e) => return Response::Error { message: e },
            };

            let models = match cache.lock() {
                Ok(m) => m,
                Err(e) => {
                    return Response::Error {
                        message: format!("Lock error: {}", e),
                    }
                }
            };

            let embedding_model = match models.get(&model) {
                Some(m) => m,
                None => {
                    return Response::Error {
                        message: format!("Model not loaded: {}", model),
                    }
                }
            };

            // Generate embeddings
            let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let embeddings = match embedding_model.embed(text_refs, None) {
                Ok(e) => e,
                Err(e) => {
                    return Response::Error {
                        message: format!("Embedding generation failed: {}", e),
                    }
                }
            };

            let duration_ms = gen_start.elapsed().as_millis() as u64;
            let dimensions = embeddings.first().map(|e| e.len()).unwrap_or(0);

            println!(
                "✨ Generated {} embeddings ({}d) in {}ms",
                embeddings.len(),
                dimensions,
                duration_ms
            );

            Response::Ok {
                data: json!({
                    "embeddings": embeddings,
                    "model": model,
                    "dimensions": dimensions,
                    "count": embeddings.len(),
                    "durationMs": duration_ms
                }),
            }
        }

        Request::ModelLoad { model } => {
            let start = Instant::now();

            match get_or_load_model(&model) {
                Ok(_) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    Response::Ok {
                        data: json!({
                            "model": model,
                            "loaded": true,
                            "durationMs": duration_ms
                        }),
                    }
                }
                Err(e) => Response::Error { message: e },
            }
        }

        Request::ModelList => {
            let models = get_model_info_list();
            Response::Ok {
                data: json!({
                    "models": models,
                    "count": models.len(),
                    "cacheDir": get_cache_dir().to_string_lossy()
                }),
            }
        }

        Request::ModelInfo { model } => {
            let models = get_model_info_list();
            match models.into_iter().find(|m| m.name == model) {
                Some(info) => Response::Ok {
                    data: serde_json::to_value(info).unwrap_or(json!({})),
                },
                None => Response::Error {
                    message: format!("Unknown model: {}", model),
                },
            }
        }

        Request::ModelUnload { model } => {
            let cache = get_model_cache();
            let mut models = match cache.lock() {
                Ok(m) => m,
                Err(e) => {
                    return Response::Error {
                        message: format!("Lock error: {}", e),
                    }
                }
            };

            if models.remove(&model).is_some() {
                println!("🗑️  Unloaded model: {}", model);
                Response::Ok {
                    data: json!({
                        "model": model,
                        "unloaded": true
                    }),
                }
            } else {
                Response::Error {
                    message: format!("Model not loaded: {}", model),
                }
            }
        }
    }
}

// ============================================================================
// Connection Handler
// ============================================================================

fn handle_connection(stream: UnixStream, start_time: Instant) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break; // Connection closed
        }

        // Parse request
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = Response::Error {
                    message: format!("Parse error: {}", e),
                };
                let response_json = serde_json::to_string(&error_response)?;
                writeln!(writer, "{}", response_json)?;
                writer.flush()?;
                continue;
            }
        };

        // Handle request
        let response = handle_request(request, start_time);

        // Send response
        let response_json = serde_json::to_string(&response)?;
        writeln!(writer, "{}", response_json)?;
        writer.flush()?;
    }

    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/jtag-embedding.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    let start_time = Instant::now();

    // Remove existing socket
    if fs::metadata(socket_path).is_ok() {
        fs::remove_file(socket_path)?;
    }

    println!("🦀 Embedding Worker starting...");
    println!("📡 Socket: {}", socket_path);
    println!("📁 Model cache: {:?}", get_cache_dir());
    println!();

    // Pre-load default model on startup (optional - comment out for lazy loading)
    println!("📥 Pre-loading default model (AllMiniLML6V2)...");
    match get_or_load_model("AllMiniLML6V2") {
        Ok(_) => println!("✅ Default model ready"),
        Err(e) => println!("⚠️  Failed to pre-load default model: {}", e),
    }
    println!();

    // Bind socket
    let listener = UnixListener::bind(socket_path)?;
    println!("✅ Listening for connections");
    println!();

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let start = start_time;
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, start) {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }

    Ok(())
}
