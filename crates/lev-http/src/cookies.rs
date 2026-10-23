// SPDX-License-Identifier: MIT OR Apache-2.0
//! A small RFC 6265 cookie jar.
//!
//! The jar is deliberately in-memory and scoped to one [`crate::HttpClient`]. It is
//! what makes a sequence of API calls a *session*: log in, receive a session cookie,
//! and have every later request carry it without the collection author threading a
//! value through variables by hand.
//!
//! The rules implemented are the ones that matter for an HTTP client:
//! host-only and domain cookies, path matching, `Secure`, `Max-Age`/`Expires`, and
//! duplicate replacement. `HttpOnly` is parsed but has no effect here — this is an
//! HTTP client, not a browser, so there is no script to protect from.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hyper::header::HeaderValue;
use url::Url;

/// Monotonic creation order, so cookies with equal paths are sent in the order they
/// arrived. A process-wide counter is enough: no cookie outlives the process.
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CookieKey {
    domain: String,
    path: String,
    name: String,
}

#[derive(Debug, Clone)]
struct StoredCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    host_only: bool,
    secure: bool,
    expires: Option<SystemTime>,
    created: u64,
}

/// A shared, thread-safe cookie jar.
///
/// Cloning the handle clones the reference: every virtual user on a load run that
/// shares one client shares its cookie state. Use [`crate::ClientConfig::without_cookies`]
/// when each virtual user must be independent.
#[derive(Debug, Clone, Default)]
pub(crate) struct CookieJar {
    store: Arc<Mutex<HashMap<CookieKey, StoredCookie>>>,
}

impl CookieJar {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<CookieKey, StoredCookie>> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The `Cookie` header value for `url`, or `None` when no cookie matches.
    pub(crate) fn header_for(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        let request_path = url.path();
        let secure = url.scheme() == "https";
        let now = SystemTime::now();

        let mut store = self.lock();
        store.retain(|_, cookie| cookie.expires.is_none_or(|expires| expires > now));

        let mut matching: Vec<&StoredCookie> = store
            .values()
            .filter(|cookie| {
                (!cookie.secure || secure)
                    && domain_matches(&host, cookie)
                    && path_matches(request_path, &cookie.path)
            })
            .collect();
        // RFC 6265 §5.4: longer paths first, then earlier creation.
        matching.sort_by(|left, right| {
            right
                .path
                .len()
                .cmp(&left.path.len())
                .then(left.created.cmp(&right.created))
        });

        if matching.is_empty() {
            return None;
        }
        Some(
            matching
                .into_iter()
                .map(|cookie| format!("{}={}", cookie.name, cookie.value))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    /// Store every `Set-Cookie` value in `values` that applies to `url`.
    pub(crate) fn store_headers<'a, I>(&self, url: &Url, values: I)
    where
        I: IntoIterator<Item = &'a HeaderValue>,
    {
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return;
        };
        let now = SystemTime::now();
        let mut store = self.lock();

        for value in values {
            let Ok(raw) = value.to_str() else {
                continue;
            };
            let Some(cookie) = parse_set_cookie(raw, &host, url.path(), now) else {
                continue;
            };
            let key = CookieKey {
                domain: cookie.domain.clone(),
                path: cookie.path.clone(),
                name: cookie.name.clone(),
            };
            if cookie.expires.is_some_and(|expires| expires <= now) {
                store.remove(&key);
            } else {
                store.insert(key, cookie);
            }
        }
    }
}

fn domain_matches(host: &str, cookie: &StoredCookie) -> bool {
    if cookie.host_only {
        return host.eq_ignore_ascii_case(&cookie.domain);
    }
    host.eq_ignore_ascii_case(&cookie.domain)
        || host
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", cookie.domain.to_ascii_lowercase()))
}

fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if !request_path.starts_with(cookie_path) {
        return false;
    }
    cookie_path.ends_with('/')
        || request_path.as_bytes().get(cookie_path.len()).copied() == Some(b'/')
}

fn default_path(request_path: &str) -> String {
    if !request_path.starts_with('/') {
        return "/".to_string();
    }
    match request_path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => request_path[..index].to_string(),
    }
}

fn parse_set_cookie(
    raw: &str,
    host: &str,
    request_path: &str,
    now: SystemTime,
) -> Option<StoredCookie> {
    let mut segments = raw.split(';');
    let pair = segments.next()?.trim();
    let (name, value) = pair.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let value = value.trim().to_string();

    let mut domain_attr: Option<String> = None;
    let mut path_attr: Option<String> = None;
    let mut secure = false;
    let mut max_age: Option<i64> = None;
    let mut expires: Option<SystemTime> = None;

    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let (key, attribute) = match segment.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (segment, ""),
        };
        if key.eq_ignore_ascii_case("domain") {
            let domain = attribute.trim_start_matches('.').to_ascii_lowercase();
            if !domain.is_empty() {
                domain_attr = Some(domain);
            }
        } else if key.eq_ignore_ascii_case("path") {
            if attribute.starts_with('/') {
                path_attr = Some(attribute.to_string());
            }
        } else if key.eq_ignore_ascii_case("secure") {
            secure = true;
        } else if key.eq_ignore_ascii_case("max-age") {
            max_age = attribute.parse::<i64>().ok();
        } else if key.eq_ignore_ascii_case("expires") {
            expires = parse_http_date(attribute);
        }
    }

    let (domain, host_only) = match domain_attr {
        Some(domain) => {
            if domain != host
                && !host
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{}", domain.to_ascii_lowercase()))
            {
                return None;
            }
            (domain, false)
        }
        None => (host.to_string(), true),
    };

    // `Max-Age` wins over `Expires` when both are present (RFC 6265 §5.3).
    let expires = match max_age {
        Some(seconds) if seconds <= 0 => Some(now),
        Some(seconds) => Some(now + Duration::from_secs(seconds as u64)),
        None => expires,
    };

    Some(StoredCookie {
        name: name.to_string(),
        value,
        domain,
        path: path_attr.unwrap_or_else(|| default_path(request_path)),
        host_only,
        secure,
        expires,
        created: NEXT_ID.fetch_add(1, Ordering::Relaxed),
    })
}

