use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use offbeat_core::OffbeatNode;
use offbeat_core::auth;
use offbeat_core::gossip_manager::{GossipMessage, encode_gossip_message};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser, Debug)]
#[command(author, version, about = "Offbeat Headless Node", long_about = None)]
struct Args {
    /// Path to the database file. If not provided, a temporary one will be used.
    #[arg(short, long)]
    db: Option<PathBuf>,

    /// Festival ID to join at startup.
    #[arg(short, long, default_value = "rockwerchter26")]
    festival: String,

    /// Log level (offbeat_core=debug, etc.)
    #[arg(
        short,
        long,
        env = "OFFBEAT_LOG",
        default_value = "info,offbeat_core=debug,iroh_ble_transport=debug"
    )]
    logs: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::new(&args.logs))
        .init();

    let db_path = match args.db {
        Some(p) => p,
        None => tempfile::NamedTempFile::new()?.path().to_path_buf(),
    };

    tracing::info!("starting offbeat node at {}", db_path.display());

    // Create the node with full networking (BLE + Gossip + Iroh)
    let node = Arc::new(OffbeatNode::new_with_networking(&db_path).await?);

    // Bootstrap: start background discovery and sync tasks.
    let handles = node.spawn_ble_sync();
    tracing::info!("BLE stack and background sync tasks spawned");

    // Register the festival state resource so the subscription manager picks it
    // up. ADMIN_ROOT_PUBKEY lets this node verify updates signed by the server's
    // root key.
    let festival_id = args.festival.clone();
    {
        let mut reg = node.resource_registry.write().expect("lock poisoned");
        reg.register_festival(&festival_id, offbeat_core::cert::ADMIN_ROOT_PUBKEY);
    }
    tracing::info!("auto-joined festival: {}", festival_id);

    // Print local identity so the operator can recognise this node.
    if let Ok(key) = auth::generate_or_load_identity(&node.db) {
        tracing::info!(user_id = %auth::get_user_id(&key), "local identity");
    }

    // Periodic status line.
    let status_node = node.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            if let Some(cm) = &status_node.connection_manager {
                let peers = cm.peer_snapshot();
                let active = peers
                    .iter()
                    .filter(|p| {
                        matches!(
                            p.gossip_status,
                            offbeat_core::connection_manager::GossipStatus::Active
                        )
                    })
                    .count();
                tracing::info!(
                    "status: {} known peers, {} active in mesh",
                    peers.len(),
                    active
                );
            }
        }
    });

    // Command REPL: read newline-delimited commands from stdin (typically a FIFO
    // so the daemon can be driven while backgrounded).
    let cmd_node = node.clone();
    let cmd_festival = festival_id.clone();
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut lines = BufReader::new(stdin).lines();
        print_help();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if line == "quit" || line == "exit" {
                        tracing::info!("REPL: quit requested");
                        break;
                    }
                    if let Err(e) = handle_command(&cmd_node, &cmd_festival, &line).await {
                        tracing::error!("command error: {e:#}");
                    }
                }
                // EOF on the control channel: keep the node alive, stop the REPL.
                Ok(None) => {
                    tracing::debug!("REPL: stdin closed, REPL idle (node still running)");
                    break;
                }
                Err(e) => {
                    tracing::error!("REPL read error: {e}");
                    break;
                }
            }
        }
    });

    tracing::info!("node is running. send commands on stdin, or ctrl-c / SIGTERM to exit.");

    // Wait for ctrl-c (SIGINT) or SIGTERM so blew's Drop runs StopDiscovery and
    // we don't strand a BlueZ discovery session.
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
            _ = sigterm.recv() => tracing::info!("SIGTERM received"),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
    }

    tracing::info!("shutting down...");
    for handle in handles {
        handle.abort();
    }
    // Drop the node so the BLE transport's Drop stops scanning/advertising.
    drop(node);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Ok(())
}

fn print_help() {
    tracing::info!(
        "commands:\n  \
         join <offbeat://group/...>     join a group via invite payload\n  \
         msg <group_id> <text...>       send a group chat message\n  \
         checkin <group_id> <stage_id>  check in to a stage\n  \
         state <group_id>               dump group state (members + pins)\n  \
         chat <group_id>                dump recent group chat history\n  \
         groups                         list known groups\n  \
         lineup                         summarise synced festival state\n  \
         peers                          list known BLE/gossip peers\n  \
         nudge                          nudge gossip join for visible peers\n  \
         quit                           shut down"
    );
}

