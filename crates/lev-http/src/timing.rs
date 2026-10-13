// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-phase request timing.
//!
//! Phase 0 measures the phases the pooled high-level client can see: total, time to
//! first byte (response headers), and receive. DNS / connect / TLS need the
//! instrumented connector and are populated in Phase 1; until then they are `None`
//! and printed as `n/a`.

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
    /// DNS resolution (Phase 1 connector instrumentation).
    pub dns: Option<Duration>,
    /// TCP connect (Phase 1 connector instrumentation).
    pub connect: Option<Duration>,
    /// TLS handshake (Phase 1 connector instrumentation).
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
