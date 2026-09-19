//! Dev loader: JSONL from `scripts/synth_ledger.py` → `activity_events` (S2-T3).
//!
//! Usage: `DATABASE_URL=… cargo run --bin load_ledger -- /tmp/ledger.jsonl`

use std::io::BufRead;

use spatial_os::intelligence::ledger::{ActivityEvent, LedgerService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let path = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: load_ledger <file.jsonl>"))?;
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    let pool = spatial_os::db::connect(&url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let ledger = LedgerService::new(Some(pool), std::env::var("LEDGER_HASH_KEY").unwrap_or_else(|_| "agentrix-ledger-dev-key".into()).into_bytes());

    let file = std::fs::File::open(&path)?;
    let mut events = Vec::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let ev: ActivityEvent = serde_json::from_str(&line)?;
        events.push(ev);
    }
    let n = ledger.import(events).await?;
    println!("loaded {n} events from {path}");
    Ok(())
}
