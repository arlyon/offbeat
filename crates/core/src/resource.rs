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

/// Unified resource enum — replaces the former `Resource` trait + 4 struct types.
#[derive(Debug, Clone)]
pub enum Resource {
    FestivalState {
        festival_id: String,
        public_key: [u8; 32],
    },
    GroupState {
        group_id: String,
        group_key: [u8; 32],
    },
    GroupChat {
        group_id: String,
        group_key: [u8; 32],
    },
    StageChat {
        festival_id: String,
        stage_id: String,
        public_key: [u8; 32],
    },
}

impl Resource {
    pub fn id(&self) -> String {
        match self {
            Resource::FestivalState { festival_id, .. } => {
                format!("festival/{festival_id}/state")
            }
            Resource::GroupState { group_id, .. } => {
                format!("group/{group_id}/state")
            }
            Resource::GroupChat { group_id, .. } => {
                format!("group/{group_id}/chat")
            }
            Resource::StageChat {
                festival_id,
                stage_id,
                ..
            } => {
                format!("festival/{festival_id}/chat/{stage_id}")
            }
        }
    }

    pub fn kind(&self) -> ResourceKind {
        match self {
            Resource::FestivalState { .. } | Resource::GroupState { .. } => ResourceKind::CrdtDoc,
            Resource::GroupChat { .. } | Resource::StageChat { .. } => ResourceKind::AppendLog,
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self {
            Resource::FestivalState { public_key, .. } | Resource::StageChat { public_key, .. } => {
                Visibility::PublicSigned {
                    public_key: *public_key,
                }
            }
            Resource::GroupState { group_key, .. } | Resource::GroupChat { group_key, .. } => {
                Visibility::PrivateEncrypted {
                    group_key: *group_key,
                }
            }
        }
    }

    pub fn topic(&self) -> TopicId {
        match self {
            Resource::FestivalState { festival_id, .. } => {
                topics::festival_topic(festival_id, "state")
            }
            Resource::GroupState { group_key, .. } => topics::group_topic(group_key, "state"),
            Resource::GroupChat { group_key, .. } => topics::group_topic(group_key, "chat"),
            Resource::StageChat {
                festival_id,
                stage_id,
                ..
            } => topics::festival_topic(festival_id, &format!("chat/{stage_id}")),
        }
    }

    pub fn topic_string(&self) -> String {
        match self {
            Resource::FestivalState { festival_id, .. } => {
                format!("offbeat/{festival_id}/state")
            }
            Resource::GroupState { group_id, .. } => {
                format!("group/{group_id}/state")
            }
            Resource::GroupChat { group_id, .. } => {
                format!("group/{group_id}/chat")
            }
            Resource::StageChat {
                festival_id,
                stage_id,
                ..
            } => {
                format!("offbeat/{festival_id}/chat/{stage_id}")
            }
        }
    }

    pub fn priority(&self) -> Priority {
        match self {
            Resource::FestivalState { .. } => Priority::CRITICAL,
            Resource::GroupState { .. } => Priority::HIGH,
            Resource::GroupChat { .. } => Priority::MEDIUM,
            Resource::StageChat { .. } => Priority::LOW,
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors
// ---------------------------------------------------------------------------

impl Resource {
    pub fn festival_state(festival_id: impl Into<String>, public_key: [u8; 32]) -> Self {
        Self::FestivalState {
            festival_id: festival_id.into(),
            public_key,
        }
    }

    pub fn group_state(group_key: [u8; 32]) -> Self {
        let group_id = crypto::group_id_from_key(&group_key);
        Self::GroupState {
            group_id,
            group_key,
        }
    }

    pub fn group_chat(group_key: [u8; 32]) -> Self {
        let group_id = crypto::group_id_from_key(&group_key);
        Self::GroupChat {
            group_id,
            group_key,
        }
    }

    pub fn stage_chat(
        festival_id: impl Into<String>,
        stage_id: impl Into<String>,
        public_key: [u8; 32],
    ) -> Self {
        Self::StageChat {
            festival_id: festival_id.into(),
            stage_id: stage_id.into(),
            public_key,
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceRegistry
// ---------------------------------------------------------------------------

/// Registry of all active syncable resources, keyed by resource ID.
pub struct ResourceRegistry {
    resources: HashMap<String, Resource>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Register a resource. Replaces any existing resource with the same ID.
    pub fn register(&mut self, resource: Resource) {
        self.resources.insert(resource.id(), resource);
    }

    /// Remove a resource by ID.
    pub fn deregister(&mut self, id: &str) {
        self.resources.remove(id);
    }

    /// Look up a resource by ID.
    pub fn get(&self, id: &str) -> Option<&Resource> {
        self.resources.get(id)
    }

    /// Register a festival's state resource (CRDT doc, critical priority).
    pub fn register_festival(&mut self, festival_id: &str, public_key: [u8; 32]) {
        self.register(Resource::festival_state(festival_id, public_key));
    }

    /// Register all groups for a festival (state + chat resources per group).
    pub fn register_groups(&mut self, groups: &[(String, [u8; 32])]) {
        for (_, key) in groups {
            self.register(Resource::group_state(*key));
            self.register(Resource::group_chat(*key));
        }
    }

    /// Remove both resources protected by a group's key.
    pub fn deregister_group(&mut self, group_key: [u8; 32]) {
        self.deregister(&Resource::group_state(group_key).id());
        self.deregister(&Resource::group_chat(group_key).id());
    }

    /// Return all resources sorted by priority (lowest value first).
    pub fn by_priority(&self) -> Vec<&Resource> {
        let mut v: Vec<&Resource> = self.resources.values().collect();
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

    #[test]
    fn festival_state_kind_and_priority() {
        let r = Resource::festival_state("fest-2026", PK);
        assert_eq!(r.kind(), ResourceKind::CrdtDoc);
        assert_eq!(r.priority(), Priority::CRITICAL);
    }

    #[test]
    fn festival_state_topic_matches_topics_module() {
        let r = Resource::festival_state("fest-2026", PK);
        assert_eq!(r.topic(), topics::festival_topic("fest-2026", "state"));
    }

    #[test]
    fn festival_state_topic_string() {
        let r = Resource::festival_state("fest-2026", PK);
        assert_eq!(r.topic_string(), "offbeat/fest-2026/state");
    }

    #[test]
    fn festival_state_id() {
        let r = Resource::festival_state("fest-2026", PK);
        assert_eq!(r.id(), "festival/fest-2026/state");
    }

    #[test]
    fn group_state_kind_and_priority() {
        let r = Resource::group_state(GK);
        assert_eq!(r.kind(), ResourceKind::CrdtDoc);
        assert_eq!(r.priority(), Priority::HIGH);
    }

    #[test]
    fn group_state_topic_matches_topics_module() {
        let r = Resource::group_state(GK);
        assert_eq!(r.topic(), topics::group_topic(&GK, "state"));
    }

    #[test]
    fn group_state_topic_string() {
        let r = Resource::group_state(GK);
        let group_id = crypto::group_id_from_key(&GK);
        assert_eq!(r.topic_string(), format!("group/{group_id}/state"));
    }

    #[test]
    fn group_chat_kind_and_priority() {
        let r = Resource::group_chat(GK);
        assert_eq!(r.kind(), ResourceKind::AppendLog);
        assert_eq!(r.priority(), Priority::MEDIUM);
    }

    #[test]
    fn stage_chat_kind_and_priority() {
        let r = Resource::stage_chat("fest-2026", "main-stage", PK);
        assert_eq!(r.kind(), ResourceKind::AppendLog);
        assert_eq!(r.priority(), Priority::LOW);
    }

    #[test]
    fn stage_chat_topic_matches_topics_module() {
        let r = Resource::stage_chat("fest-2026", "main-stage", PK);
        assert_eq!(
            r.topic(),
            topics::festival_topic("fest-2026", "chat/main-stage")
        );
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = ResourceRegistry::new();
        reg.register(Resource::festival_state("fest-2026", PK));
        assert!(reg.get("festival/fest-2026/state").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_deregister() {
        let mut reg = ResourceRegistry::new();
        reg.register(Resource::festival_state("fest-2026", PK));
        reg.deregister("festival/fest-2026/state");
        assert!(reg.get("festival/fest-2026/state").is_none());
    }

    #[test]
    fn registry_deregister_group_removes_state_and_chat() {
        let key = [1; 32];
        let mut reg = ResourceRegistry::new();
        reg.register_groups(&[("ignored".to_string(), key)]);

        reg.deregister_group(key);

        assert!(reg.get(&Resource::group_state(key).id()).is_none());
        assert!(reg.get(&Resource::group_chat(key).id()).is_none());
    }

    #[test]
    fn registry_by_priority_ordering() {
        let mut reg = ResourceRegistry::new();
        reg.register(Resource::stage_chat("fest-2026", "main-stage", PK));
        reg.register(Resource::group_chat(GK));
        reg.register(Resource::group_state(GK));
        reg.register(Resource::festival_state("fest-2026", PK));

        let ordered = reg.by_priority();
        assert_eq!(ordered.len(), 4);

        let priorities: Vec<Priority> = ordered.iter().map(|r| r.priority()).collect();
        for window in priorities.windows(2) {
            assert!(
                window[0] <= window[1],
                "priorities not sorted: {:?}",
                priorities
            );
        }

        assert_eq!(priorities[0], Priority::CRITICAL);
        assert_eq!(priorities[3], Priority::LOW);
    }

    #[test]
    fn registry_replace_existing() {
        let mut reg = ResourceRegistry::new();
        reg.register(Resource::festival_state("fest-2026", PK));
        reg.register(Resource::festival_state("fest-2026", [9u8; 32]));
        assert_eq!(reg.by_priority().len(), 1);
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::CRITICAL < Priority::HIGH);
        assert!(Priority::HIGH < Priority::MEDIUM);
        assert!(Priority::MEDIUM < Priority::LOW);
    }

    #[test]
    fn register_groups_produces_state_and_chat() {
        let mut reg = ResourceRegistry::new();
        let groups = vec![
            ("group-a".to_string(), [3u8; 32]),
            ("group-b".to_string(), [4u8; 32]),
        ];
        reg.register_groups(&groups);

        let ordered = reg.by_priority();
        assert_eq!(ordered.len(), 4, "2 groups × 2 resources = 4");

        let priorities: Vec<Priority> = ordered.iter().map(|r| r.priority()).collect();
        // GroupState (HIGH) should come before GroupChat (MEDIUM)
        for window in priorities.windows(2) {
            assert!(
                window[0] <= window[1],
                "priorities not sorted: {:?}",
                priorities
            );
        }

        assert_eq!(priorities[0], Priority::HIGH);
        assert_eq!(priorities[1], Priority::HIGH);
        assert_eq!(priorities[2], Priority::MEDIUM);
        assert_eq!(priorities[3], Priority::MEDIUM);
    }
}
