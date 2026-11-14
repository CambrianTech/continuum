//! Explicit offline historical-NULL migration. Never run against a serving database.
#[path = "../orm/offline_training_progress.rs"]
mod migration;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or(
        "usage: migrate-training-progress OFFLINE_DATABASE [--apply-historical-unstarted]",
    )?;
    let apply = match args.next() {
        None => false,
        Some(flag) if flag == "--apply-historical-unstarted" => true,
        Some(_) => {
            return Err("only --apply-historical-unstarted is accepted; otherwise dry run".into())
        }
    };
    if args.next().is_some() {
        return Err("unexpected migration arguments".into());
    }
    let receipt = migration::run(std::path::Path::new(&path), apply).await?;
    println!(
        "{}",
        serde_json::to_string(&receipt).map_err(|error| error.to_string())?
    );
    Ok(())
}
