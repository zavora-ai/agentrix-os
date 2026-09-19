//! Best-effort sync of `mcp_allowlists.toml` into the mcp-registry control plane.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::tools::allowlist::{AllowlistCatalog, AllowlistEntry};
use crate::tools::mcp;
use crate::tools::mcp_exec;

pub async fn try_sync(config: &AppConfig, catalog: &AllowlistCatalog) -> anyhow::Result<()> {
    let Some(registry_path) = config.mcp_registry_path.as_ref() else {
        return Ok(());
    };
    if !registry_path.exists() {
        tracing::warn!(
            "MCP_REGISTRY_PATH set but binary missing at {}",
            registry_path.display()
        );
        return Ok(());
    }

    let registry = Arc::new(mcp::spawn_mcp_server(registry_path).await?);
    let mut server_ids: HashMap<String, String> = HashMap::new();

    let mut by_server: HashMap<String, Vec<&AllowlistEntry>> = HashMap::new();
    for entry in catalog.entries() {
        by_server
            .entry(entry.mcp_server.clone())
            .or_default()
            .push(entry);
    }

    for (mcp_server, entries) in &by_server {
        if mcp_server == crate::tools::tasks::TOOLSET_ID {
            // Built-in, in-process toolset (S4-T3) — nothing to register with the MCP registry.
            continue;
        }
        let Some(command) = mcp_command_for(config, mcp_server) else {
            tracing::warn!("mcp-registry sync: no path for server '{mcp_server}'");
            continue;
        };

        let register_out = mcp_exec::exec_tool(
            registry.clone(),
            "register_mcp_server",
            serde_json::json!({
                "name": mcp_server,
                "description": format!("Agentrix OS {mcp_server} MCP"),
                "owner": "agentrix-os",
                "domain": "spatial-os",
                "environment": "dev",
                "transport": "stdio",
                "command": command,
            }),
        )
        .await
        .and_then(|v| mcp_exec::tool_output_string(&v).map(str::to_string))
        .unwrap_or_default();

        let server_id = serde_json::from_str::<serde_json::Value>(&register_out)
            .ok()
            .and_then(|v| v.get("server_id").and_then(|s| s.as_str()).map(str::to_string))
            .unwrap_or_else(|| format!("mcp_{mcp_server}_dev"));

        server_ids.insert(mcp_server.clone(), server_id.clone());

        let mut tool_names: Vec<String> = entries
            .iter()
            .flat_map(|e| e.tools.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        tool_names.sort();

        let tools: Vec<serde_json::Value> = tool_names
            .iter()
            .map(|name| {
                serde_json::json!({
                    "name": name,
                    "description": format!("{name} on {mcp_server}"),
                })
            })
            .collect();

        let _ = mcp_exec::exec_tool(
            registry.clone(),
            "discover_mcp_tools",
            serde_json::json!({
                "server_id": server_id,
                "tools": tools,
            }),
        )
        .await;

        for entry in entries {
            let _ = mcp_exec::exec_tool(
                registry.clone(),
                "set_tool_allowlist",
                serde_json::json!({
                    "server_id": server_id,
                    "tool_names": entry.tools,
                    "environment": "dev",
                    "agents": [entry.agent],
                    "reason": entry.reason,
                }),
            )
            .await;
        }
    }

    tracing::info!(
        "mcp-registry synced {} servers from mcp_allowlists.toml",
        server_ids.len()
    );
    Ok(())
}

fn mcp_command_for(config: &AppConfig, server: &str) -> Option<String> {
    let path: &Path = match server {
        "worksheet" => &config.mcp_worksheet_path,
        "docx" => &config.mcp_docx_path,
        "slides" => &config.mcp_slides_path,
        "calendar" => &config.mcp_calendar_path,
        "email" => &config.mcp_email_path,
        "news" => &config.mcp_news_path,
        "weather" => &config.mcp_weather_path,
        "market_data" => &config.mcp_market_data_path,
        "slack" => &config.mcp_slack_path,
        "crm" => &config.mcp_crm_path,
        "banking" => &config.mcp_banking_path,
        "github" => &config.mcp_github_path,
        "maps" => &config.mcp_maps_path,
        "real_estate" => &config.mcp_real_estate_path,
        _ => return None,
    };
    if path.exists() {
        Some(path_to_command(path))
    } else {
        None
    }
}

fn path_to_command(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}