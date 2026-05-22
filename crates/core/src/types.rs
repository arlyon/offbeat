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
    pub year: u32,
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

#[derive(Debug, Clone)]
pub struct SignedUpdate {
    pub update: Vec<u8>,    // raw Yrs update bytes
    pub author: String,
    pub signature: Vec<u8>, // raw Ed25519 signature bytes
}

/// Hourly weather data arrays (parallel arrays, one entry per hour).
///
/// Field names match the Open-Meteo API response exactly (snake_case with
/// numeric suffixes), so we use explicit `serde(rename)` instead of
/// `rename_all` — camelCase would mangle `temperature_2m` → `temperature2m`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyWeather {
    pub time: Vec<String>,
    pub temperature_2m: Vec<f64>,
    pub precipitation_probability: Vec<f64>,
    pub weather_code: Vec<u32>,
    pub wind_speed_10m: Vec<f64>,
}

/// Weather forecast stored in the Yrs doc under the `"weather"` key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherForecast {
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub lat: f64,
    pub lon: f64,
    pub timezone: String,
    pub hourly: HourlyWeather,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact JSON shape the Festival DO writes into the Yrs doc.
    const WEATHER_JSON: &str = r#"{
        "updatedAt": "2026-06-13T12:00:00Z",
        "lat": 51.5369,
        "lon": -0.0394,
        "timezone": "Europe/London",
        "hourly": {
            "time": ["2026-06-13T06:00", "2026-06-13T07:00", "2026-06-13T08:00"],
            "temperature_2m": [15.2, 16.1, 17.0],
            "precipitation_probability": [5, 10, 15],
            "weather_code": [1, 2, 3],
            "wind_speed_10m": [8.2, 9.1, 10.0]
        }
    }"#;

    #[test]
    fn deserialize_weather_forecast() {
        let forecast: WeatherForecast = serde_json::from_str(WEATHER_JSON).unwrap();
        assert_eq!(forecast.lat, 51.5369);
        assert_eq!(forecast.lon, -0.0394);
        assert_eq!(forecast.timezone, "Europe/London");
        assert_eq!(forecast.hourly.time.len(), 3);
        assert_eq!(forecast.hourly.temperature_2m[0], 15.2);
        assert_eq!(forecast.hourly.precipitation_probability[1], 10.0);
        assert_eq!(forecast.hourly.weather_code[2], 3);
        assert_eq!(forecast.hourly.wind_speed_10m[0], 8.2);
    }

    #[test]
    fn serialize_roundtrip() {
        let forecast: WeatherForecast = serde_json::from_str(WEATHER_JSON).unwrap();
        let json = serde_json::to_string(&forecast).unwrap();
        let roundtrip: WeatherForecast = serde_json::from_str(&json).unwrap();
        assert_eq!(forecast.updated_at, roundtrip.updated_at);
        assert_eq!(forecast.hourly.time, roundtrip.hourly.time);
        assert_eq!(forecast.hourly.temperature_2m, roundtrip.hourly.temperature_2m);
    }

    #[test]
    fn weather_from_yrs_doc() {
        use yrs::{Doc, Map, Transact};

        let doc = Doc::new();
        let root = doc.get_or_insert_map("root");
        {
            let mut txn = doc.transact_mut();
            root.insert(&mut txn, "weather", WEATHER_JSON.trim());
        }
        let txn = doc.transact();
        let raw = root.get(&txn, "weather").unwrap().to_string(&txn);
        let forecast: WeatherForecast = serde_json::from_str(&raw).unwrap();
        assert_eq!(forecast.hourly.time.len(), 3);
        assert_eq!(forecast.lat, 51.5369);
    }

    #[test]
    fn empty_hourly_arrays() {
        let json = r#"{
            "updatedAt": "2026-06-13T12:00:00Z",
            "lat": 0.0, "lon": 0.0,
            "timezone": "UTC",
            "hourly": {
                "time": [],
                "temperature_2m": [],
                "precipitation_probability": [],
                "weather_code": [],
                "wind_speed_10m": []
            }
        }"#;
        let forecast: WeatherForecast = serde_json::from_str(json).unwrap();
        assert!(forecast.hourly.time.is_empty());
    }
}
