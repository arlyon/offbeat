use std::sync::Arc;

use base64::Engine as _;
use iroh_gossip::proto::TopicId;
use yrs::Map;

use crate::crypto;
use crate::db::Database;
use crate::doc_manager::{self, DocManager};
use crate::topics;
use crate::types::GroupPin;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

pub struct GroupCreateResult {
    pub group_id: String,
    pub festival_id: String,
    pub invite_payload: String,
    pub topic_state: TopicId,
    pub topic_chat: TopicId,
}

pub struct GroupJoinResult {
    pub group_id: String,
    pub festival_id: String,
    pub group_key: [u8; 32],
    pub topic_state: TopicId,
    pub topic_chat: TopicId,
}

pub struct GroupMember {
    pub user_id: String,
    pub display_name: String,
    pub status: String,
    pub stage_id: Option<String>,
    pub custom_location: Option<String>,
}

pub struct GroupState {
    pub name: String,
    pub members: Vec<GroupMember>,
    pub pins: Vec<GroupPin>,
}

// ---------------------------------------------------------------------------
// GroupManager
// ---------------------------------------------------------------------------

pub struct GroupManager {
    db: Arc<Database>,
    doc_manager: Arc<DocManager>,
}

impl GroupManager {
    pub fn new(db: Arc<Database>, doc_manager: Arc<DocManager>) -> Self {
        Self { db, doc_manager }
    }

    // -----------------------------------------------------------------------
    // create_group
    // -----------------------------------------------------------------------

    pub async fn create_group(
        &self,
        festival_id: &str,
        name: &str,
        user_id: &str,
        display_name: &str,
    ) -> anyhow::Result<GroupCreateResult> {
        let key = crypto::generate_group_key();
        let group_id = crypto::group_id_from_key(&key);

        self.db.save_group(&group_id, festival_id, name, &key)?;

        let doc_id = format!("group/{group_id}");

        // Set scalar fields + add member in a single mutation
        let name = name.to_string();
        let festival_id_s = festival_id.to_string();
        let user_id_s = user_id.to_string();
        let display_name_s = display_name.to_string();
        self.doc_manager.mutate(&doc_id, &["root", "members"], |maps, txn| {
            let (root, members) = (&maps[0], &maps[1]);
            root.insert(txn, "name", name);
            root.insert(txn, "festival_id", festival_id_s);
            root.insert(txn, "created_by", user_id_s.clone());

            let member = doc_manager::get_or_init_map(members, txn, &user_id_s);
            member.insert(txn, "displayName", display_name_s);
            member.insert(txn, "status", "active");
        })?;

        let b64key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
        let invite_payload = format!("offbeat://group/{festival_id}/{group_id}/{b64key}");

        let topic_state = topics::group_topic(&key, "state");
        let topic_chat = topics::group_topic(&key, "chat");

        Ok(GroupCreateResult {
            group_id,
            festival_id: festival_id.to_string(),
            invite_payload,
            topic_state,
            topic_chat,
        })
    }

    // -----------------------------------------------------------------------
    // join_group
    // -----------------------------------------------------------------------

    pub async fn join_group(
        &self,
        invite_payload: &str,
        user_id: &str,
        display_name: &str,
    ) -> anyhow::Result<GroupJoinResult> {
        // Parse invite payload. Supports two formats:
        //   New: "offbeat://group/{festival_id}/{group_id}/{base64url_key}"
        //   Old: "offbeat://group/{group_id}/{base64url_key}"
        let stripped = invite_payload
            .strip_prefix("offbeat://group/")
            .ok_or_else(|| anyhow::anyhow!("invalid invite payload format"))?;

        let segments: Vec<&str> = stripped.split('/').collect();

        let (festival_id, group_id_from_invite, b64key) = match segments.len() {
            3 => {
                // New format: festival_id/group_id/key
                (segments[0].to_string(), segments[1], segments[2])
            }
            2 => {
                // Old format: group_id/key (backward compat)
                (String::new(), segments[0], segments[1])
            }
            _ => anyhow::bail!(
                "invite payload has unexpected number of segments: {}",
                segments.len()
            ),
        };

        let key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64key)
            .map_err(|e| anyhow::anyhow!("base64 decode invite key: {e}"))?;

