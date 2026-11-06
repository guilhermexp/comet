//! Read-only control mailbox, separate from prompts, stop and configuration.
use crate::HarnessError;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use zeron_proto::ContextBreakdown;

pub struct ContextInspectionRequest {
    pub deadline: tokio::time::Instant,
    pub reply: oneshot::Sender<Result<ContextBreakdown, HarnessError>>,
}

#[derive(Clone, Default)]
pub(crate) struct ContextInspection(Arc<Mutex<Option<mpsc::Sender<ContextInspectionRequest>>>>);
impl ContextInspection {
    pub fn attach(&self) -> mpsc::Receiver<ContextInspectionRequest> {
        let (tx, rx) = mpsc::channel(8);
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
        rx
    }
    pub async fn inspect(&self) -> Result<ContextBreakdown, HarnessError> {
        let sender = self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| {
                HarnessError::Unsupported(
                    "Native context inspection is unavailable for this runtime".into(),
                )
            })?;
        let (reply, rx) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        sender
            .try_send(ContextInspectionRequest { deadline, reply })
            .map_err(|_| {
                HarnessError::Protocol("Context inspection mailbox is closed or busy".into())
            })?;
        tokio::time::timeout_at(deadline, rx)
            .await
            .map_err(|_| HarnessError::Protocol("Context inspection timed out".into()))?
            .map_err(|_| HarnessError::Protocol("Context inspection runtime exited".into()))?
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    /// Drive the actual session future with a counting waker. A closed recv
    /// that spins exhausts Tokio's cooperative budget and wakes itself again;
    /// a parked select registers the waker but does not schedule another poll.
    pub(crate) async fn idle_poll_wakes(
        mut driver: std::pin::Pin<&mut impl std::future::Future<Output = ()>>,
    ) -> usize {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Wakes(AtomicUsize);
        impl std::task::Wake for Wakes {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        // Give the next poll a fresh cooperative budget, independent of the
        // number of startup frames the fixture consumed. No elapsed-time oracle.
        tokio::task::yield_now().await;
        let wakes = Arc::new(Wakes(AtomicUsize::new(0)));
        let waker = std::task::Waker::from(wakes.clone());
        let mut cx = std::task::Context::from_waker(&waker);
        assert!(
            driver.as_mut().poll(&mut cx).is_pending(),
            "active session ended"
        );
        wakes.0.load(Ordering::SeqCst)
    }

    pub(crate) async fn next_text(
        mut driver: std::pin::Pin<&mut impl std::future::Future<Output = ()>>,
        events: &mut mpsc::Receiver<Result<crate::AgentEvent, HarnessError>>,
    ) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    _ = driver.as_mut() => panic!("session ended before the next text event"),
                    event = events.recv() => {
                        if let crate::AgentEvent::TextDelta { text } = event.unwrap().unwrap() {
                            return text;
                        }
                    }
                }
            }
        })
        .await
        .expect("fixture must deliver its next event")
    }

    #[tokio::test(start_paused = true)]
    async fn mailbox_timeout_and_exit_do_not_change_turn_controls() {
        let turn = crate::TurnControl::default();
        assert!(matches!(
            turn.inspect_context().await,
            Err(HarnessError::Unsupported(_))
        ));
        let mut receiver = turn.context_requests();
        let caller = turn.clone();
        let first = tokio::spawn(async move { caller.inspect_context().await });
        let request = receiver.recv().await.unwrap();
        tokio::time::advance(std::time::Duration::from_secs(6)).await;
        assert!(first.await.unwrap().is_err());
        assert!(request.reply.is_closed());
        let caller = turn.clone();
        let second = tokio::spawn(async move { caller.inspect_context().await });
        let request = receiver.recv().await.unwrap();
        drop(request);
        drop(receiver);
        assert!(second.await.unwrap().is_err());
        assert!(!turn.background_live());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), turn.stop_requested())
                .await
                .is_err()
        );
    }
}
