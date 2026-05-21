use std::collections::HashMap;

use iroh_gossip::proto::TopicId;

use crate::{crypto, topics};

/// The two fundamental data shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    CrdtDoc,
    AppendLog,
}

/// Visibility determines how data is protected on the wire.
#[derive(Debug, Clone)]
pub enum Visibility {
    PublicSigned { public_key: [u8; 32] },
    PrivateEncrypted { group_key: [u8; 32] },
}

/// Sync priority — lower number = synced first on connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(u8);

impl Priority {
    pub const CRITICAL: Self = Self(0);
    pub const HIGH: Self = Self(1);
    pub const MEDIUM: Self = Self(2);
    pub const LOW: Self = Self(3);
}

/// Common interface for all syncable resources.
pub trait Resource: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> ResourceKind;
    fn visibility(&self) -> Visibility;
    fn topic(&self) -> TopicId;
    fn topic_string(&self) -> String;
    fn priority(&self) -> Priority;
}

// ---------------------------------------------------------------------------
// FestivalState
// ---------------------------------------------------------------------------

/// Festival lineup/stage state — CRDT doc, publicly signed, critical priority.
pub struct FestivalState {
    festival_id: String,
    public_key: [u8; 32],
    id_string: String,
}

impl FestivalState {
    pub fn new(festival_id: impl Into<String>, public_key: [u8; 32]) -> Self {
        let festival_id = festival_id.into();
        let id_string = format!("festival/{festival_id}/state");
        Self { festival_id, public_key, id_string }
    }
}

impl Resource for FestivalState {
    fn id(&self) -> &str {
        &self.id_string
    }

    fn kind(&self) -> ResourceKind {
        ResourceKind::CrdtDoc
    }

    fn visibility(&self) -> Visibility {
        Visibility::PublicSigned { public_key: self.public_key }
    }

    fn topic(&self) -> TopicId {
        topics::festival_topic(&self.festival_id, "state")
    }

    fn topic_string(&self) -> String {
        format!("offbeat/{}/state", self.festival_id)
    }

    fn priority(&self) -> Priority {
        Priority::CRITICAL
    }
}

// ---------------------------------------------------------------------------
// GroupState
// ---------------------------------------------------------------------------

/// Group membership/presence state — CRDT doc, privately encrypted, high priority.
pub struct GroupState {
    group_key: [u8; 32],
    group_id: String,
    id_string: String,
}

impl GroupState {
    pub fn new(group_key: [u8; 32]) -> Self {
        let group_id = crypto::group_id_from_key(&group_key);
        let id_string = format!("group/{group_id}/state");
        Self { group_key, group_id, id_string }
    }
}

impl Resource for GroupState {
    fn id(&self) -> &str {
        &self.id_string
    }

    fn kind(&self) -> ResourceKind {
        ResourceKind::CrdtDoc
    }

    fn visibility(&self) -> Visibility {
        Visibility::PrivateEncrypted { group_key: self.group_key }
    }

    fn topic(&self) -> TopicId {
        topics::group_topic(&self.group_key, "state")
    }

    fn topic_string(&self) -> String {
        format!("group/{}/state", self.group_id)
    }

    fn priority(&self) -> Priority {
        Priority::HIGH
    }
}

// ---------------------------------------------------------------------------
// GroupChat
// ---------------------------------------------------------------------------

/// Group chat messages — append-log, privately encrypted, medium priority.
pub struct GroupChat {
    group_key: [u8; 32],
    group_id: String,
    id_string: String,
}

impl GroupChat {
    pub fn new(group_key: [u8; 32]) -> Self {
        let group_id = crypto::group_id_from_key(&group_key);
        let id_string = format!("group/{group_id}/chat");
        Self { group_key, group_id, id_string }
    }
}

impl Resource for GroupChat {
    fn id(&self) -> &str {
        &self.id_string
    }

    fn kind(&self) -> ResourceKind {
        ResourceKind::AppendLog
    }

    fn visibility(&self) -> Visibility {
        Visibility::PrivateEncrypted { group_key: self.group_key }
    }

    fn topic(&self) -> TopicId {
        topics::group_topic(&self.group_key, "chat")
    }

