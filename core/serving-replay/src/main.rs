//! `serving-replay`: replay an act sequence against an engine and print the report.
//! Flags and defaults are documented in the crate README.

use std::path::PathBuf;

use serving_replay::acts::ActSequence;
use serving_replay::captures::{self, CaptureOrder, CaptureSelection};
use serving_replay::engine_process::EngineProcess;
use serving_replay::http::Endpoint;
use serving_replay::llama_server::LlamaServer;
use serving_replay::report::{Report, Summary};
use serving_replay::seated::SeatedTarget;
use serving_replay::synthetic::{self, SyntheticSpec};
use serving_replay::{model, replay};

struct Args {
    captures: Option<PathBuf>,
    synthetic: SyntheticSpec,
    selection: CaptureSelection,
    model: PathBuf,
    engine_bin: Option<PathBuf>,
    attach: Option<String>,
    port: u16,
    seats: u32,
    seat_ctx: u32,
    kv_type: String,
    cache_ram_mib: u32,
    llama_src: PathBuf,
    out: Option<PathBuf>,
    quiet: bool,
    engine_extra: Vec<String>,
}

/// `$CONTINUUM_HOME`, else `~/.continuum` (the core's `paths::continuum_home` rule; this
/// crate does not link the core).
fn continuum_home() -> PathBuf {
    std::env::var_os("CONTINUUM_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".continuum")) // unwrap_or_*: the documented home resolution order
}

fn parse() -> Result<Args, String> {
    let home = continuum_home();
    let mut a = Args {
        captures: None,
        synthetic: SyntheticSpec { minds: 4, acts: 24, shared_head_chars: 8_000, base_chars: 24_000, tail_chars: 2_000, reply_chars: 600, switch: 0.6, seed: 1, n_predict: 8 },
        selection: CaptureSelection { minds: 4, purpose: Some("cognition/act".into()), acts_per_mind: 6, message_tail_chars: None, n_predict: 8, order: CaptureOrder::Time },
        model: home.join("models/validators/qwen35-0.8b.gguf"),
        engine_bin: None,
        attach: None,
        port: 58190,
        seats: 2,
        seat_ctx: 98_304,
        kv_type: "q4_0".into(),
        cache_ram_mib: 9_696,
        llama_src: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/llama.cpp")),
        out: None,
        quiet: false,
        engine_extra: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        macro_rules! num {
            () => {{
                let v = val()?;
                v.parse().map_err(|_| format!("bad value for {flag}: {v}"))?
            }};
        }
        match flag.as_str() {
            "--captures" => a.captures = Some(PathBuf::from(val()?)),
            "--synthetic" => a.captures = None,
            "--model" => a.model = PathBuf::from(val()?),
            "--engine-bin" => a.engine_bin = Some(PathBuf::from(val()?)),
            "--attach" => a.attach = Some(val()?),
            "--port" => a.port = num!(),
            "--seats" => a.seats = num!(),
            "--seat-ctx" => a.seat_ctx = num!(),
            "--kv-type" => a.kv_type = val()?,
            "--cache-ram-mib" => a.cache_ram_mib = num!(),
            "--llama-src" => a.llama_src = PathBuf::from(val()?),
            "--out" => a.out = Some(PathBuf::from(val()?)),
            "--quiet" => a.quiet = true,
            "--minds" => {
                let n: usize = num!();
                a.synthetic.minds = n;
                a.selection.minds = n;
            }
            "--n-predict" => {
                let n: u32 = num!();
                a.synthetic.n_predict = n;
                a.selection.n_predict = n;
            }
            "--acts" => a.synthetic.acts = num!(),
            "--head-chars" => a.synthetic.shared_head_chars = num!(),
            "--base-chars" => a.synthetic.base_chars = num!(),
            "--tail-chars" => a.synthetic.tail_chars = num!(),
            "--reply-chars" => a.synthetic.reply_chars = num!(),
            "--switch" => a.synthetic.switch = num!(),
            "--seed" => a.synthetic.seed = num!(),
            "--acts-per-mind" => a.selection.acts_per_mind = num!(),
            "--purpose" => {
                let p = val()?;
                a.selection.purpose = (p != "any").then_some(p);
            }
            "--order" => {
                a.selection.order = match val()?.as_str() {
                    "time" => CaptureOrder::Time,
                    "round-robin" => CaptureOrder::RoundRobin,
                    other => return Err(format!("bad value for --order: {other} (time | round-robin)")),
                }
            }
            "--message-tail-chars" => a.selection.message_tail_chars = Some(num!()),
            "--" => a.engine_extra.extend(it.by_ref()),
            "--help" | "-h" => return Err("usage: see core/serving-replay/README.md".into()),
            other => return Err(format!("unknown flag {other} (see core/serving-replay/README.md)")),
        }
    }
    Ok(a)
}

/// The live lane's flags (read from the M5's running engine, `ps`, 2026-10-23, fork
/// e7e1077b3), sized for the replay's seats and model. What differs from live is only
/// what is the replay's own: model, port, context size, slot-save path.
fn engine_argv(a: &Args, engine_bin: &std::path::Path, page_dir: &std::path::Path) -> Vec<String> {
    let ctx = u64::from(a.seats) * u64::from(a.seat_ctx);
    let mut argv: Vec<String> = [
        engine_bin.display().to_string(),
        "-m".into(), a.model.display().to_string(),
        "--host".into(), "127.0.0.1".into(),
        "--port".into(), a.port.to_string(),
        "-c".into(), ctx.to_string(),
        "--parallel".into(), a.seats.to_string(),
        "--cache-reuse".into(), "256".into(),
        "--ctx-checkpoints".into(), "32".into(),
        "--cache-ram".into(), a.cache_ram_mib.to_string(),
        "--slot-save-path".into(), format!("{}/", page_dir.display()),
        "--ubatch-size".into(), "2048".into(),
        "--no-context-shift".into(),
        "--jinja".into(),
        "--cache-type-k".into(), a.kv_type.clone(),
        "--cache-type-v".into(), a.kv_type.clone(),
        "--flash-attn".into(), "on".into(),
    ]
    .into();
    argv.extend(a.engine_extra.iter().cloned());
    argv
}

/// The installed engine built from the fork commit this tree pins (`core/vendor/LLAMA_CPP_PIN`):
/// the `bin/engine-*/` whose `.llama-server.stamp` names that commit. The slot letter is
/// deployment state; the pin is the fact.
fn pinned_engine(home: &std::path::Path) -> Result<PathBuf, String> {
    let pin_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/LLAMA_CPP_PIN");
    let pin = std::fs::read_to_string(pin_path).map_err(|e| format!("cannot read {pin_path}: {e}; pass --engine-bin"))?;
    let short = pin.trim().get(..9).ok_or_else(|| format!("{pin_path} holds no commit; pass --engine-bin"))?.to_owned();
    let bin = home.join("bin");
    let dirs = std::fs::read_dir(&bin).map_err(|e| format!("cannot list {}: {e}; pass --engine-bin", bin.display()))?;
    dirs.filter_map(Result::ok)
        .map(|d| d.path())
        .filter(|d| d.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("engine-")))
        .find(|d| std::fs::read_to_string(d.join(".llama-server.stamp")).is_ok_and(|s| s.starts_with(&short)))
        .map(|d| d.join("llama-server"))
        .ok_or_else(|| format!("no engine under {} is built from the pinned fork {short}; pass --engine-bin", bin.display()))
}

