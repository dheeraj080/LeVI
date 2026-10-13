// SPDX-License-Identifier: MIT OR Apache-2.0
//! Protocol versioning and negotiation.
//!
//! The rule is deliberately strict and boring: **majors must match exactly**.
//! Within a major, both sides speak the lower of the two minors, so adding a
//! message is a non-breaking change and removing one is a major bump.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The protocol version this build speaks.
pub const PROTOCOL_VERSION: Version = Version::new(0, 2);

/// A `major.minor` protocol version.
///
/// The major version changes only when a message is removed or its meaning
/// changes. Adding a message or an optional field is a minor bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// The peer and the engine cannot agree on a protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NegotiationError {
    #[error(
        "incompatible protocol: peer speaks {peer}, this build speaks {local} \
         (major versions must match)"
    )]
    MajorMismatch { peer: Version, local: Version },
}

/// Agree on a version to speak, given the peer's and our own.
///
/// Requires equal majors and yields the lower minor.
pub fn negotiate(peer: Version, local: Version) -> Result<Version, NegotiationError> {
    if peer.major != local.major {
        return Err(NegotiationError::MajorMismatch { peer, local });
    }
    Ok(Version::new(local.major, peer.minor.min(local.minor)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_versions_negotiate_to_the_lower_minor() {
        let local = Version::new(0, 3);
        assert_eq!(
            negotiate(Version::new(0, 1), local).unwrap(),
            Version::new(0, 1)
        );
        assert_eq!(
            negotiate(Version::new(0, 3), local).unwrap(),
            Version::new(0, 3)
        );
        assert_eq!(
            negotiate(Version::new(0, 9), local).unwrap(),
            Version::new(0, 3)
        );
    }

    #[test]
    fn a_different_major_is_rejected() {
        let error = negotiate(Version::new(1, 0), PROTOCOL_VERSION).unwrap_err();
        assert_eq!(
            error,
            NegotiationError::MajorMismatch {
                peer: Version::new(1, 0),
                local: PROTOCOL_VERSION,
            }
        );
    }

    #[test]
    fn version_renders_as_major_dot_minor() {
        assert_eq!(Version::new(1, 4).to_string(), "1.4");
    }
}
