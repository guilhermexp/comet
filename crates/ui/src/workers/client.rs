//! The one `LocalWorkersClient` the app talks through. Five constructors used
//! to race the same process-wide request-id counter and replay cache
//! (`crates/workers-unpeel/AGENTS.md`); one instance keeps that invariant
//! at the call site instead of inside the counter.
use std::sync::LazyLock;

use zeron_workers_unpeel::LocalWorkersClient;

/// Cheap clone: the client is four `Arc`s.
pub(crate) fn shared() -> LocalWorkersClient {
    static CLIENT: LazyLock<LocalWorkersClient> = LazyLock::new(LocalWorkersClient::new);
    CLIENT.clone()
}