    fn topic_string(&self) -> String {
        format!("group/{}/chat", self.group_id)
    }

    fn priority(&self) -> Priority {
        Priority::MEDIUM
    }
}

// ---------------------------------------------------------------------------
// StageChat
// ---------------------------------------------------------------------------

/// Stage-specific chat messages — append-log, publicly signed, low priority.
pub struct StageChat {
    festival_id: String,
    stage_id: String,
    public_key: [u8; 32],
    id_string: String,
}

impl StageChat {
    pub fn new(
        festival_id: impl Into<String>,
        stage_id: impl Into<String>,
        public_key: [u8; 32],
    ) -> Self {
        let festival_id = festival_id.into();
        let stage_id = stage_id.into();
        let id_string = format!("festival/{festival_id}/chat/{stage_id}");
        Self { festival_id, stage_id, public_key, id_string }
    }
}

impl Resource for StageChat {
    fn id(&self) -> &str {
        &self.id_string
    }

    fn kind(&self) -> ResourceKind {
        ResourceKind::AppendLog
    }

    fn visibility(&self) -> Visibility {
        Visibility::PublicSigned { public_key: self.public_key }
    }

    fn topic(&self) -> TopicId {
        topics::festival_topic(&self.festival_id, &format!("chat/{}", self.stage_id))
    }

    fn topic_string(&self) -> String {
        format!("offbeat/{}/chat/{}", self.festival_id, self.stage_id)
    }

    fn priority(&self) -> Priority {
        Priority::LOW
    }
}

// ---------------------------------------------------------------------------
// ResourceRegistry
// ---------------------------------------------------------------------------

