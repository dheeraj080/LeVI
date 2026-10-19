// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-phase request timing.
//!
//! Three phases describe the exchange (total, time to first byte, receive) and three
//! describe the *connection* (dns, connect, tls). The connection phases are present
//! only when this request opened a connection: a pooled client reuses one, and a
//! request served from the pool reports them as `None` rather than as zero. What that
//! means is documented on the connector module that measures them.

use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timings {
    /// Whole request, from just before dispatch to the last body byte.
    pub total: Duration,
    /// Time to first byte: from dispatch to response headers.
    pub ttfb: Duration,
    /// Reading the response body.
    pub receive: Duration,
    /// DNS resolution. Absent when this request did not resolve anything.
    pub dns: Option<Duration>,
    /// TCP connect. Absent when this request reused a pooled connection.
    pub connect: Option<Duration>,
    /// TLS handshake. Absent for a reused connection, or for plain `http`.
    pub tls: Option<Duration>,
}

impl fmt::Display for Timings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  total      {:>10}", format_duration(self.total))?;
        writeln!(f, "  ttfb       {:>10}", format_duration(self.ttfb))?;
        writeln!(f, "  receive    {:>10}", format_duration(self.receive))?;
        for (label, value) in [
            ("dns", self.dns),
            ("connect", self.connect),
            ("tls", self.tls),
        ] {
            let shown = value
                .map(format_duration)
                .unwrap_or_else(|| "n/a".to_string());
            writeln!(f, "  {label:<10} {shown:>10}")?;
        }
        Ok(())
    }
}

fn format_duration(duration: Duration) -> String {
    let micros = duration.as_micros();
    if micros < 1_000 {
        format!("{micros} us")
    } else {
        format!("{:.3} ms", duration.as_secs_f64() * 1_000.0)
    }
}
