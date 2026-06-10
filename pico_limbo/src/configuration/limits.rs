use serde::{Deserialize, Serialize};

/// Connection timeouts. These bound how long a single TCP connection may stay
/// idle before the server drops it, releasing its file descriptor and memory.
///
/// They are the primary defence against connection-exhaustion floods (slowloris,
/// half-open connections, port scanners) in a reverse-proxy / DDoS-mitigation
/// deployment, where a large number of connections — valid or not — may arrive
/// at once. Set a value to `0` to disable that timeout.
#[derive(Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    /// Maximum number of seconds a connection may spend in the pre-play phases
    /// (handshake, status, login, configuration) before being dropped.
    /// Connections that open but never start playing are reaped after this.
    /// Default: 30. Lower it (e.g. 5-10) to reap idle/abusive connections faster.
    pub login_timeout: u64,

    /// Maximum number of seconds a connection that has reached the play phase may
    /// go without sending any packet (including keep-alive responses) before being
    /// dropped. Default: 30.
    pub read_timeout: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            login_timeout: 30,
            read_timeout: 30,
        }
    }
}
