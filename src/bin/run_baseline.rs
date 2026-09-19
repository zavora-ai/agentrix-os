//! Dev runner for S7: ledger → daily values → baseline → drift, for one user as of a date.
//! The nightly job (S7-T4) will do the same on the ambient cron; this is for local runs.
//!
//! Usage: `DATABASE_URL=… cargo run --bin run_baseline -- [user_id] [as_of YYYY-MM-DD]`
//! Defaults: the synthetic user from `scripts/synth_ledger.py`, as_of = today (UTC).
//! `INTELLIGENCE_UTC_OFFSET_HOURS` (default 0) sets the day boundary, e.g. `3` for Nairobi.
//! `LEDGER_HASH_KEY` is read as in `main.rs` so the observation's ledger row uses the same key.

use chrono::{Duration, FixedOffset, NaiveDate, Utc};
use spatial_os::intelligence::{baseline, patterns, store, LedgerService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let mut args = std::env::args().skip(1);
    let user_id = args.next().unwrap_or_else(|| "00000000-0000-0000-0000-000000000001".to_string());
    let as_of = match args.next() {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")?,
        None => Utc::now().date_naive(),
    };
    let offset_hours: i32 = std::env::var("INTELLIGENCE_UTC_OFFSET_HOURS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let offset = FixedOffset::east_opt(offset_hours * 3600).ok_or_else(|| anyhow::anyhow!("offset out of range"))?;
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    let pool = spatial_os::db::connect(&url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let hash_key = std::env::var("LEDGER_HASH_KEY").unwrap_or_else(|_| "agentrix-ledger-dev-key".into()).into_bytes();
    let ledger = LedgerService::new(Some(pool.clone()), hash_key);

    let cfg = baseline::Config::default();
    let from = as_of - Duration::days(cfg.window_days + cfg.recent_days + 7);
    let events = store::fetch_events(&pool, &user_id, from, as_of + Duration::days(1)).await?;
    let daily = patterns::aggregate_in(&events, &offset);
    store::upsert_daily(&pool, &daily).await?;
    let existing = store::recent_observations(&pool, &user_id, as_of - Duration::days(cfg.cooldown_days + 1)).await?;
    let out = baseline::evaluate(&user_id, &daily, as_of, &existing, &cfg);
    store::upsert_baselines(&pool, &out.baselines).await?;

    println!(
        "user {user_id} as of {as_of} (UTC{offset_hours:+}): {} events → {} daily values, {} baselines",
        events.len(),
        daily.len(),
        out.baselines.len()
    );
    if let Some(w) = &out.warmup {
        println!("learning your routine — {} of {} days", w.days_seen, w.days_needed);
        return Ok(());
    }
    for d in &out.drifts {
        println!(
            "  drift {:<22} {:<8} baseline {:>7.2} → recent {:>7.2} (short {:>7.2}) Δ {:+.2} > {:.2}; {} days beyond",
            d.dimension, d.day_class.as_str(), d.baseline_median, d.recent_median, d.short_median, d.delta, d.threshold, d.days_beyond
        );
    }
    if !out.cooled_down.is_empty() {
        println!("  cooled down: {}", out.cooled_down.join(", "));
    }
    match &out.observation {
        Some(o) => {
            let id = store::insert_observation(&pool, o).await?;
            ledger.record(store::ledger_event(o));
            ledger.flush().await;
            println!("observation {id} [{}] (ledgered)\n  {}\n  {}", o.dimensions.join(", "), o.text, o.offer);
        }
        None => println!("no meaningful drift"),
    }
    Ok(())
}
