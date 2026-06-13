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

use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::gossip_manager::GossipMessage;
use crate::proto;

/// ALPN for the transient peer catch-up protocol.
pub const SYNC_ALPN: &[u8] = b"offbeat/sync/1";

/// Cap on a request frame (a doc id + state vector) — generous but bounded.
const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Cap on a response diff. A full festival doc is small; 8 MiB is ample.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Server side: answers a catch-up request on a bi-stream. The request is a
/// [`proto::RelayClientMessage`] carrying either an `SvExchange` (CRDT diff) or
/// a `ChatCatchup` (append-log history). One request → one response per stream:
/// `SvExchange` replies with the raw Yrs diff, `ChatCatchup` with an encoded
/// [`proto::ChatDiffResponse`]. The client knows which decoder to use because it
/// chose the request.
#[derive(Clone)]
pub struct SyncProtocol {
    doc_manager: Arc<DocManager>,
    db: Arc<Database>,
}

impl std::fmt::Debug for SyncProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncProtocol").finish()
    }
}

impl SyncProtocol {
    pub fn new(doc_manager: Arc<DocManager>, db: Arc<Database>) -> Self {
        Self { doc_manager, db }
    }

    /// Serve a chat-history request: messages on `topic` that the requester is
    /// missing per its per-writer state vector, wrapped as `Chat` gossip
    /// envelopes. We send them as plaintext over the QUIC link (TLS-encrypted,
    /// mutually authenticated, peer-to-peer between two trusted members) rather
    /// than re-encrypting — there is no untrusted relay on this path.
    fn build_chat_diff(&self, req: &proto::ChatCatchupRequest) -> proto::ChatDiffResponse {
        let limit = req.limit.clamp(1, 1000);
        let messages = self
            .db
            .get_chat_messages(&req.topic, limit, 0)
            .unwrap_or_default();
        let envelopes = messages
            .into_iter()
            .filter(|m| {
                let hwm = req.sv.get(&m.user_id).copied().unwrap_or(0);
                // Unsequenced (writer_seq == 0) messages are always included —
                // the requester's INSERT-OR-IGNORE makes that idempotent.
                m.writer_seq == 0 || m.writer_seq > hwm
            })
            .map(|m| proto::GossipEnvelope::from_gossip_message(&GossipMessage::Chat(m)))
            .collect();
        proto::ChatDiffResponse {
            topic: req.topic.clone(),
            messages: envelopes,
        }
    }
}

