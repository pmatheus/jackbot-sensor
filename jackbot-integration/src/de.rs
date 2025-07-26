//! Deserialization utilities

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Extract the next item from an iterator
pub fn extract_next<T, I>(iter: &mut I) -> Option<T>
where
    I: Iterator<Item = T>,
{
    iter.next()
}

/// Convert epoch duration to UTC datetime
pub fn datetime_utc_from_epoch_duration(duration: Duration) -> DateTime<Utc> {
    let secs = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos();
    
    DateTime::from_timestamp(secs, nanos)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
}

/// Convert epoch milliseconds to UTC datetime
pub fn datetime_utc_from_epoch_millis(millis: i64) -> DateTime<Utc> {
    let secs = millis / 1000;
    let nanos = ((millis % 1000) * 1_000_000) as u32;
    
    DateTime::from_timestamp(secs, nanos)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
}

/// Parse a timestamp string to UTC datetime
pub fn parse_timestamp(s: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    // Try different timestamp formats
    if let Ok(millis) = s.parse::<i64>() {
        return Ok(datetime_utc_from_epoch_millis(millis));
    }
    
    // Try ISO 8601 format
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    
    // Try basic ISO format
    DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ")
        .map(|dt| dt.with_timezone(&Utc))
}

/// Deserialize u64 epoch milliseconds as UTC datetime
pub fn de_u64_epoch_ms_as_datetime_utc<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let millis = u64::deserialize(deserializer)?;
    Ok(datetime_utc_from_epoch_millis(millis as i64))
}

/// Deserialize string with custom parsing
pub fn de_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    use serde::{Deserialize, de::Error};
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(D::Error::custom)
}

/// Deserialize string u64 epoch milliseconds as UTC datetime
pub fn de_str_u64_epoch_ms_as_datetime_utc<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::{Deserialize, de::Error};
    let s = String::deserialize(deserializer)?;
    let millis: u64 = s.parse().map_err(D::Error::custom)?;
    Ok(datetime_utc_from_epoch_millis(millis as i64))
}