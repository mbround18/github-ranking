//! Prometheus metrics.
//!
//! Hand-rolled rather than pulled from a crate: the service has a handful of
//! counters and two gauges, and the text exposition format is a dozen lines to
//! produce. That keeps the dependency tree small enough to stay comfortable in
//! a distroless image.
//!
//! Labels are deliberately low-cardinality — the matched *route*, never the raw
//! path, so `/api/rank/{username}` is one series rather than one per user.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Upper bounds in seconds for the latency histogram. Chosen around what
/// actually matters here: a cache hit is sub-millisecond, a cold render is
/// dominated by two GitHub round trips.
const LATENCY_BUCKETS: [f64; 9] = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0];

#[derive(Debug, Default)]
pub struct Metrics {
    requests_total: AtomicU64,
    responses_2xx: AtomicU64,
    responses_4xx: AtomicU64,
    responses_5xx: AtomicU64,

    cache_hits: AtomicU64,
    cache_misses: AtomicU64,

    github_requests: AtomicU64,
    github_errors: AtomicU64,
    github_rate_limited: AtomicU64,

    cards_rendered: AtomicU64,

    /// Cumulative bucket counts, plus a final entry for +Inf.
    latency_buckets: [AtomicU64; LATENCY_BUCKETS.len() + 1],
    latency_sum_micros: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_response(&self, status: u16, latency: Duration) {
        let counter = match status {
            200..=399 => &self.responses_2xx,
            400..=499 => &self.responses_4xx,
            _ => &self.responses_5xx,
        };
        counter.fetch_add(1, Ordering::Relaxed);

        let seconds = latency.as_secs_f64();
        // Cumulative histogram: a sample counts in its own bucket and every
        // wider one, which is what Prometheus expects from `_bucket` series.
        for (index, bound) in LATENCY_BUCKETS.iter().enumerate() {
            if seconds <= *bound {
                self.latency_buckets[index].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.latency_buckets[LATENCY_BUCKETS.len()].fetch_add(1, Ordering::Relaxed);
        self.latency_sum_micros
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_github_request(&self) {
        self.github_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_github_error(&self) {
        self.github_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_github_rate_limited(&self) {
        self.github_rate_limited.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_card_rendered(&self) {
        self.cards_rendered.fetch_add(1, Ordering::Relaxed);
    }

    /// Render the Prometheus text exposition format.
    ///
    /// `credentials_available` and `cache_entries` are sampled at scrape time
    /// rather than tracked, because they are live properties of other
    /// components rather than events.
    pub fn encode(&self, credentials_available: usize, cache_entries: u64) -> String {
        let get = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let mut out = String::with_capacity(2048);

        counter(
            &mut out,
            "github_ranked_requests_total",
            "Total HTTP requests received.",
            get(&self.requests_total),
        );

        out.push_str("# HELP github_ranked_responses_total HTTP responses by status class.\n");
        out.push_str("# TYPE github_ranked_responses_total counter\n");
        for (class, value) in [
            ("2xx", get(&self.responses_2xx)),
            ("4xx", get(&self.responses_4xx)),
            ("5xx", get(&self.responses_5xx)),
        ] {
            out.push_str(&format!(
                "github_ranked_responses_total{{class=\"{class}\"}} {value}\n"
            ));
        }

        counter(
            &mut out,
            "github_ranked_cache_hits_total",
            "Rank lookups served from cache.",
            get(&self.cache_hits),
        );
        counter(
            &mut out,
            "github_ranked_cache_misses_total",
            "Rank lookups that required a fetch.",
            get(&self.cache_misses),
        );
        counter(
            &mut out,
            "github_ranked_github_requests_total",
            "GraphQL requests sent to GitHub.",
            get(&self.github_requests),
        );
        counter(
            &mut out,
            "github_ranked_github_errors_total",
            "GitHub requests that failed.",
            get(&self.github_errors),
        );
        counter(
            &mut out,
            "github_ranked_github_rate_limited_total",
            "GitHub requests rejected for rate limiting.",
            get(&self.github_rate_limited),
        );
        counter(
            &mut out,
            "github_ranked_cards_rendered_total",
            "Badge cards rendered.",
            get(&self.cards_rendered),
        );

        gauge(
            &mut out,
            "github_ranked_credentials_available",
            "Credentials with quota remaining.",
            credentials_available as f64,
        );
        gauge(
            &mut out,
            "github_ranked_cache_entries",
            "Entries held in the in-memory cache.",
            cache_entries as f64,
        );

        out.push_str("# HELP github_ranked_request_duration_seconds Request latency.\n");
        out.push_str("# TYPE github_ranked_request_duration_seconds histogram\n");
        for (index, bound) in LATENCY_BUCKETS.iter().enumerate() {
            out.push_str(&format!(
                "github_ranked_request_duration_seconds_bucket{{le=\"{bound}\"}} {}\n",
                get(&self.latency_buckets[index])
            ));
        }
        let total = get(&self.latency_buckets[LATENCY_BUCKETS.len()]);
        out.push_str(&format!(
            "github_ranked_request_duration_seconds_bucket{{le=\"+Inf\"}} {total}\n"
        ));
        out.push_str(&format!(
            "github_ranked_request_duration_seconds_sum {}\n",
            get(&self.latency_sum_micros) as f64 / 1_000_000.0
        ));
        out.push_str(&format!(
            "github_ranked_request_duration_seconds_count {total}\n"
        ));

        out
    }
}

fn counter(out: &mut String, name: &str, help: &str, value: u64) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {value}\n"
    ));
}

fn gauge(out: &mut String, name: &str, help: &str, value: f64) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate() {
        let metrics = Metrics::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_cache_hit();

        let output = metrics.encode(2, 7);
        assert!(output.contains("github_ranked_requests_total 2"));
        assert!(output.contains("github_ranked_cache_hits_total 1"));
        assert!(output.contains("github_ranked_cache_misses_total 0"));
    }

    #[test]
    fn responses_are_bucketed_by_class() {
        let metrics = Metrics::new();
        metrics.record_response(200, Duration::from_millis(1));
        metrics.record_response(404, Duration::from_millis(1));
        metrics.record_response(500, Duration::from_millis(1));
        metrics.record_response(502, Duration::from_millis(1));

        let output = metrics.encode(1, 0);
        assert!(output.contains(r#"github_ranked_responses_total{class="2xx"} 1"#));
        assert!(output.contains(r#"github_ranked_responses_total{class="4xx"} 1"#));
        assert!(output.contains(r#"github_ranked_responses_total{class="5xx"} 2"#));
    }

    #[test]
    fn histogram_buckets_are_cumulative() {
        let metrics = Metrics::new();
        // 20ms falls in the 0.05 bucket and every wider one, but not 0.01.
        metrics.record_response(200, Duration::from_millis(20));

        let output = metrics.encode(1, 0);
        assert!(output.contains(r#"le="0.01"} 0"#));
        assert!(output.contains(r#"le="0.05"} 1"#));
        assert!(output.contains(r#"le="0.1"} 1"#));
        assert!(output.contains(r#"le="+Inf"} 1"#));
        assert!(output.contains("github_ranked_request_duration_seconds_count 1"));
    }

    #[test]
    fn gauges_reflect_the_sampled_values() {
        let output = Metrics::new().encode(3, 42);
        assert!(output.contains("github_ranked_credentials_available 3"));
        assert!(output.contains("github_ranked_cache_entries 42"));
    }

    /// Every series must be preceded by HELP and TYPE, or scrapers complain.
    #[test]
    fn exposition_format_is_well_formed() {
        let output = Metrics::new().encode(1, 1);

        let helps = output.matches("# HELP ").count();
        let types = output.matches("# TYPE ").count();
        assert_eq!(helps, types, "every metric needs both HELP and TYPE");
        assert!(helps >= 10);

        for line in output.lines().filter(|l| !l.starts_with('#')) {
            assert!(line.contains(' '), "malformed sample line: {line:?}");
        }
    }
}
