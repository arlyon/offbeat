//! `offbeat/sync/1` — peer-served CRDT catch-up over an iroh ALPN bi-stream.
//!
//! Triggered by a gossip `NeighborUp`: gossip has already discovered and
//! holepunched a path to the neighbor, so we open a **transient** connection on
//! this ALPN (a separate QUIC connection — QUIC binds one ALPN per connection,
//! so we can't reuse gossip's), exchange a Yrs state vector, apply the diff, and
//! close. Live updates keep flowing over gossip.
//!
//! Group resources encrypt both Yrs state vectors and returned diffs with the
//! group key, proving possession without exposing private state to unrelated
//! festival peers. Festival resources are different: a peer can only serve the
//! latest festival-authority-signed full checkpoint it has verified and
//! persisted. Both paths apply through [`crate::sync::SyncOrchestrator`] so
//! direct catch-up uses the same trust, persistence, and watcher lifecycle.

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
const MAX_CHAT_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_CHAT_CATCHUP_MESSAGES: u32 = 500;

/// Server side: answers a catch-up request on a bi-stream. The request is a
/// [`proto::RelayClientMessage`] carrying either an `SvExchange` (CRDT diff) or
/// a `ChatCatchup` (append-log history). One request → one response per stream:
/// `SvExchange` replies with an authority envelope for festival state or an
/// AES-GCM encrypted Yrs diff for group state. `ChatCatchup` replies with an
/// encoded [`proto::ChatDiffResponse`]. The client knows which decoder to use
/// because it chose the request.
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

    fn build_festival_checkpoint(&self, doc_id: &str) -> Vec<u8> {
        let Ok(Some(checkpoint)) = self.db.load_latest_festival_checkpoint(doc_id, 2) else {
            return Vec::new();
        };
        proto::GossipEnvelope::from_gossip_message(&GossipMessage::FestivalUpdate {
            doc_id: checkpoint.doc_id,
            kind: checkpoint.kind,
            authority_seq: checkpoint.authority_seq,
            signed_update: checkpoint.signed_update,
        })
        .encode_to_vec()
    }

    fn build_group_diff(&self, doc_id: &str, encrypted_sv: &[u8]) -> Vec<u8> {
        let Some(group_id) = doc_id
            .strip_prefix("group/")
            .and_then(|value| value.strip_suffix("/state"))
            .filter(|value| !value.is_empty() && !value.contains('/'))
        else {
            return Vec::new();
        };
        let Ok(Some(group_key)) = self.db.load_group_key(group_id) else {
            return Vec::new();
        };
        if crate::crypto::group_id_from_key(&group_key) != group_id {
            return Vec::new();
        }
        let Ok(sv) = crate::crypto::decrypt(&group_key, encrypted_sv) else {
            return Vec::new();
        };
        let Ok(diff) = self.doc_manager.encode_diff(doc_id, &sv) else {
            return Vec::new();
        };
        crate::crypto::encrypt(&group_key, &diff).unwrap_or_default()
    }

    /// Serve public chat history. Encrypted group-chat catch-up needs a
    /// possession-proof request and is intentionally disabled until that
    /// protocol is wired; endpoint identity alone is not group membership.
    fn build_chat_diff(&self, req: &proto::ChatCatchupRequest) -> proto::ChatDiffResponse {
        if req.topic.starts_with("group/") {
            return proto::ChatDiffResponse {
                topic: req.topic.clone(),
                messages: Vec::new(),
            };
        }
        let limit = req.limit.clamp(1, 1000);
        let messages = self
            .db
            .get_messages_since_heads(&req.topic, &req.sv, &req.head_ids, limit)
            .unwrap_or_default();
        let mut response = proto::ChatDiffResponse {
            topic: req.topic.clone(),
            messages: Vec::new(),
        };
        let mut included_proofs = std::collections::HashSet::new();
        for message in messages {
            let page_start = response.messages.len();
            if let Ok(writer_key) = <[u8; 32]>::try_from(message.writer_key.as_slice()) {
                let writer_id = message.writer_id();
                if included_proofs.insert(writer_id)
                    && let Ok(Some(proof)) = self.db.get_chat_author_proof(&writer_key)
                {
                    response
                        .messages
                        .push(proto::GossipEnvelope::from_gossip_message(
                            &GossipMessage::ChatAuthorProof {
                                writer_key: proof.writer_key,
                                attestation_message: proof.attestation_message,
                                attestation_signature: proof.attestation_signature,
                                issuer: proof.issuer,
                            },
                        ));
                }
            }
            response
                .messages
                .push(proto::GossipEnvelope::from_gossip_message(
                    &GossipMessage::Chat(message),
                ));
            if response.encoded_len() > MAX_CHAT_RESPONSE_BYTES {
                response.messages.truncate(page_start);
                break;
            }
        }
        response
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
            Some(Msg::SvExchange(sv_req)) if sv_req.doc_id.starts_with("festival/") => {
                self.build_festival_checkpoint(&sv_req.doc_id)
            }
            Some(Msg::SvExchange(sv_req)) if sv_req.doc_id.starts_with("group/") => {
                self.build_group_diff(&sv_req.doc_id, &sv_req.sv)
            }
            Some(Msg::SvExchange(_)) => Vec::new(),
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
    #[allow(dead_code)]
    doc_manager: Arc<DocManager>,
    sync_orchestrator: Option<Arc<crate::sync::SyncOrchestrator>>,
}

