//! `offbeat/sync/1` — peer-served CRDT catch-up over an iroh ALPN bi-stream.
//!
//! Triggered by a gossip `NeighborUp`: gossip has already discovered and
//! holepunched a path to the neighbor, so we open a **transient** connection on
//! this ALPN (a separate QUIC connection — QUIC binds one ALPN per connection,
//! so we can't reuse gossip's), exchange a Yrs state vector, apply the diff, and
//! close. Live updates keep flowing over gossip.
//!
//! The exchange mirrors the Festival DO's `sv_exchange` (a raw Yrs diff via
//! [`DocManager::encode_diff`]). Peer-trust hardening — verifying that a
//! relayed festival diff descends from the signed origin — is Phase 5 (cert
//! chain); today, like the DO path, the diff is applied directly.

use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use prost::Message as _;

use crate::doc_manager::DocManager;
use crate::proto;

/// ALPN for the transient peer catch-up protocol.
pub const SYNC_ALPN: &[u8] = b"offbeat/sync/1";

/// Cap on a request frame (a doc id + state vector) — generous but bounded.
const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Cap on a response diff. A full festival doc is small; 8 MiB is ample.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Server side: answers an `sv_exchange` by returning the Yrs diff since the
/// requester's state vector. One request → one response per bi-stream.
#[derive(Clone)]
pub struct SyncProtocol {
    doc_manager: Arc<DocManager>,
}

impl std::fmt::Debug for SyncProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncProtocol").finish()
    }
}

impl SyncProtocol {
    pub fn new(doc_manager: Arc<DocManager>) -> Self {
        Self { doc_manager }
    }
}

impl ProtocolHandler for SyncProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = connection.accept_bi().await?;

        let req_bytes = recv
            .read_to_end(MAX_REQUEST_BYTES)
            .await
            .map_err(AcceptError::from_err)?;
        let req =
            proto::SvExchangeRequest::decode(req_bytes.as_slice()).map_err(AcceptError::from_err)?;

        // Diff of our copy since the requester's state vector. A missing/unknown
        // doc or decode error yields an empty diff rather than tearing down the
        // connection — the requester simply learns nothing new.
        let diff = self
            .doc_manager
            .encode_diff(&req.doc_id, &req.sv)
            .unwrap_or_default();

        send.write_all(&diff).await.map_err(AcceptError::from_err)?;
        send.finish().map_err(AcceptError::from_err)?;
        // Keep the connection until the peer has read the response.
        connection.closed().await;
        Ok(())
    }
}

/// Client side: a transient [`PeerConnection`](crate::sync::PeerConnection) that
/// dials a single neighbor over [`SYNC_ALPN`], runs one exchange, and closes.
///
/// `subscribe`/`broadcast` are no-ops here — membership and live fan-out are
/// gossip's job; this peer only does point-in-time catch-up.
pub struct IrohSyncPeer {
    endpoint: iroh::Endpoint,
    peer: iroh::EndpointAddr,
    doc_manager: Arc<DocManager>,
}

impl IrohSyncPeer {
    /// `peer` accepts a bare `EndpointId` (address resolved via discovery — what
    /// gossip provides in production) or a full `EndpointAddr` with direct
    /// addresses (used by loopback tests and BLE-seeded paths).
    pub fn new(
        endpoint: iroh::Endpoint,
        peer: impl Into<iroh::EndpointAddr>,
        doc_manager: Arc<DocManager>,
    ) -> Self {
        Self {
            endpoint,
            peer: peer.into(),
            doc_manager,
        }
    }
}

impl crate::sync::PeerConnection for IrohSyncPeer {
    async fn subscribe(&self, _topics: Vec<String>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> anyhow::Result<()> {
        let conn = self.endpoint.connect(self.peer.clone(), SYNC_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;

        let req = proto::SvExchangeRequest {
            doc_id: doc_id.to_string(),
            sv: sv.to_vec(),
        };
        send.write_all(&req.encode_to_vec()).await?;
        send.finish()?;

        let diff = recv.read_to_end(MAX_RESPONSE_BYTES).await?;
        if !diff.is_empty() {
            self.doc_manager.apply_update(doc_id, &diff)?;
        }
        conn.close(0u32.into(), b"done");
        Ok(())
    }

    async fn chat_catchup(
        &self,
        _topic: &str,
        _sv: &crate::sync::ChatStateVector,
        _limit: u32,
    ) -> anyhow::Result<()> {
        // Chat catch-up over ALPN is a follow-up; CRDT sv_exchange is the
        // NeighborUp anti-entropy path. Chat still syncs via the relay/gossip.
        Ok(())
    }

    async fn broadcast(&self, _topic: &str, _data: &[u8]) -> anyhow::Result<()> {
        anyhow::bail!("broadcast is not supported on a transient sync peer; use gossip")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::sync::PeerConnection;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    /// A relay-disabled, discovery-free endpoint bound on loopback. Peers reach
    /// each other only via explicit `EndpointAddr`s — no external infra.
    async fn local_endpoint(alpns: Vec<Vec<u8>>) -> iroh::Endpoint {
        iroh::Endpoint::builder(iroh::endpoint::presets::N0)
            .relay_mode(iroh::RelayMode::Disabled)
            .alpns(alpns)
            .bind()
            .await
            .expect("bind loopback endpoint")
    }

    fn populated_doc(doc_id: &str) -> Arc<DocManager> {
        let db = Arc::new(Database::new_in_memory().unwrap());
        let dm = Arc::new(DocManager::new(db));
        dm.get_or_create(doc_id);
        let update = {
            let d = Doc::new();
            let m = d.get_or_insert_map("root");
            {
                let mut txn = d.transact_mut();
                m.insert(&mut txn, "stage", "main");
            }
            d.transact()
                .encode_state_as_update_v1(&StateVector::default())
        };
        dm.apply_update(doc_id, &update).unwrap();
        dm
    }

    /// End-to-end over the real wire: two iroh endpoints on loopback, the server
    /// running the offbeat/sync/1 Router protocol, the client driving a real
    /// IrohSyncPeer sv_exchange and converging to the server's CRDT state.
    #[tokio::test]
    async fn two_node_sv_exchange_over_alpn_converges() {
        let doc_id = "festival/fest1/state";

        // Server: populated doc behind the sync protocol.
        let server_doc = populated_doc(doc_id);
        let server_ep = local_endpoint(vec![SYNC_ALPN.to_vec()]).await;
        let _router = iroh::protocol::Router::builder(server_ep.clone())
            .accept(SYNC_ALPN, SyncProtocol::new(server_doc.clone()))
            .spawn();
        let server_addr = server_ep.addr();

        // Client: empty doc, dials the server's full address (no discovery).
        let client_db = Arc::new(Database::new_in_memory().unwrap());
        let client_doc = Arc::new(DocManager::new(client_db));
        let client_ep = local_endpoint(vec![]).await;

        client_doc.get_or_create(doc_id);
        assert_eq!(client_doc.read_map_value(doc_id, "stage"), None);

        let sv = client_doc.get_state_vector(doc_id).unwrap();
        let peer = IrohSyncPeer::new(client_ep.clone(), server_addr, client_doc.clone());
        peer.sv_exchange(doc_id, &sv).await.unwrap();

        // Converged over the actual ALPN round-trip.
        assert_eq!(
            client_doc.read_map_value(doc_id, "stage"),
            Some("main".to_string())
        );

        client_ep.close().await;
        server_ep.close().await;
    }
}
