use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use zip::ZipArchive;

use crate::Arrival;

/// Live GTFS-Realtime trip updates published by Caltrain (no API key).
const TRIP_UPDATES_URL: &str = "https://www.caltrain.com/files/rt/tripupdates/CT.json";
/// Static GTFS linked from Caltrain's developer page. Used for trip destinations.
const GTFS_URL: &str = "https://data.trilliumtransit.com/gtfs/caltrain-ca-us/caltrain-ca-us.zip";

/// Northbound and southbound platforms at Redwood City.
const REDWOOD_STOPS: &[&str] = &["70141", "70142"];

const GTFS_TTL: Duration = Duration::from_secs(6 * 60 * 60);

struct GtfsCache {
    fetched_at: Instant,
    headsigns: Arc<HashMap<String, String>>,
}

static GTFS_CACHE: Mutex<Option<GtfsCache>> = Mutex::new(None);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("board/0.1 (redwood-city-caltrain)")
        .build()
        .expect("reqwest client")
}

#[derive(Debug, Deserialize)]
struct Feed {
    #[serde(rename = "Header")]
    header: FeedHeader,
    #[serde(rename = "Entities", default)]
    entities: Vec<Entity>,
}

#[derive(Debug, Deserialize)]
struct FeedHeader {
    #[serde(rename = "Timestamp", default)]
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct Entity {
    #[serde(rename = "TripUpdate")]
    trip_update: Option<TripUpdate>,
}

#[derive(Debug, Deserialize)]
struct TripUpdate {
    #[serde(rename = "Trip")]
    trip: Trip,
    #[serde(rename = "StopTimeUpdate", default)]
    stop_time_update: Vec<StopTimeUpdate>,
}

#[derive(Debug, Deserialize)]
struct Trip {
    #[serde(rename = "TripId")]
    trip_id: String,
    #[serde(rename = "RouteId", default)]
    route_id: String,
    #[serde(rename = "DirectionId")]
    direction_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct StopTimeUpdate {
    #[serde(rename = "StopId")]
    stop_id: String,
    #[serde(rename = "Arrival")]
    arrival: Option<TimedEvent>,
    #[serde(rename = "Departure")]
    departure: Option<TimedEvent>,
}

#[derive(Debug, Deserialize)]
struct TimedEvent {
    #[serde(rename = "Time")]
    time: Option<i64>,
}

fn service_name(route_id: &str) -> String {
    let lower = route_id.to_ascii_lowercase();
    if lower.contains("express") {
        "Express".to_string()
    } else if lower.contains("limited") {
        "Limited".to_string()
    } else if lower.contains("south") {
        "S. County".to_string()
    } else if lower.contains("local") {
        "Local".to_string()
    } else if route_id.is_empty() {
        "Train".to_string()
    } else {
        route_id.to_string()
    }
}

/// Badge color. Chosen so white "N"/"S" text stays readable.
fn service_color(service: &str) -> [u8; 3] {
    match service {
        "Express" => [0xC4, 0x18, 0x18],
        "Limited" => [0x0E, 0x7C, 0x86],
        "S. County" => [0x8A, 0x5A, 0x00],
        _ => [0x3E, 0x5C, 0x6E],
    }
}

fn direction_label(direction_id: Option<i64>, stop_id: &str) -> String {
    match direction_id {
        Some(0) => "N".to_string(),
        Some(1) => "S".to_string(),
        _ if stop_id.ends_with('1') => "N".to_string(),
        _ => "S".to_string(),
    }
}

fn fallback_destination(direction: &str) -> String {
    if direction == "N" {
        "San Francisco".to_string()
    } else {
        "San Jose".to_string()
    }
}

async fn headsigns() -> Result<Arc<HashMap<String, String>>, String> {
    {
        let guard = GTFS_CACHE.lock().map_err(|e| e.to_string())?;
        if let Some(cache) = guard.as_ref() {
            if cache.fetched_at.elapsed() < GTFS_TTL {
                return Ok(Arc::clone(&cache.headsigns));
            }
        }
    }

    let bytes = client()
        .get(GTFS_URL)
        .send()
        .await
        .map_err(|e| format!("gtfs download: {e}"))?
        .error_for_status()
        .map_err(|e| format!("gtfs download: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("gtfs download: {e}"))?;

    let headsigns = parse_headsigns(&bytes)?;
    let headsigns = Arc::new(headsigns);
    let mut guard = GTFS_CACHE.lock().map_err(|e| e.to_string())?;
    *guard = Some(GtfsCache {
        fetched_at: Instant::now(),
        headsigns: Arc::clone(&headsigns),
    });
    Ok(headsigns)
}

fn parse_headsigns(zip_bytes: &[u8]) -> Result<HashMap<String, String>, String> {
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| format!("gtfs zip: {e}"))?;
    let mut file = archive
        .by_name("trips.txt")
        .map_err(|e| format!("gtfs trips.txt: {e}"))?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| format!("gtfs trips.txt: {e}"))?;

    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| format!("gtfs trips.txt: {e}"))?
        .clone();
    let trip_idx = headers
        .iter()
        .position(|h| h == "trip_id")
        .ok_or("gtfs trips.txt missing trip_id")?;
    let headsign_idx = headers
        .iter()
        .position(|h| h == "trip_headsign")
        .ok_or("gtfs trips.txt missing trip_headsign")?;

