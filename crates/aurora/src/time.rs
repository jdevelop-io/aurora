//! Timestamp formatting for the JSON event stream.

use jiff::{Timestamp, Unit};

/// The current instant as an RFC 3339 string with millisecond precision and a
/// `Z` suffix, for example `2026-07-17T10:00:00.120Z`.
///
/// RFC 3339 is machine-consumable everywhere (`jq`'s `fromdateiso8601`, every
/// language's parser) and stays readable in a raw CI log, so it serves both a
/// script and a human debugging a pipeline.
pub fn now_iso8601() -> String {
    let rounded = Timestamp::now()
        .round(Unit::Millisecond)
        .expect("rounding to milliseconds cannot fail: the increment divides 86_400s evenly");
    // An explicit precision of 3 pins the fraction to three digits; jiff's default
    // `Display` trims trailing zeros (120 ms would render as `.12Z`), which breaks
    // the fixed-width contract this helper promises.
    format!("{rounded:.3}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_iso8601_is_parseable_rfc3339_utc() {
        let stamp = now_iso8601();
        // Ends in Z (UTC) and round-trips through the RFC 3339 parser.
        assert!(stamp.ends_with('Z'), "expected a Z suffix, got {stamp}");
        let parsed: Timestamp = stamp.parse().expect("must parse back as a Timestamp");
        // Rounded to milliseconds: no sub-millisecond digits remain.
        assert_eq!(parsed.subsec_nanosecond() % 1_000_000, 0, "not millisecond-rounded: {stamp}");
        // The fraction is always exactly three digits: jiff's default `Display`
        // trims trailing zeros, which the parse-back above cannot detect, so
        // assert the width directly on the rendered string.
        let dot = stamp.find('.').expect("expected a fractional part, got {stamp}");
        let fraction = &stamp[dot + 1..stamp.len() - 1];
        assert_eq!(fraction.len(), 3, "expected three fractional digits, got {stamp}");
    }
}