impl IrohSyncPeer {
    /// `peer` accepts a bare `EndpointId` (address resolved via discovery — what
    /// gossip provides in production) or a full `EndpointAddr` with direct
    /// addresses (used by loopback tests and BLE-seeded paths).
    pub fn new(
        endpoint: iroh::Endpoint,
        peer: impl Into<iroh::EndpointAddr>,
        doc_manager: Arc<DocManager>,
        sync_orchestrator: Option<Arc<crate::sync::SyncOrchestrator>>,
    ) -> Self {
        Self {
            endpoint,
            peer: peer.into(),
            doc_manager,
            sync_orchestrator,
        }
    }
}

impl crate::sync::PeerConnection for IrohSyncPeer {
    async fn subscribe(&self, _topics: Vec<String>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> anyhow::Result<()> {
        let request_sv = if doc_id.starts_with("festival/") {
            sv.to_vec()
        } else if doc_id.starts_with("group/") {
            let orchestrator = self
                .sync_orchestrator
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("group peer sync requires a sync orchestrator"))?;
            let group_key = orchestrator.group_key_for_doc(doc_id)?;
            crate::crypto::encrypt(&group_key, sv)?
        } else {
            anyhow::bail!("unsupported CRDT document ID {doc_id}");
        };

        let conn = self.endpoint.connect(self.peer.clone(), SYNC_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;

        let req = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::SvExchange(
                proto::SvExchangeRequest {
                    doc_id: doc_id.to_string(),
                    sv: request_sv,
                },
            )),
        };
        send.write_all(&req.encode_to_vec()).await?;
        send.finish()?;