    let mut map = HashMap::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("gtfs trips.txt: {e}"))?;
        let Some(trip_id) = record.get(trip_idx) else {
            continue;
        };
        let Some(headsign) = record.get(headsign_idx) else {
            continue;
        };
        if !trip_id.is_empty() && !headsign.is_empty() {
            map.insert(trip_id.to_string(), headsign.to_string());
        }
    }
    if map.is_empty() {
        return Err("gtfs trips.txt had no trips".into());
    }
    Ok(map)
}

/// Upcoming Redwood City trains from Caltrain's live trip-update feed.
/// Destinations come from the static GTFS `trip_headsign`.
///
/// The clock offset is `server_time - local_time` in milliseconds, taken from
/// the feed header, so a device without NTP can still drop trains that have
/// already left.
pub async fn fetch_board() -> Result<(Vec<Arrival>, i64), String> {
    let (feed_result, headsign_result) = tokio::join!(fetch_feed(), headsigns());
    let feed = feed_result?;
    // A missing schedule should not blank the board; direction is a usable fallback.
    let headsigns = match headsign_result {
        Ok(map) => Some(map),
        Err(err) => {
            eprintln!("caltrain schedule unavailable, using direction fallback: {err}");
            None
        }
    };

    let local_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let server_ms = if feed.header.timestamp > 0 {
        feed.header.timestamp * 1000
    } else {
        local_ms
    };
    let clock_offset = server_ms - local_ms;

    let mut arrivals = Vec::new();
    for entity in feed.entities {
        let Some(update) = entity.trip_update else {
            continue;
        };
        let Some(stop) = update
            .stop_time_update
            .iter()
            .find(|stop| REDWOOD_STOPS.contains(&stop.stop_id.as_str()))
        else {
            continue;
        };

        let arrival_s = stop.arrival.as_ref().and_then(|event| event.time);
        let departure_s = stop.departure.as_ref().and_then(|event| event.time);
        let (arrival_s, departure_s) = match (arrival_s, departure_s) {
            (Some(arrival), Some(departure)) => (arrival, departure),
            (Some(arrival), None) => (arrival, arrival),
            (None, Some(departure)) => (departure, departure),
            (None, None) => continue,
        };

        let direction = direction_label(update.trip.direction_id, &stop.stop_id);
        let destination = headsigns
            .as_ref()
            .and_then(|map| map.get(&update.trip.trip_id))
            .cloned()
            .unwrap_or_else(|| fallback_destination(&direction));
        let service = service_name(&update.trip.route_id);
        let arrival_ms = arrival_s * 1000;
        let departure_ms = departure_s * 1000;
        let minutes = (departure_ms - server_ms) / 60_000;

        arrivals.push(Arrival {
            destination,
            arrival_time_ms: arrival_ms,
            departure_time_ms: departure_ms,
            minutes,
            route_id: update.trip.route_id,
            route_label: direction,
            route_color: service_color(&service),
            trip_id: update.trip.trip_id,
            stop_id: stop.stop_id.clone(),
            service,
        });
    }

    Ok((arrivals, clock_offset))
}

async fn fetch_feed() -> Result<Feed, String> {
    let response = client()
        .get(TRIP_UPDATES_URL)
        .send()
        .await
        .map_err(|e| format!("trip updates: {e}"))?
        .error_for_status()
        .map_err(|e| format!("trip updates: {e}"))?;
    response
        .json()
        .await
        .map_err(|e| format!("trip updates: {e}"))
}

/// `fetch_board` with up to 3 attempts.
pub async fn fetch_with_retry() -> Result<(Vec<Arrival>, i64), String> {
    let mut last_err = String::from("trip updates failed");
    for attempt in 0..3 {
        match fetch_board().await {
            Ok(result) => return Ok(result),
            Err(err) if attempt < 2 => {
                last_err = err;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(err) => return Err(err),
        }
    }
    Err(last_err)
}
