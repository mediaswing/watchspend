//! Asking GitHub, once at startup, whether there is a newer release.
//!
//! Deliberately the mildest kind of update check there is: it downloads
//! nothing, installs nothing, and runs nothing. It reads one small piece of
//! JSON, and if the version in it is higher than this one, the app offers to
//! open the release page in a browser. Everything after that is the user's
//! doing, which is the difference between an update prompt and an update
//! mechanism — and the reason this one cannot be turned into a way of getting
//! code onto someone's machine.
//!
//! It is also a request to a third party from a program about someone's
//! money, so: it can be turned off, it is off for anyone who never says yes to
//! it being on, and it says nothing at all when the network is not there.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

/// Where the check asks, and where the user is sent.
const RELEASES_API: &str = "https://api.github.com/repos/mediaswing/watchspend/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/mediaswing/watchspend/releases/latest";

/// This build's version, from the manifest, so the two cannot drift apart.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

const TIMEOUT: Duration = Duration::from_secs(5);
/// The answer is a few hundred bytes. Anything wildly bigger is not the answer.
const MAX_RESPONSE: u64 = 256 * 1024;

/// A release newer than this build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    /// The version, without the leading `v`.
    pub version: String,
    /// Where to send someone who wants it.
    pub page: String,
}

/// A check running on another thread. Dropping it abandons the answer, which
/// is what should happen if the window is closed while it is in flight.
pub struct Check {
    receiver: Option<Receiver<Option<Update>>>,
}

impl Check {
    /// Start a check in the background. The UI never waits on this: a slow or
    /// unreachable network must not hold up a budgeting app that has no need
    /// of the network at all.
    pub fn start() -> Self {
        let (sender, receiver) = channel();
        std::thread::Builder::new()
            .name("update-check".to_owned())
            .spawn(move || {
                let result = look_for_newer(CURRENT);
                // The receiver is gone if the app closed first; that is fine.
                let _ = sender.send(result);
            })
            .map_or_else(
                |err| {
                    log::warn!("could not start the update check: {err}");
                    Self { receiver: None }
                },
                |_handle| Self {
                    receiver: Some(receiver),
                },
            )
    }

    /// A check that was never started, for when the user has turned it off.
    pub fn disabled() -> Self {
        Self { receiver: None }
    }

    /// Is the check still out? Used to keep the window repainting until the
    /// answer is in, since nothing else would wake it.
    pub fn is_running(&self) -> bool {
        self.receiver.is_some()
    }

    /// The answer, once, if it has arrived.
    pub fn poll(&mut self) -> Option<Update> {
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok(update) => {
                self.receiver = None;
                update
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                None
            }
        }
    }
}

/// Ask, and say nothing if the asking fails.
///
/// Every failure here is silent bar a log line. There is no version of "could
/// not reach GitHub" that a person sitting down to enter yesterday's shopping
/// needs to read.
fn look_for_newer(current: &str) -> Option<Update> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // A redirect off HTTPS is not something to follow.
        .https_only(true)
        .max_redirects(3)
        .user_agent(concat!(
            "generic-accounting-system/",
            env!("CARGO_PKG_VERSION")
        ))
        // The platform's own TLS, which the MariaDB connection already uses.
        // Carrying a second TLS stack in one small app would mean two sets of
        // root certificates to keep straight and two sets of advisories to
        // follow.
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .build()
        .new_agent();

    let body = agent
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .call()
        .inspect_err(|err| log::info!("update check did not get through: {err}"))
        .ok()?
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE)
        .read_to_string()
        .inspect_err(|err| log::info!("update check could not be read: {err}"))
        .ok()?;

    parse(&body, current)
}

