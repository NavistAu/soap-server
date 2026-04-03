// WS-Security timestamp validation
use chrono::{DateTime, Utc, Duration};
use crate::fault::SoapFault;

/// Parse an ISO 8601 datetime string (wsu:Created format).
pub fn parse_created(s: &str) -> Result<DateTime<Utc>, SoapFault> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| SoapFault::sender("Invalid WS-Security timestamp format"))
}

/// Check that `created` is within `tolerance_secs` of `now`.
/// Per WS-Security spec, default tolerance is 300 seconds.
pub fn check_freshness(
    now: DateTime<Utc>,
    created: DateTime<Utc>,
    tolerance_secs: i64,
) -> Result<(), SoapFault> {
    let diff = now.signed_duration_since(created);
    if diff > Duration::seconds(tolerance_secs) {
        return Err(SoapFault::sender("WS-Security timestamp expired"));
    }
    if diff < Duration::seconds(-tolerance_secs) {
        return Err(SoapFault::sender("WS-Security timestamp is in the future"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 3, 12, 0, 0).unwrap()
    }

    #[test]
    fn freshness_accepts_recent_timestamp() {
        let created = now() - Duration::seconds(10);
        assert!(check_freshness(now(), created, 300).is_ok());
    }

    #[test]
    fn freshness_rejects_expired_timestamp() {
        let created = now() - Duration::seconds(400);
        let result = check_freshness(now(), created, 300);
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(fault.reason.contains("expired"), "got: {}", fault.reason);
    }

    #[test]
    fn freshness_rejects_future_timestamp() {
        let created = now() + Duration::seconds(400);
        let result = check_freshness(now(), created, 300);
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(fault.reason.contains("future"), "got: {}", fault.reason);
    }

    #[test]
    fn parse_created_valid_rfc3339() {
        let result = parse_created("2026-04-03T12:00:00Z");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_created_invalid_string_returns_err() {
        let result = parse_created("not-a-date");
        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert!(fault.reason.contains("Invalid"), "got: {}", fault.reason);
    }

    #[test]
    fn freshness_accepts_at_tolerance_boundary() {
        // Exactly at the tolerance boundary (equal) should be accepted
        let created = now() - Duration::seconds(300);
        assert!(check_freshness(now(), created, 300).is_ok());
    }

    #[test]
    fn freshness_rejects_one_second_past_tolerance() {
        let created = now() - Duration::seconds(301);
        assert!(check_freshness(now(), created, 300).is_err());
    }
}