        let group_key: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invite key must be 32 bytes"))?;

        // Verify the group_id matches the key
        let derived_id = crypto::group_id_from_key(&group_key);
        if derived_id != group_id_from_invite {
            anyhow::bail!(
                "invite group_id mismatch: expected {derived_id}, got {group_id_from_invite}"
            );
        }

        self.db
            .save_group(&derived_id, &festival_id, "", &group_key)?;

        let doc_id = format!("group/{derived_id}");

        let user_id_s = user_id.to_string();
        let display_name_s = display_name.to_string();
        self.doc_manager.mutate(&doc_id, &["members"], |maps, txn| {
            let member = doc_manager::get_or_init_map(&maps[0], txn, &user_id_s);
            member.insert(txn, "displayName", display_name_s);
            member.insert(txn, "status", "active");
        })?;

        let topic_state = topics::group_topic(&group_key, "state");
        let topic_chat = topics::group_topic(&group_key, "chat");

        Ok(GroupJoinResult {
            group_id: derived_id,
            festival_id,
            group_key,
            topic_state,
            topic_chat,
        })
    }

    // -----------------------------------------------------------------------
    // leave_group
    // -----------------------------------------------------------------------

    /// Remove self from the group. Returns the Yrs update bytes for the
    /// tombstone entry (so the caller can broadcast it), or `None` if the doc
    /// was not found.
    pub async fn leave_group(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let doc_id = format!("group/{group_id}");
        let user_id_s = user_id.to_string();
        let update = self.doc_manager.mutate(&doc_id, &["members"], |maps, txn| {
            maps[0].remove(txn, &user_id_s);
        })?;

        self.db.delete_group(group_id)?;

        Ok(Some(update))
    }

    // -----------------------------------------------------------------------
    // check_in
    // -----------------------------------------------------------------------

    /// Record the user's current location in the group Yrs doc. Returns the
    /// encrypted update bytes ready to publish over gossip.
    pub async fn check_in(
        &self,
        group_id: &str,
        user_id: &str,
        stage_id: Option<&str>,
        custom_location: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let doc_id = format!("group/{group_id}");
        let user_id_s = user_id.to_string();
        let stage_id_s = stage_id.map(String::from);
        let custom_loc_s = custom_location.map(String::from);
        let updated_at = now_rfc3339();

        let diff = self.doc_manager.mutate(&doc_id, &["members"], |maps, txn| {
            let member = doc_manager::get_or_init_map(&maps[0], txn, &user_id_s);
            member.insert(txn, "status", "active");
            member.insert(txn, "updatedAt", updated_at);
            if let Some(sid) = stage_id_s {
                member.insert(txn, "stageId", sid);
                member.remove(txn, "customLocation");
            } else if let Some(cl) = custom_loc_s {
                member.insert(txn, "customLocation", cl);
                member.remove(txn, "stageId");
            }
        })?;

        let encrypted = crypto::encrypt(&group_key, &diff)?;
        Ok(encrypted)
    }

    // -----------------------------------------------------------------------
    // update_stars
    // -----------------------------------------------------------------------

    pub async fn update_stars(
        &self,
        group_id: &str,
        user_id: &str,
        set_ids: Vec<String>,
    ) -> anyhow::Result<Vec<u8>> {
        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let doc_id = format!("group/{group_id}");
        let user_id_s = user_id.to_string();
        let diff = self.doc_manager.mutate(&doc_id, &["stars"], |maps, txn| {
            let user_stars = doc_manager::get_or_init_map(&maps[0], txn, &user_id_s);
            let old_keys: Vec<String> = user_stars.keys(txn).map(|k| k.to_string()).collect();
            for k in &old_keys {
                user_stars.remove(txn, k);
            }
            for set_id in &set_ids {
                user_stars.insert(txn, set_id.as_str(), true);
            }
        })?;

        let encrypted = crypto::encrypt(&group_key, &diff)?;
        Ok(encrypted)
    }

    // -----------------------------------------------------------------------
    // add_pin
    // -----------------------------------------------------------------------

    pub async fn add_pin(
        &self,
        group_id: &str,
        pin_id: &str,
        label: &str,
        location: &str,
        pinned_by: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let doc_id = format!("group/{group_id}");
        let pin_id_s = pin_id.to_string();
        let label_s = label.to_string();
        let location_s = location.to_string();
        let pinned_by_s = pinned_by.to_string();
        let created_at = now_rfc3339();

        let diff = self.doc_manager.mutate(&doc_id, &["pins"], |maps, txn| {
            let pin = doc_manager::get_or_init_map(&maps[0], txn, &pin_id_s);
            pin.insert(txn, "label", label_s);
            pin.insert(txn, "location", location_s);
            pin.insert(txn, "pinnedBy", pinned_by_s);
            pin.insert(txn, "createdAt", created_at);
        })?;

        let encrypted = crypto::encrypt(&group_key, &diff)?;
        Ok(encrypted)
    }

    // -----------------------------------------------------------------------
    // get_group_state
    // -----------------------------------------------------------------------

    pub async fn get_group_state(&self, group_id: &str) -> anyhow::Result<GroupState> {
        let doc_id = format!("group/{group_id}");
        let name = self
            .doc_manager
            .read_map_value(&doc_id, "name")
            .unwrap_or_default();

        let members_raw = self.doc_manager.read_nested_map_entries(&doc_id, "members");
        let pins_raw = self.doc_manager.read_nested_map_entries(&doc_id, "pins");

        let members = members_raw
            .into_iter()
            .map(|(uid, fields)| GroupMember {
                user_id: uid,
                display_name: doc_manager::any_str(&fields, "displayName")
                    .unwrap_or_default(),
                status: doc_manager::any_str(&fields, "status")
                    .unwrap_or_else(|| "active".to_string()),
                stage_id: doc_manager::any_str(&fields, "stageId"),
                custom_location: doc_manager::any_str(&fields, "customLocation"),
            })
            .collect();

        let pins = pins_raw
            .into_iter()
            .map(|(pid, fields)| GroupPin {
                id: pid,
                label: doc_manager::any_str(&fields, "label").unwrap_or_default(),
                location: doc_manager::any_str(&fields, "location").unwrap_or_default(),
                pinned_by: doc_manager::any_str(&fields, "pinnedBy").unwrap_or_default(),
                created_at: doc_manager::any_str(&fields, "createdAt").unwrap_or_default(),
            })
            .collect();

        Ok(GroupState {
            name,
            members,
            pins,
        })
    }

    // -----------------------------------------------------------------------
    // SV handshake helpers
    // -----------------------------------------------------------------------

    /// Build an encrypted `sync_request` payload containing the local Yrs
    /// state vector for the group doc. The caller should send this as a relay
    /// message so other peers can compute and return a targeted diff.
    pub async fn request_group_sync(&self, group_id: &str) -> anyhow::Result<Vec<u8>> {
        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let doc_id = format!("group/{group_id}");
        self.doc_manager.get_or_create(&doc_id);
        let sv_bytes = self.doc_manager.get_state_vector(&doc_id)?;

        // Encrypt the SV so it doesn't leak doc structure to bystanders.
        let encrypted_sv = crypto::encrypt(&group_key, &sv_bytes)?;
        Ok(encrypted_sv)
    }

    /// Given an encrypted remote state vector, compute the diff and return it
    /// encrypted so the requester can apply it to catch up.
    pub async fn handle_sync_request(
        &self,
        group_id: &str,
        remote_sv_encrypted: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let remote_sv = crypto::decrypt(&group_key, remote_sv_encrypted)?;

        let doc_id = format!("group/{group_id}");
        let diff = self.doc_manager.encode_diff(&doc_id, &remote_sv)?;

        let encrypted_diff = crypto::encrypt(&group_key, &diff)?;
        Ok(encrypted_diff)
    }

    // -----------------------------------------------------------------------
    // Read stars (for sync verification)
    // -----------------------------------------------------------------------

    /// Read the starred set IDs for a user from the group doc.
    pub fn read_user_stars(&self, group_id: &str, user_id: &str) -> Vec<String> {
        use yrs::{any::Any, Out};
        let doc_id = format!("group/{group_id}");
        self.doc_manager.read(&doc_id, &["stars"], |maps, txn| {
            match maps[0].get(txn, user_id) {
                Some(Out::YMap(user_stars)) => user_stars
                    .keys(txn)
                    .filter(|k| {
                        matches!(
                            user_stars.get(txn, k),
                            Some(Out::Any(Any::Bool(true)))
                        )
                    })
                    .map(|k| k.to_string())
                    .collect(),
                _ => vec![],
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Minimal RFC-3339-ish timestamp without pulling in chrono.
    format!("{secs}Z")
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth;

    fn make_manager() -> GroupManager {
        let db = Arc::new(Database::new_in_memory().expect("in-memory db"));
        let doc_manager = Arc::new(DocManager::new(db.clone()));
        GroupManager::new(db, doc_manager)
    }

    #[tokio::test]
    async fn test_create_group() {
        let gm = make_manager();
        let result = gm
            .create_group("fest-1", "Crew A", "user1", "Alice")
            .await
            .unwrap();

        assert_eq!(result.group_id.len(), 32);
        assert!(result.invite_payload.starts_with("offbeat://group/"));

        // DB entry exists
        let groups = gm.db.load_groups("fest-1").unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, result.group_id);

        // Yrs doc has name
        let doc_id = format!("group/{}", result.group_id);
        let name = gm.doc_manager.read_map_value(&doc_id, "name").unwrap();
        assert_eq!(name, "Crew A");

        // Member is in nested members map
        let member = gm
            .doc_manager
            .read_nested_map_entry(&doc_id, "members", "user1")
            .unwrap();
        assert_eq!(
            doc_manager::any_str(&member, "displayName"),
            Some("Alice".to_string())
        );
        assert_eq!(
            doc_manager::any_str(&member, "status"),
            Some("active".to_string())
        );
    }

    #[tokio::test]
    async fn test_join_group() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Squad", "user1", "Alice")
            .await
            .unwrap();

        let join = gm
            .join_group(&create.invite_payload, "user2", "Bob")
            .await
            .unwrap();

        assert_eq!(join.group_id, create.group_id);

        // Both members in the doc
        let doc_id = format!("group/{}", create.group_id);
        let m1 = gm
            .doc_manager
            .read_nested_map_entry(&doc_id, "members", "user1");
        let m2 = gm
            .doc_manager
            .read_nested_map_entry(&doc_id, "members", "user2");
        assert!(m1.is_some(), "user1 should be in doc");
        assert!(m2.is_some(), "user2 should be in doc");
    }

    #[tokio::test]
    async fn test_leave_group() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Crew", "user1", "Alice")
            .await
            .unwrap();

        let update = gm.leave_group(&create.group_id, "user1").await.unwrap();
        assert!(update.is_some());

        // DB entry removed
        let groups = gm.db.load_groups("fest-1").unwrap();
        assert!(groups.is_empty());
    }

    #[tokio::test]
    async fn test_leave_group_removes_member_key() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Crew", "user1", "Alice")
            .await
            .unwrap();

        // user1 is in the doc
        let doc_id = format!("group/{}", create.group_id);
        let val = gm
            .doc_manager
            .read_nested_map_entry(&doc_id, "members", "user1");
        assert!(val.is_some(), "member should exist before leave");

        gm.leave_group(&create.group_id, "user1").await.unwrap();

        // After leaving, the member should be gone
        let val = gm
            .doc_manager
            .read_nested_map_entry(&doc_id, "members", "user1");
        assert!(val.is_none(), "member should be removed after leave");
    }

    #[tokio::test]
    async fn test_check_in() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Crew", "user1", "Alice")
            .await
            .unwrap();

        let encrypted = gm
            .check_in(&create.group_id, "user1", Some("main-stage"), None)
            .await
            .unwrap();

        // Should decrypt to valid bytes
        let group_key = gm.db.load_group_key(&create.group_id).unwrap().unwrap();
        let plaintext = crypto::decrypt(&group_key, &encrypted).unwrap();
        // Valid Yrs update bytes (non-empty)
        assert!(!plaintext.is_empty());
    }

    #[tokio::test]
    async fn test_update_stars() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Crew", "user1", "Alice")
            .await
            .unwrap();

        let set_ids = vec!["set-a".to_string(), "set-b".to_string()];
        let encrypted = gm
            .update_stars(&create.group_id, "user1", set_ids.clone())
            .await
            .unwrap();

        // Decrypt and verify update bytes are non-empty
        let group_key = gm.db.load_group_key(&create.group_id).unwrap().unwrap();
        let plaintext = crypto::decrypt(&group_key, &encrypted).unwrap();
        assert!(!plaintext.is_empty());

        // Stars should be in the nested map
        let stars = gm.read_user_stars(&create.group_id, "user1");
        assert_eq!(stars.len(), 2);
        assert!(stars.contains(&"set-a".to_string()));
        assert!(stars.contains(&"set-b".to_string()));
    }

    #[tokio::test]
    async fn test_add_pin() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Crew", "user1", "Alice")
            .await
            .unwrap();

        let encrypted = gm
            .add_pin(
                &create.group_id,
                "pin-1",
                "Tent area",
                "53.5,-2.2",
                "user1",
            )
            .await
            .unwrap();

        let group_key = gm.db.load_group_key(&create.group_id).unwrap().unwrap();
        let plaintext = crypto::decrypt(&group_key, &encrypted).unwrap();
        assert!(!plaintext.is_empty());

        let doc_id = format!("group/{}", create.group_id);
        let pin = gm
            .doc_manager
            .read_nested_map_entry(&doc_id, "pins", "pin-1")
            .unwrap();
        assert_eq!(
            doc_manager::any_str(&pin, "label"),
            Some("Tent area".to_string())
        );
        assert_eq!(
            doc_manager::any_str(&pin, "pinnedBy"),
            Some("user1".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_group_state() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Crew", "user1", "Alice")
            .await
            .unwrap();
        let group_id = &create.group_id;

        // Join a second member
        gm.join_group(&create.invite_payload, "user2", "Bob")
            .await
            .unwrap();

        // user1 checks in
        gm.check_in(group_id, "user1", Some("main-stage"), None)
            .await
            .unwrap();

        // Add a pin
        gm.add_pin(group_id, "pin-42", "Meeting point", "0,0", "user1")
            .await
            .unwrap();

        let state = gm.get_group_state(group_id).await.unwrap();
        assert_eq!(state.name, "Crew");
        assert_eq!(state.members.len(), 2);
        assert_eq!(state.pins.len(), 1);
        assert_eq!(state.pins[0].label, "Meeting point");

        let alice = state
            .members
            .iter()
            .find(|m| m.user_id == "user1")
            .unwrap();
        assert_eq!(alice.stage_id.as_deref(), Some("main-stage"));
    }

    #[tokio::test]
    async fn test_invite_payload_roundtrip() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Roundtrip", "u1", "Alice")
            .await
            .unwrap();

        let join = gm
            .join_group(&create.invite_payload, "u2", "Bob")
            .await
            .unwrap();

        assert_eq!(join.group_id, create.group_id);
        assert_eq!(join.festival_id, "fest-1");
        assert_eq!(join.topic_state, create.topic_state);
        assert_eq!(join.topic_chat, create.topic_chat);
    }

    #[tokio::test]
    async fn test_invite_payload_contains_festival_id() {
        let gm = make_manager();
        let create = gm
            .create_group("wavelength26", "Crew", "u1", "Alice")
            .await
            .unwrap();

        // New format: offbeat://group/{festival_id}/{group_id}/{key}
        assert!(create
            .invite_payload
            .starts_with("offbeat://group/wavelength26/"));
        assert_eq!(create.festival_id, "wavelength26");

        // Should have 3 segments after "offbeat://group/"
        let stripped = create
            .invite_payload
            .strip_prefix("offbeat://group/")
            .unwrap();
        let segments: Vec<&str> = stripped.split('/').collect();
        assert_eq!(
            segments.len(),
            3,
            "new format should have 3 segments: festival_id/group_id/key"
        );
        assert_eq!(segments[0], "wavelength26");
        assert_eq!(segments[1], create.group_id);
    }

    #[tokio::test]
    async fn test_join_group_backward_compat_old_format() {
        let gm = make_manager();
        let create = gm
            .create_group("fest-1", "Compat", "u1", "Alice")
            .await
            .unwrap();

        // Build old format manually: offbeat://group/{group_id}/{key}
        let key = gm.db.load_group_key(&create.group_id).unwrap().unwrap();
        let b64key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
        let old_payload = format!("offbeat://group/{}/{}", create.group_id, b64key);

        // Create a fresh manager to join (simulate different device)
        let gm2 = make_manager();
        let join = gm2.join_group(&old_payload, "u2", "Bob").await.unwrap();

        assert_eq!(join.group_id, create.group_id);
        assert_eq!(join.festival_id, ""); // old format has no festival_id
    }

    #[test]
    fn test_auth_user_id_in_groups_context() {
        // Ensure auth and groups integrate: same signing key → same user ID.
        let db = Arc::new(Database::new_in_memory().unwrap());
        let key = auth::generate_or_load_identity(&db).unwrap();
        let id1 = auth::get_user_id(&key);
        let id2 = auth::get_user_id(&key);
        assert_eq!(id1, id2);
    }

    /// Full two-user integration test:
    ///
    /// 1. User A creates a group, sends a chat message, checks in at a stage,
    ///    and stars (likes) an event.
    /// 2. User B joins the group via invite link.
    /// 3. B performs an SV sync handshake to catch up on A's state.
    /// 4. B should see: group name, both members, A's check-in location,
    ///    A's starred events, and A's chat message.
    #[tokio::test]
    async fn test_two_user_group_sync_full() {
        use crate::chat::ChatManager;

        // --- Device setup (separate DBs simulate separate devices) ---
        let db_a = Arc::new(Database::new_in_memory().expect("in-memory db"));
        let doc_a = Arc::new(DocManager::new(db_a.clone()));
        let gm_a = GroupManager::new(db_a.clone(), doc_a.clone());
        let chat_a = ChatManager::new(db_a.clone(), doc_a.clone());

        let db_b = Arc::new(Database::new_in_memory().expect("in-memory db"));
        let doc_b = Arc::new(DocManager::new(db_b.clone()));
        let gm_b = GroupManager::new(db_b.clone(), doc_b.clone());
        let chat_b = ChatManager::new(db_b.clone(), doc_b.clone());

        // --- User A: create group ---
        let create = gm_a
            .create_group("fest-1", "The Crew", "alice", "Alice")
            .await
            .unwrap();
        let group_id = &create.group_id;

        // --- User A: send a chat message ---
        let (encrypted_chat, _topic) = chat_a
            .send_group_chat(group_id, "alice", "Alice", "hey everyone!")
            .unwrap();

        // --- User A: check in at main-stage ---
        let _encrypted_checkin = gm_a
            .check_in(group_id, "alice", Some("main-stage"), None)
            .await
            .unwrap();

        // --- User A: star (like) some events ---
        let _encrypted_stars = gm_a
            .update_stars(
                group_id,
                "alice",
                vec!["event-1".into(), "event-2".into()],
            )
            .await
            .unwrap();

        // --- User B: join the group ---
        gm_b.join_group(&create.invite_payload, "bob", "Bob")
            .await
            .unwrap();

        // --- SV sync: B requests, A responds, B applies ---
        let encrypted_sv = gm_b.request_group_sync(group_id).await.unwrap();
        let encrypted_diff = gm_a
            .handle_sync_request(group_id, &encrypted_sv)
            .await
            .unwrap();

        let group_key = db_b.load_group_key(group_id).unwrap().unwrap();
        let diff = crate::crypto::decrypt(&group_key, &encrypted_diff).unwrap();
        doc_b
            .apply_update(&format!("group/{group_id}"), &diff)
            .unwrap();

        // --- B receives A's chat message (simulates gossip delivery) ---
        let received_msg = chat_b
            .receive_encrypted_group_chat(group_id, &encrypted_chat)
            .unwrap();
        assert_eq!(received_msg.text, "hey everyone!");
        assert_eq!(received_msg.user_id, "alice");
        assert_eq!(received_msg.display_name, "Alice");

        // Verify chat is in B's history
        let history = chat_b
            .get_history(&format!("group/{group_id}/chat"), 10, 0)
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].text, "hey everyone!");

        // --- Verify B sees full group state ---
        let state = gm_b.get_group_state(group_id).await.unwrap();

        // Group name
        assert_eq!(state.name, "The Crew");

        // Both members present
        assert_eq!(state.members.len(), 2);
        let alice = state
            .members
            .iter()
            .find(|m| m.user_id == "alice")
            .expect("alice should be a member");
        let bob = state
            .members
            .iter()
            .find(|m| m.user_id == "bob")
            .expect("bob should be a member");
        assert_eq!(alice.display_name, "Alice");
        assert_eq!(bob.display_name, "Bob");

        // Alice's check-in location
        assert_eq!(alice.stage_id.as_deref(), Some("main-stage"));

        // Alice's starred events (read from nested map)
        let stars = gm_b.read_user_stars(group_id, "alice");
        assert_eq!(stars.len(), 2);
        assert!(stars.contains(&"event-1".to_string()));
        assert!(stars.contains(&"event-2".to_string()));
    }

    /// D1 creates a group and makes some changes. D2 joins and has only its
    /// own member entry. D2 sends its SV (encrypted) as a sync_request; D1
    /// computes the diff and returns it as a sync_response; D2 applies the diff
    /// and now has all of D1's changes. Then D2 makes a change, sends the diff
    /// to D1, and D1 applies it successfully because they are now in sync.
    #[tokio::test]
    async fn test_sync_request_response_roundtrip() {
        let gm_d1 = make_manager();
        let gm_d2 = make_manager();

        // D1 creates group and makes changes.
        let create = gm_d1
            .create_group("fest-1", "SV Crew", "d1", "D1")
            .await
            .unwrap();
        let group_id = &create.group_id;

        gm_d1
            .add_pin(group_id, "pin-1", "Meeting Point", "0,0", "d1")
            .await
            .unwrap();
        gm_d1
            .check_in(group_id, "d1", Some("main-stage"), None)
            .await
            .unwrap();

        // D2 joins the group (has only its member entry).
        gm_d2
            .join_group(&create.invite_payload, "d2", "D2")
            .await
            .unwrap();

        // D2 produces an encrypted SV (sync_request payload).
        let encrypted_sv = gm_d2.request_group_sync(group_id).await.unwrap();

        // D1 handles the sync_request: computes diff, returns encrypted diff.
        let encrypted_diff = gm_d1
            .handle_sync_request(group_id, &encrypted_sv)
            .await
            .unwrap();

        // D2 applies the diff.
        let group_key = gm_d2.db.load_group_key(group_id).unwrap().unwrap();
        let diff = crate::crypto::decrypt(&group_key, &encrypted_diff).unwrap();
        gm_d2
            .doc_manager
            .apply_update(&format!("group/{group_id}"), &diff)
            .unwrap();

        // D2 should now see D1's pin and location.
        let state_d2 = gm_d2.get_group_state(group_id).await.unwrap();
        assert_eq!(state_d2.pins.len(), 1, "D2 should see D1's pin");
        assert_eq!(state_d2.pins[0].label, "Meeting Point");

        let d1_member = state_d2
            .members
            .iter()
            .find(|m| m.user_id == "d1")
            .expect("D2 should see D1 as member");
        assert_eq!(d1_member.stage_id.as_deref(), Some("main-stage"));

        // First, also sync D2's state (member/d2) to D1 via a reverse handshake.
        let encrypted_sv_d1 = gm_d1.request_group_sync(group_id).await.unwrap();
        let encrypted_diff_d2_to_d1 = gm_d2
            .handle_sync_request(group_id, &encrypted_sv_d1)
            .await
            .unwrap();
        let diff_join =
            crate::crypto::decrypt(&group_key, &encrypted_diff_d2_to_d1).unwrap();
        gm_d1
            .doc_manager
            .apply_update(&format!("group/{group_id}"), &diff_join)
            .unwrap();

        // D2 makes a change and sends the diff to D1.
        let encrypted_update_d2 = gm_d2
            .check_in(group_id, "d2", Some("side-stage"), None)
            .await
            .unwrap();

        // D1 applies D2's diff (they are now fully bidirectionally synced).
        let diff_d2 = crate::crypto::decrypt(&group_key, &encrypted_update_d2).unwrap();
        gm_d1
            .doc_manager
            .apply_update(&format!("group/{group_id}"), &diff_d2)
            .unwrap();

        let state_d1 = gm_d1.get_group_state(group_id).await.unwrap();
        let d2_member = state_d1
            .members
            .iter()
            .find(|m| m.user_id == "d2")
            .expect("D1 should see D2 as member after bidirectional sync");
        assert_eq!(d2_member.stage_id.as_deref(), Some("side-stage"));
    }
}
