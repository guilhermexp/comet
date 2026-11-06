//! Latest-only, in-memory inspection on an already running Chat host.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use zeron_proto::{ChatConfig, ContextAvailability, ContextBreakdown};

#[derive(Default)]
pub(super) struct ContextCache {
    gate: tokio::sync::Mutex<()>,
    latest: Mutex<Option<Cached>>,
    touched: AtomicU64,
}
struct Cached {
    report: ContextBreakdown,
    config: Option<ChatConfig>,
    resume_binding: Option<(String, String, Option<HarnessId>)>,
    completed: tokio::time::Instant,
}

const MAX_CACHED_CHATS: usize = 32;
static NEXT_CACHE_READ: AtomicU64 = AtomicU64::new(0);
fn cache_for(
    caches: &mut HashMap<String, Arc<ContextCache>>,
    chat_id: &str,
) -> Option<Arc<ContextCache>> {
    if !caches.contains_key(chat_id) && caches.len() >= MAX_CACHED_CHATS {
        // Only the map owns an idle entry. Never split an in-flight reader's
        // coalescing gate by replacing its cache while another caller holds it.
        let oldest = caches
            .iter()
            .filter(|(_, cache)| Arc::strong_count(cache) == 1)
            .min_by_key(|(_, cache)| cache.touched.load(Ordering::Relaxed))
            .map(|(id, _)| id.clone())?;
        caches.remove(&oldest);
    }
    let cache = caches.entry(chat_id.to_owned()).or_default().clone();
    cache.touched.store(
        NEXT_CACHE_READ.fetch_add(1, Ordering::Relaxed),
        Ordering::Relaxed,
    );
    Some(cache)
}

// Keep the ownership sweep separate from admission so its lock boundary is testable.
fn prune_caches(
    caches: &Mutex<HashMap<String, Arc<ContextCache>>>,
    mut is_local: impl FnMut(&str) -> bool,
) {
    lock(caches).retain(|id, _| is_local(id));
}

