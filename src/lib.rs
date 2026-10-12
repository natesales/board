pub mod clock;
mod caltrain;

pub use caltrain::{fetch_board, fetch_with_retry};

#[derive(Debug, Clone)]
pub struct Arrival {
    pub destination: String,
    /// Predicted arrival at Redwood City, unix milliseconds.
    pub arrival_time_ms: i64,
    /// Predicted departure from Redwood City, unix milliseconds.
    pub departure_time_ms: i64,
    /// Minutes until departure, refreshed from the local clock.
    pub minutes: i64,
    pub route_id: String,
    /// "N" or "S".
    pub route_label: String,
    pub route_color: [u8; 3],
    pub trip_id: String,
    pub stop_id: String,
    /// Short service name: Local, Limited, Express, S. County.
    pub service: String,
}

/// Recalculate minutes-until-departure, then sort, filter, and dedup.
/// `clock_offset_ms` is server_time - local_time, from the feed timestamp.
pub fn recalculate_and_filter(arrivals: &mut Vec<Arrival>, clock_offset_ms: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + clock_offset_ms;
    for arrival in arrivals.iter_mut() {
        arrival.minutes = (arrival.departure_time_ms - now) / 60_000;
    }
    arrivals.sort_by_key(|arrival| arrival.departure_time_ms);
    arrivals.retain(|arrival| arrival.minutes >= -1 && arrival.minutes <= 180);
    arrivals.dedup_by(|a, b| a.trip_id == b.trip_id);
    arrivals.truncate(8);
}

/// Format minutes until departure: "Now" when due, otherwise "Xm".
pub fn format_time(minutes: i64) -> String {
    if minutes <= 0 {
        "Now".to_string()
    } else {
        format!("{minutes}m")
    }
}

/// Clock time in the local timezone, like "8:31 PM".
pub fn format_clock(unix_ms: i64) -> String {
    let Some(dt) = chrono::DateTime::from_timestamp_millis(unix_ms) else {
        return "—".to_string();
    };
    format_ampm(&dt.with_timezone(&chrono::Local), false)
}

/// Current local time with seconds, like "8:31:05 PM".
pub fn format_now(now: &chrono::DateTime<chrono::Local>) -> String {
    format_ampm(now, true)
}

fn format_ampm(dt: &chrono::DateTime<chrono::Local>, seconds: bool) -> String {
    use chrono::Timelike;
    let (is_pm, hour) = dt.hour12();
    let suffix = if is_pm { "PM" } else { "AM" };
    if seconds {
        format!("{hour}:{:02}:{:02} {suffix}", dt.minute(), dt.second())
    } else {
        format!("{hour}:{:02} {suffix}", dt.minute())
    }
}