async fn handle_command(node: &Arc<OffbeatNode>, festival_id: &str, line: &str) -> anyhow::Result<()> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();

    let signing_key = auth::generate_or_load_identity(&node.db)?;
    let user_id = auth::get_user_id(&signing_key);
    let display_name = auth::get_display_name(&node.db)?.unwrap_or_else(|| user_id.clone());

    match cmd {
        "help" => print_help(),
        "join" => {
            if rest.is_empty() {
                anyhow::bail!("usage: join <offbeat://group/...>");
            }
            let result = node
                .group_manager
                .join_group(rest, &user_id, &display_name)
                .await?;
            {
                let mut reg = node
                    .resource_registry
                    .write()
                    .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
                reg.register_groups(&[(result.group_id.clone(), result.group_key)]);
            }
            node.sync_orchestrator
                .cache_group_key(&result.group_id, result.group_key);
            node.notifier
                .notify_doc(&format!("group/{}", result.group_id));
            tracing::info!(
                group_id = %result.group_id,
                festival_id = %result.festival_id,
                "joined group; subscribed + key cached"
            );
            // Announce our membership: broadcast the full encrypted doc state so
            // peers in the mesh learn we joined (best-effort).
            broadcast_group_state(node, &result.group_id, &result.group_key).await;
        }
        "msg" => {
            let mut a = rest.splitn(2, char::is_whitespace);
            let group_id = a.next().unwrap_or("").to_string();
            let text = a.next().unwrap_or("").trim();
            if group_id.is_empty() || text.is_empty() {
                anyhow::bail!("usage: msg <group_id> <text...>");
            }
            let (encrypted, topic_id) =
                node.chat_manager
                    .send_group_chat(&group_id, &user_id, &display_name, text)?;
            let group_key = node
                .db
                .load_group_key(&group_id)?
                .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;
            let bytes = encode_gossip_message(&GossipMessage::EncryptedChat {
                group_key,
                encrypted,
            });
            if let Some(gm) = &node.gossip_manager {
                gm.lock().await.broadcast(topic_id, bytes).await?;
            }
            node.notifier.notify_chat(&format!("group/{group_id}/chat"));
            tracing::info!(%group_id, "group chat sent + broadcast");
        }
        "checkin" => {
            let mut a = rest.splitn(2, char::is_whitespace);
            let group_id = a.next().unwrap_or("").to_string();
            let stage = a.next().unwrap_or("").trim();
            if group_id.is_empty() || stage.is_empty() {
                anyhow::bail!("usage: checkin <group_id> <stage_id>");
            }
            let encrypted = node
                .group_manager
                .check_in(&group_id, &user_id, Some(stage), None)
                .await?;
            let group_key = node
                .db
                .load_group_key(&group_id)?
                .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;
            let bytes = encode_gossip_message(&GossipMessage::GroupUpdate {
                doc_id: format!("group/{group_id}"),
                encrypted,
                group_key,
            });
            if let Some(gm) = &node.gossip_manager {
                let topic = offbeat_core::topics::group_topic(&group_key, "state");
                gm.lock().await.broadcast(topic, bytes).await?;
            }
            node.notifier.notify_doc(&format!("group/{group_id}"));
            tracing::info!(%group_id, %stage, "checked in + broadcast");
        }
        "state" => {
            if rest.is_empty() {
                anyhow::bail!("usage: state <group_id>");
            }
            let st = node.group_manager.get_group_state(rest).await?;
            tracing::info!("group '{}' ({} members, {} pins)", st.name, st.members.len(), st.pins.len());
            for m in &st.members {
                tracing::info!(
                    "  member {} [{}] status={} stage={:?} loc={:?}",
                    m.display_name, m.user_id, m.status, m.stage_id, m.custom_location
                );
            }
            for p in &st.pins {
                tracing::info!("  pin {} @ {} (by {})", p.label, p.location, p.pinned_by);
            }
        }
        "chat" => {
            if rest.is_empty() {
                anyhow::bail!("usage: chat <group_id>");
            }
            let topic = format!("group/{rest}/chat");
            let msgs = node.chat_manager.get_history(&topic, 50, 0)?;
            tracing::info!("{} messages in {topic}", msgs.len());
            for m in msgs {
                tracing::info!("  [{}] {}: {}", m.timestamp, m.display_name, m.text);
            }
        }
        "groups" => {
            let rows = node.db.load_groups(festival_id)?;
            tracing::info!("{} group(s) for {festival_id}", rows.len());
            for (id, name, _key) in rows {
                tracing::info!("  {id}  {name}");
            }
        }
        "lineup" => {
            let doc_id = format!("festival/{festival_id}/state");
            let name = node
                .doc_manager
                .read_map_value(&doc_id, "name")
                .unwrap_or_default();
            let stages = node.doc_manager.read_nested_map_entries(&doc_id, "stages");
            let sets = node.doc_manager.read_nested_map_entries(&doc_id, "sets");
            tracing::info!(
                "festival '{}' name={:?} stages={} sets={}",
                festival_id,
                name,
                stages.len(),
                sets.len()
            );
        }
        "peers" => {
            if let Some(cm) = &node.connection_manager {
                let peers = cm.peer_snapshot();
                tracing::info!("{} known peer(s)", peers.len());
                for p in peers {
                    tracing::info!("  {} source={:?} gossip={:?}", p.endpoint_id, p.source, p.gossip_status);
                }
            }
        }
        "nudge" => {
            if let (Some(ble), Some(gm)) = (&node.ble_transport, &node.gossip_manager) {
                let targets: Vec<offbeat_core::EndpointId> = ble
                    .snapshot_peers()
                    .into_iter()
                    .filter_map(|p| p.verified_endpoint)
                    .collect();
                let n = targets.len();
                gm.lock().await.join_peers_all(targets).await;
                tracing::info!("nudged gossip join for {n} verified peer(s)");
            }
        }
        other => anyhow::bail!("unknown command: {other} (try 'help')"),
    }
    Ok(())
}

/// Encode the full encrypted state of a group doc and broadcast it as a
/// `GroupUpdate` on the group's state topic (best-effort).
async fn broadcast_group_state(node: &Arc<OffbeatNode>, group_id: &str, group_key: &[u8; 32]) {
    let doc_id = format!("group/{group_id}");
    node.doc_manager.get_or_create(&doc_id);
    let diff = match node.doc_manager.encode_diff(&doc_id, &[]) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!("broadcast_group_state: encode_diff failed: {e}");
            return;
        }
    };
    let encrypted = match offbeat_core::crypto::encrypt(group_key, &diff) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!("broadcast_group_state: encrypt failed: {e}");
            return;
        }
    };
    let bytes = encode_gossip_message(&GossipMessage::GroupUpdate {
        doc_id,
        encrypted,
        group_key: *group_key,
    });
    if let Some(gm) = &node.gossip_manager {
        let topic = offbeat_core::topics::group_topic(group_key, "state");
        let _ = gm.lock().await.broadcast(topic, bytes).await;
    }
}