impl ProtocolHandler for SyncProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = connection.accept_bi().await?;

        let req_bytes = recv
            .read_to_end(MAX_REQUEST_BYTES)
            .await
            .map_err(AcceptError::from_err)?;
        let req = proto::RelayClientMessage::decode(req_bytes.as_slice())
            .map_err(AcceptError::from_err)?;

        // One response per request kind. A missing/unknown doc, unknown topic,
        // or unhandled message yields an empty response rather than tearing down
        // the connection — the requester simply learns nothing new.
        use proto::relay_client_message::Msg;
        let response: Vec<u8> = match req.msg {
            Some(Msg::SvExchange(sv_req)) => self
                .doc_manager
                .encode_diff(&sv_req.doc_id, &sv_req.sv)
                .unwrap_or_default(),
            Some(Msg::ChatCatchup(chat_req)) => self.build_chat_diff(&chat_req).encode_to_vec(),
            _ => Vec::new(),
        };

        send.write_all(&response)
            .await
            .map_err(AcceptError::from_err)?;
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

        let req = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::SvExchange(
                proto::SvExchangeRequest {
                    doc_id: doc_id.to_string(),
                    sv: sv.to_vec(),
                },
            )),
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
        topic: &str,
        sv: &crate::sync::ChatStateVector,
        limit: u32,
    ) -> anyhow::Result<Vec<proto::GossipEnvelope>> {
        let conn = self.endpoint.connect(self.peer.clone(), SYNC_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;

        let req = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::ChatCatchup(
                proto::ChatCatchupRequest {
                    topic: topic.to_string(),
                    sv: sv.writers.clone(),
                    limit,
                },
            )),
        };
        send.write_all(&req.encode_to_vec()).await?;
        send.finish()?;

        let resp_bytes = recv.read_to_end(MAX_RESPONSE_BYTES).await?;
        conn.close(0u32.into(), b"done");
        if resp_bytes.is_empty() {
            return Ok(vec![]);
        }
        let resp = proto::ChatDiffResponse::decode(resp_bytes.as_slice())?;
        Ok(resp.messages)
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

    fn populated_doc(doc_id: &str) -> (Arc<DocManager>, Arc<Database>) {
        let db = Arc::new(Database::new_in_memory().unwrap());
        let dm = Arc::new(DocManager::new(db.clone()));
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
        (dm, db)
    }

    fn chat_msg(id: &str, user: &str, text: &str, topic: &str, seq: u64) -> crate::types::ChatMessage {
        crate::types::ChatMessage {
            id: id.to_string(),
            user_id: user.to_string(),
            display_name: user.to_string(),
            text: text.to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T00:00:00Z".to_string(),
            writer_seq: seq,
        }
    }

    /// End-to-end over the real wire: two iroh endpoints on loopback, the server
    /// running the offbeat/sync/1 Router protocol, the client driving a real
    /// IrohSyncPeer sv_exchange and converging to the server's CRDT state.
    #[tokio::test]
    async fn two_node_sv_exchange_over_alpn_converges() {
        let doc_id = "festival/fest1/state";

        // Server: populated doc behind the sync protocol.
        let (server_doc, server_db) = populated_doc(doc_id);
        let server_ep = local_endpoint(vec![SYNC_ALPN.to_vec()]).await;
        let _router = iroh::protocol::Router::builder(server_ep.clone())
            .accept(SYNC_ALPN, SyncProtocol::new(server_doc.clone(), server_db))
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

    /// A peer that joined a chat *after* messages were posted must still receive
    /// the pre-join history over the offbeat/sync/1 ALPN — old messages must not
    /// be dropped. The server holds three messages; the client starts empty and
    /// catches them all up in one bi-stream exchange.
    #[tokio::test]
    async fn two_node_chat_catchup_over_alpn_backfills_history() {
        let topic = "group/abc123/chat";

        // Server: a DB with pre-existing chat history behind the sync protocol.
        let server_db = Arc::new(Database::new_in_memory().unwrap());
        let server_doc = Arc::new(DocManager::new(server_db.clone()));
        server_db.save_chat_message(&chat_msg("m1", "alice", "hello", topic, 1)).unwrap();
        server_db.save_chat_message(&chat_msg("m2", "alice", "world", topic, 2)).unwrap();
        server_db.save_chat_message(&chat_msg("m3", "bob", "hi all", topic, 1)).unwrap();

        let server_ep = local_endpoint(vec![SYNC_ALPN.to_vec()]).await;
        let _router = iroh::protocol::Router::builder(server_ep.clone())
            .accept(SYNC_ALPN, SyncProtocol::new(server_doc, server_db))
            .spawn();
        let server_addr = server_ep.addr();

        // Client: empty DB, no history, dials the server (no discovery).
        let client_db = Arc::new(Database::new_in_memory().unwrap());
        let client_doc = Arc::new(DocManager::new(client_db.clone()));
        let client_ep = local_endpoint(vec![]).await;
        assert_eq!(client_db.get_chat_messages(topic, 100, 0).unwrap().len(), 0);

        // Catch up from an empty state vector — we are missing everything.
        let sv = crate::sync::ChatStateVector::new();
        let peer = IrohSyncPeer::new(client_ep.clone(), server_addr, client_doc);
        let envelopes = peer.chat_catchup(topic, &sv, 100).await.unwrap();
        assert_eq!(envelopes.len(), 3, "server should serve all 3 history messages");

        // The caller (orchestrator) persists the served envelopes; do that here
        // and confirm the history landed verbatim.
        for env in &envelopes {
            if let Some(proto::gossip_envelope::Payload::Chat(chat)) = &env.payload {
                crate::chat::receive_festival_chat(&client_db, chat.clone().into()).unwrap();
            }
        }
        let mut got: Vec<String> = client_db
            .get_chat_messages(topic, 100, 0)
            .unwrap()
            .into_iter()
            .map(|m| m.text)
            .collect();
        got.sort();
        assert_eq!(got, vec!["hello", "hi all", "world"]);

        client_ep.close().await;
        server_ep.close().await;
    }
}
