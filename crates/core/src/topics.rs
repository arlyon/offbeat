use iroh_gossip::proto::TopicId;

/// Derive a deterministic topic ID for a festival channel.
///
/// Input: `"offbeat/{festival_id}/{channel}"` hashed with blake3.
pub fn festival_topic(festival_id: &str, channel: &str) -> TopicId {
    let input = format!("offbeat/{festival_id}/{channel}");
    let hash = blake3::hash(input.as_bytes());
    TopicId::from_bytes(*hash.as_bytes())
}

/// Derive a deterministic topic ID for a group channel.
///
/// Input: group_key bytes concatenated with channel bytes, hashed with blake3.
pub fn group_topic(group_key: &[u8; 32], channel: &str) -> TopicId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(group_key);
    hasher.update(channel.as_bytes());
    let hash = hasher.finalize();
    TopicId::from_bytes(*hash.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_festival_topic_deterministic() {
        let t1 = festival_topic("fest-2026", "state");
        let t2 = festival_topic("fest-2026", "state");
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_group_topic_deterministic() {
        let key = [42u8; 32];
        let t1 = group_topic(&key, "campsite");
        let t2 = group_topic(&key, "campsite");
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_different_inputs_different_topics() {
        let t1 = festival_topic("fest-2026", "state");
        let t2 = festival_topic("fest-2026", "chat");
        let t3 = festival_topic("other-fest", "state");
        assert_ne!(t1, t2);
        assert_ne!(t1, t3);
        assert_ne!(t2, t3);
    }

    #[test]
    fn test_festival_topic_ne_group_topic() {
        // Even if inputs look similar, festival vs group topics differ
        let key = *blake3::hash(b"offbeat/fest-2026/state").as_bytes();
        let festival = festival_topic("fest-2026", "state");
        let group = group_topic(&key, "state");
        assert_ne!(festival, group);
    }
}