/// Registry of all active syncable resources, keyed by resource ID.
pub struct ResourceRegistry {
    resources: HashMap<String, Box<dyn Resource>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self { resources: HashMap::new() }
    }

    /// Register a resource. Replaces any existing resource with the same ID.
    pub fn register(&mut self, resource: Box<dyn Resource>) {
        self.resources.insert(resource.id().to_owned(), resource);
    }

    /// Remove a resource by ID.
    pub fn deregister(&mut self, id: &str) {
        self.resources.remove(id);
    }

    /// Look up a resource by ID.
    pub fn get(&self, id: &str) -> Option<&dyn Resource> {
        self.resources.get(id).map(|r| r.as_ref())
    }

    /// Return all resources sorted by priority (lowest value first).
    pub fn by_priority(&self) -> Vec<&dyn Resource> {
        let mut v: Vec<&dyn Resource> = self.resources.values().map(|r| r.as_ref()).collect();
        v.sort_by_key(|r| r.priority());
        v
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PK: [u8; 32] = [1u8; 32];
    const GK: [u8; 32] = [2u8; 32];

    // --- FestivalState ---

    #[test]
    fn festival_state_kind_and_priority() {
        let r = FestivalState::new("fest-2026", PK);
        assert_eq!(r.kind(), ResourceKind::CrdtDoc);
        assert_eq!(r.priority(), Priority::CRITICAL);
    }

    #[test]
    fn festival_state_topic_matches_topics_module() {
        let r = FestivalState::new("fest-2026", PK);
        assert_eq!(r.topic(), topics::festival_topic("fest-2026", "state"));
    }

    #[test]
    fn festival_state_topic_string() {
        let r = FestivalState::new("fest-2026", PK);
        assert_eq!(r.topic_string(), "offbeat/fest-2026/state");
    }

    #[test]
    fn festival_state_id() {
        let r = FestivalState::new("fest-2026", PK);
        assert_eq!(r.id(), "festival/fest-2026/state");
    }

    // --- GroupState ---

    #[test]
    fn group_state_kind_and_priority() {
        let r = GroupState::new(GK);
        assert_eq!(r.kind(), ResourceKind::CrdtDoc);
        assert_eq!(r.priority(), Priority::HIGH);
    }

    #[test]
    fn group_state_topic_matches_topics_module() {
        let r = GroupState::new(GK);
        assert_eq!(r.topic(), topics::group_topic(&GK, "state"));
    }

    #[test]
    fn group_state_topic_string() {
        let r = GroupState::new(GK);
        let group_id = crypto::group_id_from_key(&GK);
        assert_eq!(r.topic_string(), format!("group/{group_id}/state"));
    }

    #[test]
    fn group_state_id() {
        let r = GroupState::new(GK);
        let group_id = crypto::group_id_from_key(&GK);
        assert_eq!(r.id(), format!("group/{group_id}/state"));
    }

    // --- GroupChat ---

    #[test]
    fn group_chat_kind_and_priority() {
        let r = GroupChat::new(GK);
        assert_eq!(r.kind(), ResourceKind::AppendLog);
        assert_eq!(r.priority(), Priority::MEDIUM);
    }

    #[test]
    fn group_chat_topic_matches_topics_module() {
        let r = GroupChat::new(GK);
        assert_eq!(r.topic(), topics::group_topic(&GK, "chat"));
    }

    #[test]
    fn group_chat_topic_string() {
        let r = GroupChat::new(GK);
        let group_id = crypto::group_id_from_key(&GK);
        assert_eq!(r.topic_string(), format!("group/{group_id}/chat"));
    }

    #[test]
    fn group_chat_id() {
        let r = GroupChat::new(GK);
        let group_id = crypto::group_id_from_key(&GK);
        assert_eq!(r.id(), format!("group/{group_id}/chat"));
    }

    // --- StageChat ---

    #[test]
    fn stage_chat_kind_and_priority() {
        let r = StageChat::new("fest-2026", "main-stage", PK);
        assert_eq!(r.kind(), ResourceKind::AppendLog);
        assert_eq!(r.priority(), Priority::LOW);
    }

    #[test]
    fn stage_chat_topic_matches_topics_module() {
        let r = StageChat::new("fest-2026", "main-stage", PK);
        assert_eq!(r.topic(), topics::festival_topic("fest-2026", "chat/main-stage"));
    }

    #[test]
    fn stage_chat_topic_string() {
        let r = StageChat::new("fest-2026", "main-stage", PK);
        assert_eq!(r.topic_string(), "offbeat/fest-2026/chat/main-stage");
    }

    #[test]
    fn stage_chat_id() {
        let r = StageChat::new("fest-2026", "main-stage", PK);
        assert_eq!(r.id(), "festival/fest-2026/chat/main-stage");
    }

    // --- ResourceRegistry ---

    #[test]
    fn registry_register_and_get() {
        let mut reg = ResourceRegistry::new();
        reg.register(Box::new(FestivalState::new("fest-2026", PK)));
        assert!(reg.get("festival/fest-2026/state").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_deregister() {
        let mut reg = ResourceRegistry::new();
        reg.register(Box::new(FestivalState::new("fest-2026", PK)));
        reg.deregister("festival/fest-2026/state");
        assert!(reg.get("festival/fest-2026/state").is_none());
    }

    #[test]
    fn registry_by_priority_ordering() {
        let mut reg = ResourceRegistry::new();
        // Insert in non-priority order
        reg.register(Box::new(StageChat::new("fest-2026", "main-stage", PK)));
        reg.register(Box::new(GroupChat::new(GK)));
        reg.register(Box::new(GroupState::new(GK)));
        reg.register(Box::new(FestivalState::new("fest-2026", PK)));

        let ordered = reg.by_priority();
        assert_eq!(ordered.len(), 4);

        let priorities: Vec<Priority> = ordered.iter().map(|r| r.priority()).collect();
        // Must be non-decreasing
        for window in priorities.windows(2) {
            assert!(window[0] <= window[1], "priorities not sorted: {:?}", priorities);
        }

        // First must be CRITICAL, last must be LOW
        assert_eq!(priorities[0], Priority::CRITICAL);
        assert_eq!(priorities[3], Priority::LOW);
    }

    #[test]
    fn registry_replace_existing() {
        let mut reg = ResourceRegistry::new();
        reg.register(Box::new(FestivalState::new("fest-2026", PK)));
        reg.register(Box::new(FestivalState::new("fest-2026", [9u8; 32])));
        // Still one entry
        assert_eq!(reg.by_priority().len(), 1);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::CRITICAL < Priority::HIGH);
        assert!(Priority::HIGH < Priority::MEDIUM);
        assert!(Priority::MEDIUM < Priority::LOW);
    }
}
