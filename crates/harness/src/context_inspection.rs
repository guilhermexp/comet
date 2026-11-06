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
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    HarnessError::Protocol("Context inspection is busy".into())
                }
                mpsc::error::TrySendError::Closed(_) => {
                    let mut attached = self.0.lock().unwrap_or_else(|p| p.into_inner());
                    if attached
                        .as_ref()
                        .is_some_and(|current| current.same_channel(&sender))
                    {
                        *attached = None;
                    }
                    HarnessError::Protocol("Context inspection runtime exited".into())
                }
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
    use futures::FutureExt;

    #[tokio::test]
    async fn full_mailbox_reports_busy_without_detaching_runtime() {
        let inspection = ContextInspection::default();
        let mut receiver = inspection.attach();
        let mut pending = (0..receiver.max_capacity())
            .map(|_| Box::pin(inspection.inspect()))
            .collect::<Vec<_>>();
        for read in &mut pending {
            assert!(futures::poll!(read.as_mut()).is_pending());
        }
        assert_eq!(receiver.len(), receiver.max_capacity());
        let result = inspection
            .inspect()
            .now_or_never()
            .expect("a full mailbox must fail immediately");
        assert!(
            matches!(&result, Err(HarnessError::Protocol(reason)) if reason == "Context inspection is busy"),
            "unexpected full-mailbox result: {result:?}"
        );
        assert!(inspection.0.lock().unwrap().is_some());
        // Previously accepted requests must still complete on this receiver.
        for read in pending {
            receiver
                .try_recv()
                .unwrap()
                .reply
                .send(Ok(ContextBreakdown::default()))
                .unwrap();
            assert!(read.await.is_ok());
        }
        let read = inspection.inspect();
        tokio::pin!(read);
        assert!(futures::poll!(read.as_mut()).is_pending());
        receiver
            .try_recv()
            .unwrap()
            .reply
            .send(Ok(ContextBreakdown::default()))
            .unwrap();
        assert!(read.await.is_ok(), "Full must preserve the attached sender");
    }

    #[tokio::test]
    async fn closed_mailbox_reports_exit_and_detaches_runtime() {
        let inspection = ContextInspection::default();
        drop(inspection.attach());
        let result = inspection
            .inspect()
            .now_or_never()
            .expect("a closed mailbox must fail immediately");
        assert!(
            matches!(&result, Err(HarnessError::Protocol(reason)) if reason == "Context inspection runtime exited"),
            "unexpected closed-mailbox result: {result:?}"
        );
        assert!(
            inspection.0.lock().unwrap().is_none(),
            "the exited runtime's sender must be cleared"
        );
        assert!(matches!(
            inspection.inspect().await,
            Err(HarnessError::Unsupported(_))
        ));
        let mut replacement = inspection.attach();
        let read = inspection.inspect();
        tokio::pin!(read);
        assert!(futures::poll!(read.as_mut()).is_pending());
        replacement
            .try_recv()
            .unwrap()
            .reply
            .send(Ok(ContextBreakdown::default()))
            .unwrap();
        assert!(read.await.is_ok(), "a new attach must remain usable");
    }

    /// Drive the actual session future with a counting waker. A closed recv
    /// that spins exhausts Tokio's cooperative budget and wakes itself again;
    /// a parked select registers the waker but does not schedule another poll.
    pub(crate) async fn idle_poll_wakes(
        mut driver: std::pin::Pin<&mut impl std::future::Future<Output = ()>>,
    ) -> (usize, bool) {
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
        let budget_remaining = tokio::task::coop::has_budget_remaining();
        // Tokio defers cooperative wakes until the scheduler regains control.
        tokio::task::yield_now().await;
        (wakes.0.load(Ordering::SeqCst), budget_remaining)
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
