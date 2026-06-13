use std::path::PathBuf;
use std::sync::Arc;
use clap::Parser;
use offbeat_core::OffbeatNode;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug)]
#[command(author, version, about = "Offbeat Headless Node", long_about = None)]
struct Args {
    /// Path to the database file. If not provided, a temporary one will be used.
    #[arg(short, long)]
    db: Option<PathBuf>,

    /// Festival ID to join at startup.
    #[arg(short, long, default_value = "rockwerchter2026")]
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

    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::new(&args.logs))
        .init();

    // Database path
    let db_path = match args.db {
        Some(p) => p,
        None => {
            let temp = tempfile::NamedTempFile::new()?;
            let path = temp.path().to_path_buf();
            // Ensure temp file isn't deleted immediately if we want it to survive
            // but for a headless test node, temp is usually fine.
            path
        }
    };

    tracing::info!("starting offbeat node at {}", db_path.display());

    // Create the node with full networking (BLE + Gossip + Iroh)
    let node = OffbeatNode::new_with_networking(&db_path).await?;
    let node = Arc::new(node);

    // Bootstrap the node: start background discovery and sync tasks
    let handles = node.spawn_ble_sync();
    tracing::info!("BLE stack and background sync tasks spawned");

    // Join the specified festival topic
    {
        let festival_id = args.festival;
        tracing::info!("auto-joining festival: {}", festival_id);
        
        // Register the festival state resource so the subscription manager picks it up.
        // We use the ADMIN_ROOT_PUBKEY as the public key so this node can verify 
        // updates signed by the server's root key.
        let mut reg = node.resource_registry.write().expect("lock poisoned");
        reg.register_festival(&festival_id, offbeat_core::cert::ADMIN_ROOT_PUBKEY);
    }

    // Keep the process alive
    tracing::info!("node is running. ctrl-c to exit.");
    
    // Periodically print status to terminal
    let status_node = node.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            if let Some(cm) = &status_node.connection_manager {
                let peers = cm.peer_snapshot();
                let active = peers.iter().filter(|p| {
                    matches!(p.gossip_status, offbeat_core::connection_manager::GossipStatus::Active)
                }).count();
                tracing::info!("status: {} known peers, {} active in mesh", peers.len(), active);
                for p in peers {
                    tracing::debug!("  - peer {}: {:?}", p.endpoint_id, p.gossip_status);
                }
            }
        }
    });

    // Wait for ctrl-c
    tokio::signal::ctrl_c().await?;
    
    tracing::info!("shutting down...");
    for handle in handles {
        handle.abort();
    }

    Ok(())
}