impl SessionsEngine {
    pub async fn context_breakdown(
        &self,
        chat_id: &str,
        selected: Option<ChatConfig>,
    ) -> ContextBreakdown {
        let requested = tokio::time::Instant::now();
        let unavailable = |reason: &str| {
            let mut report =
                ContextBreakdown::unavailable(ContextAvailability::NotApplicable, reason);
            report.chat_id = chat_id.to_owned();
            report.host_device_id = self.inner.device_id.clone();
            report.observed_at_ms = now_ms();
            report
        };
        let cache = {
            let workspace = self.inner.workspace();
            if let Some(workspace) = workspace {
                prune_caches(&self.inner.context_breakdowns, |id| {
                    workspace
                        .chat(id)
                        .ok()
                        .flatten()
                        .is_some_and(|chat| chat.device_id == self.inner.device_id)
                });
            }
            cache_for(&mut lock(&self.inner.context_breakdowns), chat_id)
        };
        let Some(cache) = cache else {
            let mut report = unavailable("Context inspection is busy; refresh to retry");
            report.availability = ContextAvailability::Error;
            return report;
        };
        let discard = || {
            let mut caches = lock(&self.inner.context_breakdowns);
            if caches
                .get(chat_id)
                .is_some_and(|entry| Arc::ptr_eq(entry, &cache))
            {
                caches.remove(chat_id);
            }
        };
        let _gate = cache.gate.lock().await;
        let live = lock(&self.inner.runs)
            .get(chat_id)
            .filter(|run| {
                run.started.load(std::sync::atomic::Ordering::Acquire)
                    && !run.retiring.load(std::sync::atomic::Ordering::Acquire)
            })
            .map(|run| {
                (
                    run.run_id.clone(),
                    run.runtime_config.clone(),
                    run.turn.clone(),
                )
            });
        let selected_matches = |runtime: &RuntimeConfig| {
            selected.as_ref().is_none_or(|selected| {
                let mut request = runtime.request.clone();
                request.model = selected.model.clone();
                request.reasoning = selected.reasoning;
                request.model_options = selected.model_options.clone();
                request.sandbox = selected.sandbox;
                request.access = selected.access;
                request.plan = selected.plan;
                runtime.can_route(selected.harness, &request)
            })
        };
        let selection_current = || {
            self.inner.workspace().is_none_or(|workspace| {
                workspace.chat(chat_id).ok().flatten().is_some_and(|chat| {
                    chat.config == selected && chat.device_id == self.inner.device_id
                })
            })
        };
        if !selection_current() {
            discard();
            return unavailable("Context changed while inspection was queued");
        }
        let Some((run_id, config, control)) = live else {
            return lock(&cache.latest)
                .as_ref()
                .filter(|c| {
                    c.config == selected && c.report.availability == ContextAvailability::Available
                })
                .map(|cached| {
                    let mut report = cached.report.clone();
                    report.stale = true;
                    report
                        .reason
                        .get_or_insert_with(|| "Runtime ended; retained snapshot".into());
                    report
                })
                .unwrap_or_else(|| unavailable("No live runtime to inspect"));
        };
        if !selected_matches(&config) {
            return unavailable("The selected model has no matching live runtime");
        }
        // Resume locators (OMP uses a session-file path) and native session
        // UUIDs are distinct identities. Validate the locator before/after the
        // read without comparing it to the report's native UUID.
        let resume_binding = || {
            self.inner
                .owned_session(chat_id)
                .map(|native| (native.session_id, native.cwd, native.harness))
        };
        let native_before = resume_binding();
        if let Some(cached) = lock(&cache.latest).as_ref().filter(|cached| {
            cached.config == selected
                && cached.report.runtime_id == run_id
                && cached.resume_binding == native_before
                && (cached.completed >= requested
                    || cached.report.availability == ContextAvailability::Unsupported)
        }) {
            return cached.report.clone();
        }
        let result = control.inspect_context().await;
        // Validate again after the read. Replacement, reconfiguration or selection
        // changes never publish a response as current for a different runtime.
        let still_current = lock(&self.inner.runs).get(chat_id).is_some_and(|run| {
            run.run_id == run_id
                && !run.retiring.load(std::sync::atomic::Ordering::Acquire)
                && config.can_route(run.runtime_config.harness_id, &run.runtime_config.request)
        });
        let selection_is_current = selection_current();
        if !selection_is_current {
            discard();
        }
        if !still_current || !selection_is_current {
            return unavailable("Context changed during inspection; refresh to inspect it");
        }
        if resume_binding() != native_before {
            return unavailable("Native session changed during inspection");
        }
        let mut report = match result {
            Ok(report) => report,
            Err(zeron_harness::HarnessError::Unsupported(reason)) => {
                ContextBreakdown::unavailable(ContextAvailability::Unsupported, reason)
            }
            Err(zeron_harness::HarnessError::Protocol(reason)) => {
                ContextBreakdown::unavailable(ContextAvailability::Error, reason)
            }
            Err(_) => ContextBreakdown::unavailable(
                ContextAvailability::Error,
                "Context inspection failed or timed out",
            ),
        };
        if matches!(
            report.availability,
            ContextAvailability::Error
                | ContextAvailability::NotApplicable
                | ContextAvailability::Unsupported
        ) && let Some(cached) = lock(&cache.latest).as_mut().filter(|cached| {
            cached.report.availability == ContextAvailability::Available
                && cached.config == selected
                && cached.report.runtime_id == run_id
                && cached.resume_binding == native_before
        }) {
            let mut retained = cached.report.clone();
            retained.stale = true;
            retained.reason = Some(format!(
                "{}; retained snapshot",
                report
                    .reason
                    .as_deref()
                    .unwrap_or("Native context unavailable")
            ));
            cached.report = retained.clone();
            cached.completed = tokio::time::Instant::now();
            return retained;
        }
        if report.source.is_empty() {
            report.source = config.harness_id.as_str().into();
        }
        report.chat_id = chat_id.to_owned();
        report.host_device_id = self.inner.device_id.clone();
        report.runtime_id = run_id;
        report.observed_at_ms = now_ms();
        *lock(&cache.latest) = Some(Cached {
            report: report.clone(),
            config: selected,
            resume_binding: native_before,
            completed: tokio::time::Instant::now(),
        });
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_pruning_queries_without_map_lock_and_keeps_active_gates() {
        let caches = Mutex::new(HashMap::new());
        for id in ["local", "missing", "foreign", "read-error"] {
            drop(cache_for(&mut lock(&caches), id).unwrap());
        }
        let active = cache_for(&mut lock(&caches), "active-missing").unwrap();
        let gate = active.gate.try_lock().unwrap();
        let mut checked = Vec::new();
        let mut available_during_query = true;
        prune_caches(&caches, |id| {
            available_during_query &= caches.try_lock().is_ok();
            checked.push(id.to_owned());
            id == "local"
        });
        let mut remaining = lock(&caches).keys().cloned().collect::<Vec<_>>();
        remaining.sort();
        drop(gate);
        drop(active);
        prune_caches(&caches, |id| id == "local");
        let after_release = lock(&caches).keys().cloned().collect::<Vec<_>>();
        assert_eq!(checked.len(), 5, "all captured IDs must be queried");
        assert!(
            available_during_query,
            "workspace lookup must run without the cache-map mutex"
        );
        assert_eq!(
            remaining,
            ["active-missing", "local"],
            "active gates cannot be removed by ownership pruning"
        );
        assert_eq!(
            after_release,
            ["local"],
            "inactive missing/foreign/unreadable entries must be removed"
        );
    }

    #[test]
    fn cache_pruning_does_not_remove_a_replacement_created_during_lookup() {
        let caches = Mutex::new(HashMap::new());
        drop(cache_for(&mut lock(&caches), "missing").unwrap());
        let mut replacement = None;
        prune_caches(&caches, |id| {
            assert_eq!(id, "missing");
            if let Ok(mut map) = caches.try_lock() {
                let next = Arc::new(ContextCache::default());
                replacement = Some(Arc::downgrade(&next));
                map.insert(id.to_owned(), next);
            }
            false
        });
        assert!(
            replacement.is_some(),
            "lookup must allow concurrent cache admission"
        );
        let map = lock(&caches);
        assert!(
            map.get("missing").is_some_and(|current| {
                std::sync::Weak::ptr_eq(replacement.as_ref().unwrap(), &Arc::downgrade(current))
            }),
            "the stale lookup result must not remove a new cache with the same ID"
        );
    }

    #[test]
    fn context_cache_bounds_chats_and_spares_inflight_readers() {
        let mut caches = HashMap::new();
        for index in 0..MAX_CACHED_CHATS * 2 {
            drop(cache_for(&mut caches, &index.to_string()).unwrap());
            assert!(
                caches.len() <= MAX_CACHED_CHATS,
                "retained Chat snapshots must be bounded"
            );
        }
        caches.clear();
        let mut active = Vec::new();
        for index in 0..MAX_CACHED_CHATS {
            active.push(cache_for(&mut caches, &index.to_string()).unwrap());
        }
        assert!(
            cache_for(&mut caches, "overflow").is_none(),
            "all retained readers are active"
        );
        assert!(Arc::ptr_eq(
            &active[0],
            &cache_for(&mut caches, "0").unwrap()
        ));
        active.remove(0);
        assert!(cache_for(&mut caches, "overflow").is_some());
        assert_eq!(caches.len(), MAX_CACHED_CHATS);
    }
}