        let diff = recv.read_to_end(MAX_RESPONSE_BYTES).await?;
        if !diff.is_empty() {
            if doc_id.starts_with("festival/") {
                let envelope = proto::GossipEnvelope::decode(diff.as_slice())?;
                let orchestrator = self.sync_orchestrator.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("festival peer sync requires a sync orchestrator")
                })?;
                orchestrator
                    .handle_incoming_envelope(doc_id, &envelope)
                    .await?;
            } else {
                let orchestrator = self.sync_orchestrator.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("group peer sync requires a sync orchestrator")
                })?;
                orchestrator.apply_encrypted_group_diff(doc_id, &diff)?;
            }
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
        if topic.starts_with("group/") {
            return Ok(Vec::new());
        }
        let effective_limit = limit.clamp(1, MAX_CHAT_CATCHUP_MESSAGES);
        let conn = self.endpoint.connect(self.peer.clone(), SYNC_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;

        let req = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::ChatCatchup(
                proto::ChatCatchupRequest {
                    topic: topic.to_string(),
                    sv: sv.sequences(),
                    limit: effective_limit,
                    head_ids: sv.head_ids(),
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
        if resp_bytes.len() > MAX_CHAT_RESPONSE_BYTES {
            anyhow::bail!("chat catch-up response exceeds byte budget");
        }
        let resp = proto::ChatDiffResponse::decode(resp_bytes.as_slice())?;
        let chat_count = resp
            .messages
            .iter()
            .filter(|envelope| {
                matches!(
                    envelope.payload.as_ref(),
                    Some(proto::gossip_envelope::Payload::Chat(_))
                )
            })
            .count();
        if chat_count > effective_limit as usize {
            anyhow::bail!("chat catch-up response exceeds requested message count");
        }
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
    use crate::notifier::ResourceNotifier;
    use crate::resource::ResourceRegistry;
    use crate::sync::{PeerConnection, SyncOrchestrator};
    use std::sync::RwLock;
    use yrs::updates::encoder::Encode;
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
                m.insert(&mut txn, "name", "The Crew");
            }
            d.transact()
                .encode_state_as_update_v1(&StateVector::default())
        };
        dm.apply_update(doc_id, &update).unwrap();
        (dm, db)
    }

    fn chat_msg(
        id: &str,
        user: &str,
        text: &str,
        topic: &str,
        seq: u64,
    ) -> crate::types::ChatMessage {
        crate::types::ChatMessage {
            id: id.to_string(),
            user_id: user.to_string(),
            display_name: user.to_string(),
            text: text.to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T00:00:00Z".to_string(),
            writer_seq: seq,
            logical_time: seq,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        }
    }

    fn signed_chat_msg(
        id: &str,
        user: &str,
        text: &str,
        topic: &str,
        seq: u64,
    ) -> crate::types::ChatMessage {
        let seed = *blake3::hash(user.as_bytes()).as_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let mut message = chat_msg(
            id,
            &crate::auth::get_user_id(&signing_key),
            text,
            topic,
            seq,
        );
        message.display_name = user.to_string();
        if let Some(channel) = topic
            .rsplit('/')
            .next()
            .filter(|channel| *channel != "campsite")
        {
            message.stage_id = Some(channel.to_string());
        }
        crate::signing::sign_public_chat_message(&signing_key, &mut message).unwrap();
        message
    }

    /// End-to-end over the real wire: two iroh endpoints on loopback, the server
    /// running the offbeat/sync/1 Router protocol, the client driving a real
    /// IrohSyncPeer sv_exchange and converging to the server's CRDT state.
    #[tokio::test]
    async fn two_node_sv_exchange_over_alpn_converges() {
        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let doc_id = format!("group/{group_id}/state");

        // Server: populated doc and matching group credential.
        let (server_doc, server_db) = populated_doc(&doc_id);
        server_db
            .save_group(&group_id, "fest-1", "The Crew", &group_key)
            .unwrap();
        let server_ep = local_endpoint(vec![SYNC_ALPN.to_vec()]).await;
        let _router = iroh::protocol::Router::builder(server_ep.clone())
            .accept(SYNC_ALPN, SyncProtocol::new(server_doc.clone(), server_db))
            .spawn();
        let server_addr = server_ep.addr();

        // Client: credential persisted, empty doc, and a normal watcher.
        let client_db = Arc::new(Database::new_in_memory().unwrap());
        client_db
            .save_group(&group_id, "fest-1", "", &group_key)
            .unwrap();
        let client_doc = Arc::new(DocManager::new(client_db.clone()));
        let notifier = Arc::new(ResourceNotifier::new());
        let mut state_rx = notifier.watch_doc(&doc_id);
        let chat_manager = Arc::new(crate::chat::ChatManager::new(
            client_db.clone(),
            client_doc.clone(),
        ));
        let orchestrator = Arc::new(SyncOrchestrator::new(
            Arc::new(RwLock::new(ResourceRegistry::new())),
            client_doc.clone(),
            chat_manager,
            client_db.clone(),
            notifier,
        ));
        orchestrator.cache_group_key(&group_id, group_key);
        let client_ep = local_endpoint(vec![]).await;

        client_doc.get_or_create(&doc_id);
        assert_eq!(client_doc.read_map_value(&doc_id, "stage"), None);

        let sv = client_doc.get_state_vector(&doc_id).unwrap();
        let peer = IrohSyncPeer::new(
            client_ep.clone(),
            server_addr,
            client_doc.clone(),
            Some(orchestrator),
        );
        peer.sv_exchange(&doc_id, &sv).await.unwrap();

        assert_eq!(
            client_doc.read_map_value(&doc_id, "stage").as_deref(),
            Some("main")
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), state_rx.changed())
            .await
            .expect("group watcher should be notified")
            .unwrap();
        assert_eq!(client_db.load_groups("fest-1").unwrap()[0].1, "The Crew");
        let reloaded = DocManager::new(client_db);
        assert_eq!(
            reloaded.read_map_value(&doc_id, "stage").as_deref(),
            Some("main")
        );

        client_ep.close().await;
        server_ep.close().await;
    }

    #[test]
    fn group_diff_requires_the_matching_group_key() {
        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let doc_id = format!("group/{group_id}/state");
        let (doc_manager, db) = populated_doc(&doc_id);
        db.save_group(&group_id, "fest-1", "The Crew", &group_key)
            .unwrap();
        let protocol = SyncProtocol::new(doc_manager, db);
        let raw_sv = StateVector::default().encode_v1();

        assert!(protocol.build_group_diff(&doc_id, &raw_sv).is_empty());
        let wrong_key = crate::crypto::generate_group_key();
        let wrong_sv = crate::crypto::encrypt(&wrong_key, &raw_sv).unwrap();
        assert!(protocol.build_group_diff(&doc_id, &wrong_sv).is_empty());
    }

    #[tokio::test]
    async fn two_node_festival_catchup_verifies_signed_checkpoint() {
        let doc_id = "festival/fest1/state";
        let signing_key = crate::signing::generate_signing_key();
        let public_key = signing_key.verifying_key().to_bytes();

        let source = Doc::new();
        let map = source.get_or_insert_map("root");
        map.insert(&mut source.transact_mut(), "stage", "main");
        let update = source
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let signature =
            crate::signing::sign_festival_update(&signing_key, doc_id, 2, 7, &update).unwrap();

        let server_db = Arc::new(Database::new_in_memory().unwrap());
        server_db
            .save_verified_festival_update(&crate::types::VerifiedFestivalUpdate {
                doc_id: doc_id.to_string(),
                kind: 2,
                authority_seq: 7,
                signed_update: crate::types::SignedUpdate {
                    update: update.clone(),
                    author: "festival-do".to_string(),
                    signature,
                },
            })
            .unwrap();
        let server_doc = Arc::new(DocManager::new(server_db.clone()));
        let server_ep = local_endpoint(vec![SYNC_ALPN.to_vec()]).await;
        let _router = iroh::protocol::Router::builder(server_ep.clone())
            .accept(SYNC_ALPN, SyncProtocol::new(server_doc, server_db))
            .spawn();

        let client_db = Arc::new(Database::new_in_memory().unwrap());
        let client_doc = Arc::new(DocManager::new(client_db.clone()));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(
            client_db.clone(),
            client_doc.clone(),
        ));
        let orchestrator = Arc::new(SyncOrchestrator::new(
            Arc::new(RwLock::new(ResourceRegistry::new())),
            client_doc.clone(),
            chat_manager,
            client_db.clone(),
            Arc::new(ResourceNotifier::new()),
        ));
        orchestrator.set_festival_public_key("fest1", public_key);
        let client_ep = local_endpoint(vec![]).await;
        let peer = IrohSyncPeer::new(
            client_ep.clone(),
            server_ep.addr(),
            client_doc.clone(),
            Some(orchestrator),
        );

        let sv = client_doc.get_state_vector(doc_id).unwrap();
        peer.sv_exchange(doc_id, &sv).await.unwrap();
        assert_eq!(
            client_doc.read_map_value(doc_id, "stage").as_deref(),
            Some("main")
        );
        assert_eq!(client_db.highest_verified_festival_seq(doc_id).unwrap(), 7);

        client_ep.close().await;
        server_ep.close().await;
    }

    /// A peer that joined a chat *after* messages were posted must still receive
    /// the pre-join history over the offbeat/sync/1 ALPN — old messages must not
    /// be dropped. The server holds three messages; the client starts empty and
    /// catches them all up in one bi-stream exchange.
    #[tokio::test]
    async fn two_node_chat_catchup_over_alpn_backfills_history() {
        let topic = "festival/fest-1/chat/main";

        // Server: a DB with pre-existing chat history behind the sync protocol.
        let server_db = Arc::new(Database::new_in_memory().unwrap());
        let server_doc = Arc::new(DocManager::new(server_db.clone()));
        server_db
            .save_chat_message(&signed_chat_msg("m1", "alice", "hello", topic, 1))
            .unwrap();
        server_db
            .save_chat_message(&signed_chat_msg("m2", "alice", "world", topic, 2))
            .unwrap();
        server_db
            .save_chat_message(&signed_chat_msg("m3", "bob", "hi all", topic, 1))
            .unwrap();

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
        let peer = IrohSyncPeer::new(client_ep.clone(), server_addr, client_doc, None);
        let envelopes = peer.chat_catchup(topic, &sv, 100).await.unwrap();
        assert_eq!(
            envelopes.len(),
            3,
            "server should serve all 3 history messages"
        );

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

    #[test]
    fn equal_hwm_with_different_head_returns_conflicting_variant() {
        let topic = "festival/fest-1/chat/main";
        let db = Arc::new(Database::new_in_memory().unwrap());
        db.save_chat_message(&chat_msg("alice-1", "alice", "hello", topic, 1))
            .unwrap();
        let protocol = SyncProtocol::new(Arc::new(DocManager::new(db.clone())), db);
        let response = protocol.build_chat_diff(&proto::ChatCatchupRequest {
            topic: topic.to_string(),
            sv: std::collections::HashMap::from([("alice".to_string(), 1)]),
            limit: 50,
            head_ids: std::collections::HashMap::from([(
                "alice".to_string(),
                "other-variant".to_string(),
            )]),
        });

        assert_eq!(response.messages.len(), 1);
        let Some(proto::gossip_envelope::Payload::Chat(message)) =
            response.messages[0].payload.as_ref()
        else {
            panic!("expected chat message");
        };
        assert_eq!(message.id, "alice-1");
    }

    #[test]
    fn chat_diff_producer_enforces_encoded_byte_budget() {
        let topic = "festival/fest-1/chat/main";
        let db = Arc::new(Database::new_in_memory().unwrap());
        for sequence in 1..=20 {
            db.save_chat_message(&chat_msg(
                &format!("m{sequence}"),
                "alice",
                &"x".repeat(64 * 1024),
                topic,
                sequence,
            ))
            .unwrap();
        }
        let protocol = SyncProtocol::new(Arc::new(DocManager::new(db.clone())), db);
        let response = protocol.build_chat_diff(&proto::ChatCatchupRequest {
            topic: topic.to_string(),
            sv: Default::default(),
            limit: 1000,
            head_ids: Default::default(),
        });

        assert!(response.encoded_len() <= MAX_CHAT_RESPONSE_BYTES);
        assert!(!response.messages.is_empty());
        assert!(response.messages.len() < 20);
    }

    #[test]
    fn group_chat_history_is_not_served_without_possession_proof() {
        let topic = "group/secret/chat";
        let db = Arc::new(Database::new_in_memory().unwrap());
        db.save_chat_message(&chat_msg("m1", "alice", "private", topic, 1))
            .unwrap();
        let protocol = SyncProtocol::new(Arc::new(DocManager::new(db.clone())), db);
        let response = protocol.build_chat_diff(&proto::ChatCatchupRequest {
            topic: topic.to_string(),
            sv: Default::default(),
            limit: 50,
            head_ids: Default::default(),
        });

        assert!(response.messages.is_empty());
    }
}
