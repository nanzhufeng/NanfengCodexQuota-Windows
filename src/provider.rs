use crate::{codex::CodexClient, limits::RateLimits, monitor::Failure};
use std::{path::PathBuf, time::Duration};

pub struct Provider {
    explicit: Option<PathBuf>,
    cached: Option<PathBuf>,
    cancellation: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Provider {
    pub fn new(explicit: Option<PathBuf>) -> Self {
        Self {
            explicit,
            cached: None,
            cancellation: Default::default(),
        }
    }
    pub fn with_cancellation(
        mut self,
        cancellation: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        self.cancellation = cancellation;
        self
    }
    pub fn read(&mut self) -> Result<(RateLimits, String), Failure> {
        let candidates = crate::discovery::discover(self.explicit.as_deref());
        if candidates.is_empty() {
            self.cached = None;
            return Err(Failure::NotInstalled);
        }
        let mut paths = Vec::new();
        if let Some(p) = &self.cached
            && p.is_file()
        {
            paths.push(p.clone());
        }
        for c in candidates {
            if !paths.contains(&c.path) {
                paths.push(c.path);
            }
        }
        let mut failure = Failure::Protocol;
        for path in paths.into_iter().take(3) {
            if self.cancellation.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            match CodexClient::new(&path)
                .with_timeout(Duration::from_secs(8))
                .with_cancellation(self.cancellation.clone())
                .read_rate_limits()
            {
                Ok(limits) => {
                    self.cached = Some(path.clone());
                    return Ok((limits, path.display().to_string()));
                }
                Err(error) => {
                    failure = classify(&format!("{error:#}"));
                    if matches!(failure, Failure::NotAuthenticated | Failure::Network) {
                        return Err(failure);
                    }
                }
            }
        }
        self.cached = None;
        Err(failure)
    }
}
fn classify(message: &str) -> Failure {
    let s = message.to_ascii_lowercase();
    if [
        "unauthorized",
        "not authenticated",
        "not logged",
        "log in",
        "401",
        "authentication",
        "refresh token",
    ]
    .iter()
    .any(|t| s.contains(t))
    {
        Failure::NotAuthenticated
    } else if [
        "network",
        "connection",
        "dns",
        "error sending request",
        "fetch",
        "http",
        "tls",
    ]
    .iter()
    .any(|t| s.contains(t))
    {
        Failure::Network
    } else if s.contains("timed out") || s.contains("timeout") {
        Failure::Timeout
    } else {
        Failure::Protocol
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_is_actionable_without_disclosing_raw_error() {
        assert_eq!(classify("401 Unauthorized"), Failure::NotAuthenticated);
        assert_eq!(classify("invalid value untrusted"), Failure::Protocol);
        assert_eq!(classify("error sending request"), Failure::Network);
    }
}
