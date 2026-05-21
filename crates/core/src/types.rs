use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Festival {
    pub id: String,
    pub name: String,
    pub year: u32,
    pub location: String,
    pub city: String,
    pub country: String,
    pub start_date: String,
    pub end_date: String,
    pub stages: Vec<Stage>,
    pub genres: Vec<String>,
    pub status: FestivalStatus,
    pub public_key: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FestivalStatus {
    #[serde(rename = "upcoming")]
    Upcoming,
    #[serde(rename = "live")]
    Live,
    #[serde(rename = "past")]
    Past,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub id: String,
    pub name: String,
    pub short: String,
    pub color: String,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub id: String,
    pub label: String,
    pub num: u32,
    pub month: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Set {
    pub id: String,
    pub day: String,
    pub stage: String,
    pub artist: String,
    pub start_min: u32,
    pub duration_min: u32,
    pub genre: String,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FestivalRef {
    pub id: String,
    pub name: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lineup {
    pub festival: FestivalRef,
    pub stages: Vec<Stage>,
    pub days: Vec<Day>,
    pub sets: Vec<Set>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberLocation {
    pub user_id: String,
    pub display_name: String,
    pub stage_id: Option<String>,
    pub custom_location: Option<String>,
    pub status: MemberStatus,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemberStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "idle")]
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPin {
    pub id: String,
    pub label: String,
    pub location: String,
    pub pinned_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    pub text: String,
    pub topic: String,
    pub stage_id: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub writer_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportMode {
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "mesh")]
    Mesh,
    #[serde(rename = "offline")]
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    pub mode: TransportMode,
    pub ws_connected: bool,
    pub ble_peers: u32,
    pub mesh_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedUpdate {
    pub update: String,    // base64-encoded binary
    pub author: String,
    pub signature: String, // base64-encoded
}