fn sequence(a: &Args) -> Result<ActSequence, String> {
    match &a.captures {
        Some(dir) => captures::load(dir, &a.selection),
        None => Ok(synthetic::build(&a.synthetic)),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(e) => {
            eprintln!("serving-replay: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// `Ok(false)` when the replay ran but failed its own gate: a failed act or a relaunch.
fn run() -> Result<bool, String> {
    let a = parse()?;
    let sequence = sequence(&a)?;
    eprintln!("serving-replay: {}", sequence.label);
    let page_dir = std::env::temp_dir().join(format!("serving-replay-pages-{}", std::process::id()));
    std::fs::create_dir_all(&page_dir).map_err(|e| format!("cannot create {}: {e}", page_dir.display()))?;
    let result = replay_with_engine(&a, &sequence, &page_dir);
    let _ = std::fs::remove_dir_all(&page_dir); // pages are this run's scratch; the engine is already stopped
    let report = result?;
    let json = serde_json::to_string_pretty(&report).map_err(|e| format!("cannot encode report: {e}"))?; // BOUNDARY: the report file
    let out = a.out.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("serving-replay-{}.json", std::process::id()))); // unwrap_or_else: no --out writes beside the system temp files
    std::fs::write(&out, json).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!("{}\nreport   {}", report.screen(), out.display());
    Ok(report.summary.failed == 0 && report.summary.relaunches.unwrap_or_default() == 0) // unwrap_or_default: an unobserved engine cannot fail this gate
}

fn replay_with_engine(a: &Args, sequence: &ActSequence, page_dir: &std::path::Path) -> Result<Report, String> {
    let (engine, endpoint_label) = match &a.attach {
        Some(addr) => (LlamaServer::attached(Endpoint(addr.clone()), a.seats, a.synthetic.seed), addr.clone()),
        None => {
            let endpoint = Endpoint(format!("127.0.0.1:{}", a.port));
            let engine_bin = match &a.engine_bin {
                Some(bin) => bin.clone(),
                None => pinned_engine(&continuum_home())?,
            };
            let argv = engine_argv(a, &engine_bin, page_dir);
            eprintln!("serving-replay: launching {}", argv.join(" "));
            let process = EngineProcess::launch(argv, page_dir.join("engine.log"), &endpoint).map_err(|e| e.0)?;
            (LlamaServer::launched(endpoint.clone(), a.seats, process, a.synthetic.seed), endpoint.0)
        }
    };
    let ready_ms = engine.ready_ms();
    eprintln!("serving-replay: engine at {endpoint_label}{}", ready_ms.map(|ms| format!(", ready in {ms:.0} ms")).unwrap_or_default()); // unwrap_or_default: an attached engine was already up
    let mut target = SeatedTarget::new(engine);
    let replayed = replay::run(&mut target, sequence, !a.quiet).map_err(|e| e.0)?;
    let summary = Summary::of(&replayed.acts, &replayed.engine);
    Ok(Report {
        sequence: sequence.label.clone(),
        model: model::label(&a.model, &a.llama_src),
        engine: replayed.engine,
        engine_ready_ms: ready_ms,
        replay_wall_ms: replayed.wall_ms,
        summary,
        acts: replayed.acts,
    })
}
