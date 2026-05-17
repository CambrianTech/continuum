use continuum_core::vdd::{
    ArtifactWriter, ChatRoundtripConfig, ChatRoundtripHarness, HarnessStatus, LiveChatProbe,
};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let harness = match args.next() {
        Some(name) => name,
        None => {
            eprintln!("usage: cargo continuum-vdd <chat-roundtrip-live>");
            std::process::exit(2);
        }
    };

    let result = match harness.as_str() {
        "chat-roundtrip-live" => {
            let runner =
                ChatRoundtripHarness::new(LiveChatProbe, ArtifactWriter::continuum_default());
            let config = match ChatRoundtripConfig::from_env() {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("invalid chat-roundtrip-live config: {error}");
                    std::process::exit(2);
                }
            };
            runner.run(config).await
        }
        other => {
            eprintln!("unknown continuum-vdd harness: {other}");
            std::process::exit(2);
        }
    };

    let bundle = match result {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("continuum-vdd failed to write artifacts: {error}");
            std::process::exit(1);
        }
    };

    let record_body = std::fs::read_to_string(&bundle.record_jsonl)
        .expect("record just written by continuum-vdd must be readable");
    let record: continuum_core::vdd::StandardVddRecord =
        serde_json::from_str(record_body.trim()).expect("record just written must parse");
    println!("{}", bundle.dir.display());
    match record.status {
        HarnessStatus::Pass => {}
        HarnessStatus::PrerequisiteMissing => std::process::exit(3),
        HarnessStatus::Fail => std::process::exit(1),
    }
}