/// Pull the version out of GitHub's answer, and only return it if it is newer.
fn parse(body: &str, current: &str) -> Option<Update> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("draft") == Some(&serde_json::Value::Bool(true))
        || value.get("prerelease") == Some(&serde_json::Value::Bool(true))
    {
        return None;
    }

    let tag = value.get("tag_name")?.as_str()?;
    let version = tag.trim_start_matches('v').trim();
    if !is_newer(version, current) {
        return None;
    }

    // The page comes from the response, so it is checked rather than trusted:
    // this app will not hand an arbitrary URL to the browser on the say-so of
    // an HTTP response.
    let page = value
        .get("html_url")
        .and_then(|url| url.as_str())
        .filter(|url| url.starts_with("https://github.com/mediaswing/watchspend/releases/"))
        .unwrap_or(RELEASES_PAGE);

    Some(Update {
        version: version.to_owned(),
        page: page.to_owned(),
    })
}

/// Compare two `major.minor.patch` versions.
///
/// Anything that does not parse loses, which means a malformed tag on the
/// releases page cannot produce a prompt.
fn is_newer(candidate: &str, current: &str) -> bool {
    match (parts(candidate), parts(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

fn parts(version: &str) -> Option<(u32, u32, u32)> {
    // Ignore any pre-release or build suffix: 1.2.3-rc1 is compared as 1.2.3,
    // and a pre-release is never offered in the first place.
    let core = version.split(['-', '+']).next()?;
    let mut numbers = core.split('.');
    let major = numbers.next()?.parse().ok()?;
    let minor = numbers.next().unwrap_or("0").parse().ok()?;
    let patch = numbers.next().unwrap_or("0").parse().ok()?;
    if numbers.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> String {
        format!(
            r#"{{"tag_name": "{tag}",
                "html_url": "https://github.com/mediaswing/watchspend/releases/tag/{tag}",
                "draft": false, "prerelease": false}}"#
        )
    }

    #[test]
    fn a_higher_version_is_offered() {
        let update = parse(&release("v1.1.0"), "1.0.0").expect("an update");
        assert_eq!(update.version, "1.1.0");
        assert!(update.page.ends_with("/tag/v1.1.0"));
    }

    #[test]
    fn the_same_or_older_is_not() {
        assert!(parse(&release("v1.0.0"), "1.0.0").is_none());
        assert!(parse(&release("v0.9.9"), "1.0.0").is_none());
        assert!(parse(&release("v1.0.0"), "1.0.1").is_none());
    }

    #[test]
    fn versions_compare_as_numbers_not_as_text() {
        // The whole point: "10" sorts before "9" as text and after it here.
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.9.0", "1.10.0"));
        assert!(is_newer("1.0.1", "1.0.0"));
    }

    #[test]
    fn nonsense_never_produces_a_prompt() {
        assert!(!is_newer("banana", "1.0.0"));
        assert!(!is_newer("", "1.0.0"));
        assert!(!is_newer("1.0.0.1", "1.0.0"));
        assert!(parse("not json at all", "1.0.0").is_none());
        assert!(parse("{}", "1.0.0").is_none());
        assert!(parse(&release("v"), "1.0.0").is_none());
    }

    #[test]
    fn drafts_and_pre_releases_are_left_alone() {
        let draft = r#"{"tag_name":"v9.0.0","draft":true,"prerelease":false}"#;
        let pre = r#"{"tag_name":"v9.0.0","draft":false,"prerelease":true}"#;
        assert!(parse(draft, "1.0.0").is_none());
        assert!(parse(pre, "1.0.0").is_none());
    }

    #[test]
    fn a_url_pointing_somewhere_else_is_not_followed() {
        let hostile = r#"{"tag_name":"v2.0.0","draft":false,"prerelease":false,
                          "html_url":"https://example.invalid/phish"}"#;
        let update = parse(hostile, "1.0.0").expect("an update");
        assert_eq!(update.page, RELEASES_PAGE);
    }

    #[test]
    fn this_build_knows_its_own_version() {
        assert!(parts(CURRENT).is_some(), "CARGO_PKG_VERSION is {CURRENT}");
    }
}