/// Parse the IMF-fixdate form of an HTTP date.
///
/// `Set-Cookie` timestamps are advisory; a malformed one simply means "session
/// cookie", which is the safe direction. The RFC 850 and asctime forms are rare
/// enough that treating them as invalid is preferable to guessing a year.
fn parse_http_date(value: &str) -> Option<SystemTime> {
    let value = value
        .rsplit_once(',')
        .map_or(value, |(_, rest)| rest)
        .trim();
    let mut parts = value.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i32 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour: u32 = clock.next()?.parse().ok()?;
    let minute: u32 = clock.next()?.parse().ok()?;
    let second: u32 = clock.next()?.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 || day == 0 || day > 31 {
        return None;
    }

    let seconds = unix_seconds(year, month, day, hour, minute, second.min(59))?;
    if seconds < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(seconds as u64))
}

/// Days since 1970-01-01, by Howard Hinnant's `days_from_civil`.
fn unix_seconds(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month as i32 + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = (era * 146_097 + day_of_era - 719_468) as i64;
    Some(days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn a_host_only_cookie_is_sent_back_to_the_same_host() {
        let jar = CookieJar::new();
        let origin = url("https://api.test/v1/login");
        jar.store_headers(
            &origin,
            [&HeaderValue::from_static("session=abc; Path=/; HttpOnly")],
        );

        assert_eq!(
            jar.header_for(&url("https://api.test/v1/orders")),
            Some("session=abc".to_string())
        );
        assert_eq!(jar.header_for(&url("https://other.test/")), None);
    }

    #[test]
    fn a_domain_cookie_reaches_subdomains() {
        let jar = CookieJar::new();
        jar.store_headers(
            &url("https://api.test/"),
            [&HeaderValue::from_static("theme=dark; Domain=.test")],
        );
        assert_eq!(
            jar.header_for(&url("https://www.test/")),
            Some("theme=dark".to_string())
        );
    }

    #[test]
    fn path_matching_respects_segment_boundaries() {
        let jar = CookieJar::new();
        jar.store_headers(
            &url("https://api.test/"),
            [&HeaderValue::from_static("a=1; Path=/v1")],
        );
        assert!(jar.header_for(&url("https://api.test/v1/orders")).is_some());
        assert!(jar.header_for(&url("https://api.test/v1")).is_some());
        assert!(jar.header_for(&url("https://api.test/v10")).is_none());
    }

    #[test]
    fn secure_cookies_stay_on_https() {
        let jar = CookieJar::new();
        jar.store_headers(
            &url("https://api.test/"),
            [&HeaderValue::from_static("s=1; Secure; Path=/")],
        );
        assert!(jar.header_for(&url("https://api.test/")).is_some());
        assert!(jar.header_for(&url("http://api.test/")).is_none());
    }

    #[test]
    fn a_replacement_keeps_only_the_new_value() {
        let jar = CookieJar::new();
        let origin = url("https://api.test/");
        jar.store_headers(&origin, [&HeaderValue::from_static("a=1; Path=/")]);
        jar.store_headers(&origin, [&HeaderValue::from_static("a=2; Path=/")]);
        assert_eq!(jar.header_for(&origin), Some("a=2".to_string()));
    }

    #[test]
    fn a_max_age_in_the_past_deletes_the_cookie() {
        let jar = CookieJar::new();
        let origin = url("https://api.test/");
        jar.store_headers(&origin, [&HeaderValue::from_static("a=1; Path=/")]);
        jar.store_headers(
            &origin,
            [&HeaderValue::from_static("a=gone; Path=/; Max-Age=0")],
        );
        assert_eq!(jar.header_for(&origin), None);
    }

    #[test]
    fn an_expires_timestamp_is_understood() {
        let parsed = parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT").unwrap();
        let seconds = parsed.duration_since(UNIX_EPOCH).unwrap().as_secs();
        // 2015-10-21T07:28:00Z
        assert_eq!(seconds, 1_445_412_480);
    }

    #[test]
    fn a_domain_that_does_not_match_is_refused() {
        let jar = CookieJar::new();
        jar.store_headers(
            &url("https://api.test/"),
            [&HeaderValue::from_static("a=1; Domain=evil.test")],
        );
        assert_eq!(jar.header_for(&url("https://api.test/")), None);
    }
}
