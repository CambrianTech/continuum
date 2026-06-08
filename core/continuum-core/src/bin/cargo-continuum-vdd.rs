use continuum_core::vdd::{
    ArtifactWriter, ChatRoundtripConfig, ChatRoundtripHarness, HarnessId, HarnessStatus,
    LiveChatProbe, HARNESS_SPECS,
};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    List,
    Run(HarnessId),
}

#[tokio::main]
async fn main() {
    let command = match parse_command(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("usage: cargo continuum-vdd list");
            eprintln!("usage: cargo continuum-vdd <chat-roundtrip-live>");
            std::process::exit(2);
        }
    };

    if command == Command::List {
        match serde_json::to_string_pretty(HARNESS_SPECS) {
            Ok(body) => {
                println!("{body}");
                return;
            }
            Err(error) => {
                eprintln!("continuum-vdd failed to serialize harness registry: {error}");
                std::process::exit(1);
            }
        }
    }

    let result = match command {
        Command::List => unreachable!("list command returned before harness execution"),
        Command::Run(HarnessId::ChatRoundtripLive) => {
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
    };

    let bundle = match result {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("continuum-vdd failed to write artifacts: {error}");
            std::process::exit(1);
        }
    };

    let record_body = match std::fs::read_to_string(&bundle.record_jsonl) {
        Ok(body) => body,
        Err(error) => {
            eprintln!(
                "continuum-vdd failed to read record {}: {error}",
                bundle.record_jsonl.display()
            );
            std::process::exit(1);
        }
    };
    let record: continuum_core::vdd::StandardVddRecord =
        match serde_json::from_str(record_body.trim()) {
            Ok(record) => record,
            Err(error) => {
                eprintln!(
                    "continuum-vdd wrote an invalid record {}: {error}",
                    bundle.record_jsonl.display()
                );
                std::process::exit(1);
            }
        };
    println!("{}", bundle.dir.display());
    match record.status {
        HarnessStatus::Pass => {}
        HarnessStatus::PrerequisiteMissing => std::process::exit(3),
        HarnessStatus::Fail => std::process::exit(1),
    }
}

fn parse_command(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err("missing continuum-vdd command".to_string());
    };
    if let Some(extra) = args.next() {
        return Err(format!("unexpected extra continuum-vdd argument: {extra}"));
    }
    match first.as_str() {
        "list" => Ok(Command::List),
        harness => HarnessId::from_str(harness)
            .map(Command::Run)
            .map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<Command, String> {
        parse_command(values.iter().map(|value| (*value).to_string()))
    }

    #[test]
    fn list_is_a_first_class_command() {
        assert_eq!(parse(&["list"]), Ok(Command::List));
    }

    #[test]
    fn direct_harness_invocation_remains_supported() {
        assert_eq!(
            parse(&["chat-roundtrip-live"]),
            Ok(Command::Run(HarnessId::ChatRoundtripLive))
        );
    }

    #[test]
    fn missing_command_fails_loud() {
        assert_eq!(parse(&[]), Err("missing continuum-vdd command".to_string()));
    }

    #[test]
    fn unknown_harness_fails_loud() {
        assert_eq!(
            parse(&["helper-chat"]),
            Err("unknown continuum-vdd harness: helper-chat".to_string())
        );
    }

    #[test]
    fn extra_arguments_fail_loud() {
        assert_eq!(
            parse(&["chat-roundtrip-live", "extra"]),
            Err("unexpected extra continuum-vdd argument: extra".to_string())
        );
    }
}
