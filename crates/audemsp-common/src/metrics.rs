//! Prometheus metrics helpers.
//!
//! Provides convenience functions to register standard metrics
//! used by all three components.

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

/// Common metrics registry shared across all components.
pub struct CoreMetrics {
    registry: Registry,
    pub active_connections: Gauge,
    pub relayed_bytes: Counter<u64>,
    pub signaling_latency_us: Gauge,
    pub error_count: Counter<u64>,
}

impl CoreMetrics {
    /// Create a new metrics instance.
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let active_connections = Gauge::default();
        registry.register(
            "active_connections",
            "Number of active WebSocket/WebRTC connections",
            active_connections.clone(),
        );

        let relayed_bytes = Counter::default();
        registry.register(
            "relayed_bytes",
            "Total bytes relayed (Server only)",
            relayed_bytes.clone(),
        );

        let signaling_latency_us = Gauge::default();
        registry.register(
            "signaling_latency_us",
            "Signaling message latency in microseconds",
            signaling_latency_us.clone(),
        );

        let error_count = Counter::default();
        registry.register(
            "error_count",
            "Total error count by error code",
            error_count.clone(),
        );

        Self {
            registry,
            active_connections,
            relayed_bytes,
            signaling_latency_us,
            error_count,
        }
    }

    /// Encode all metrics in Prometheus text format.
    pub fn encode(&self) -> String {
        let mut buf = String::new();
        encode(&mut buf, &self.registry).expect("metrics encoding infallible");
        buf
    }
}

impl Default for CoreMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_metrics_encodes_valid_prometheus() {
        let metrics = CoreMetrics::new();
        let output = metrics.encode();
        // Prometheus text format should have HELP and TYPE for each metric
        assert!(output.contains("# HELP active_connections"), "missing HELP active_connections");
        assert!(output.contains("# TYPE active_connections gauge"), "missing TYPE active_connections");
        assert!(output.contains("# HELP relayed_bytes"), "missing HELP relayed_bytes");
        assert!(output.contains("# TYPE relayed_bytes counter"), "missing TYPE relayed_bytes");
        assert!(output.contains("# HELP signaling_latency_us"), "missing HELP signaling_latency_us");
        assert!(output.contains("# TYPE signaling_latency_us gauge"), "missing TYPE signaling_latency_us");
        assert!(output.contains("# HELP error_count"), "missing HELP error_count");
        assert!(output.contains("# TYPE error_count counter"), "missing TYPE error_count");
    }

    #[test]
    fn new_metrics_starts_at_zero() {
        let metrics = CoreMetrics::new();
        let output = metrics.encode();
        // All metrics should start at 0
        assert!(output.contains("active_connections 0"), "active_connections should be 0");
        assert!(output.contains("relayed_bytes_total 0"), "relayed_bytes should be 0");
        assert!(output.contains("signaling_latency_us 0"), "signaling_latency_us should be 0");
        assert!(output.contains("error_count_total 0"), "error_count should be 0");
    }

    #[test]
    fn counter_increment_reflected_in_encode() {
        let metrics = CoreMetrics::new();
        metrics.relayed_bytes.inc_by(1024);
        metrics.error_count.inc_by(3);
        let output = metrics.encode();
        assert!(output.contains("relayed_bytes_total 1024"), "expected relayed_bytes_total 1024");
        assert!(output.contains("error_count_total 3"), "expected error_count_total 3");
    }

    #[test]
    fn gauge_set_reflected_in_encode() {
        let metrics = CoreMetrics::new();
        metrics.active_connections.set(5);
        metrics.signaling_latency_us.set(1500);
        let output = metrics.encode();
        assert!(output.contains("active_connections 5"), "expected active_connections 5");
        assert!(output.contains("signaling_latency_us 1500"), "expected signaling_latency_us 1500");
    }

    #[test]
    fn counter_multiple_increments() {
        let metrics = CoreMetrics::new();
        metrics.relayed_bytes.inc_by(100);
        metrics.relayed_bytes.inc_by(200);
        let output = metrics.encode();
        assert!(output.contains("relayed_bytes_total 300"), "expected relayed_bytes_total 300");
    }

    #[test]
    fn default_impl_works() {
        let metrics: CoreMetrics = Default::default();
        let output = metrics.encode();
        assert!(output.contains("# HELP active_connections"));
        assert!(output.contains("active_connections 0"));
    }
}
