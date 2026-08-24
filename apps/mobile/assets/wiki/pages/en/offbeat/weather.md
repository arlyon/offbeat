---
{
  "schemaVersion": 1,
  "id": "offbeat.weather",
  "locale": "en",
  "title": "OFFBEAT weather",
  "summary": "Read a cached festival forecast, check its timestamp and location, and understand missing data and severe-weather limits.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["forecast", "hourly weather", "rain", "temperature", "wind"],
  "tags": ["weather", "forecast", "offline", "Open-Meteo", "safety"],
  "generatedRefs": [],
  "priority": "high",
  "order": 880,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Festival weather fetch and signed document update",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/server/src/festival-do.ts"
    },
    {
      "title": "Festival setup and weather alarm",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/server/src/api.ts"
    },
    {
      "title": "Weather document decoding",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/dto.rs"
    },
    {
      "title": "Mobile weather subscription",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/main.dart"
    },
    {
      "title": "Hourly weather sheet",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/widgets/weather_sheet.dart"
    }
  ]
}
---
# OFFBEAT weather

OFFBEAT shows an hourly forecast for the selected festival when valid weather data exists in its signed festival document. The forecast is fetched by the festival server from Open-Meteo for the festival coordinates, then distributed through the same festival-state sync path as the lineup.

> **Safety:** This is a forecast, not a severe-weather alert or evacuation system. Follow venue announcements, stewards, emergency services and official weather authorities.

## Current support

**Available:** a top-bar temperature and condition icon, an hourly detail sheet with temperature, precipitation probability, weather code and wind speed, the forecast timezone, source label, and an `UPDATED` timestamp.

**Not available:** push weather alerts, lightning detection, flood or heat warnings, official warning feeds, evacuation instructions, a manual refresh button, or a freshness warning that automatically hides stale data.

The sheet displays the available hourly rows through the end of the festival weather window. Upstream forecast availability can still shorten that range.

## Prerequisites

- The festival record must have latitude and longitude.
- The Festival Durable Object must have successfully fetched a forecast.
- The signed festival document must have synced to this device at least once.

The forecast is associated with the configured festival coordinates, not the phone's live location. It may not describe a campsite, travel route or another part of a large venue precisely.

## Source and refresh behavior

The server requests Open-Meteo hourly temperature at 2 m, precipitation probability, weather code and wind speed at 10 m, using the forecast timezone returned for the festival coordinates.

The attendee weather window begins one day before the first programmed lineup day and ends one day after the last programmed lineup day. The server begins loading that forecast seven days before the weather window and refreshes it after the previous successful fetch is at least 24 hours old. A failed fetch leaves the prior document in place and retries on a later alarm without creating a user alert.

This cadence is server behavior, not a guarantee. Alarm scheduling, connectivity, coordinates or the upstream service can prevent an update. Always inspect the displayed `UPDATED` time.

## Reading the forecast

Tap the weather pill in the festival top bar. The sheet shows:

- Current or nearest available hourly condition.
- Temperature in degrees as supplied by the configured response.
- Precipitation probability as a percentage.
- Wind speed from the `wind_speed_10m` field.
- A condition derived from the WMO weather code.
- Forecast timezone and update time.

A probability is not certainty. A weather-code icon is a compact forecast category, not a safety classification.

## Offline behavior

Once accepted into the local Yrs festival document, weather remains available without internet and after app restart. Search or a direct weather REST request is not required in Flutter.

Offline weather does not refresh itself. The app may continue to show the last cached forecast, including past or stale hours. The `UPDATED` footer is the current way to judge age; OFFBEAT does not add an automatic stale banner.

## Missing and error states

If no complete weather metadata or no hourly entries have synced, the weather pill is absent. The current UI does not show a separate forecast error card.

If the weather pill disappears after switching festivals, that festival may have no coordinates, no successful forecast, or no locally synced hourly rows. Transport `ONLINE` does not prove weather data exists.

If an upstream fetch fails, the server logs the error and keeps the previous forecast. The app does not currently explain that failure to attendees.

## Privacy and trust

The server sends the festival's configured coordinates to Open-Meteo. The phone does not send its own live location to produce this forecast.

Weather is public festival state and is signed as part of the festival-authority update before the Rust core accepts it. The signature confirms the configured festival authority produced the OFFBEAT update. It does not certify that the forecast is accurate.

The forecast coordinates and update traffic are not private group data.

## Constraints

- Forecast accuracy, timing and local conditions are not guaranteed.
- The server stops weather refreshes after the attendee weather window closes.
- Future rows are capped by Open-Meteo's forecast availability and the configured weather-window boundary.
- There are no severe-weather notifications or official warnings in OFFBEAT.
- Cached weather must never override venue evacuation, shelter or closure instructions.
- A missing pill must not be interpreted as safe weather.

## Troubleshooting

### No weather pill appears

Confirm a festival is selected and its lineup has begun syncing. Check the connection drawer, then wait for a signed festival update. If the festival has no configured coordinates or the server has never completed a forecast, there is no local fix.

### The forecast looks old

Open the sheet and read `UPDATED`. Re-establish a relay or peer route and leave the festival selected long enough to receive a later signed update. There is no attendee force-refresh action.

### The hours do not match your clock

Check the timezone displayed in the sheet. Forecast times use the festival forecast timezone, which may differ from the phone's current travel timezone.

### Conditions disagree with what you see

Use direct observation and official venue or weather-authority guidance. Forecast grids cannot represent every local shower, wind gust or site feature.

For the shared signed data path, see [OFFBEAT lineup](wiki:offbeat.lineup) and [P2P syncing](wiki:offbeat.p2p-syncing).
