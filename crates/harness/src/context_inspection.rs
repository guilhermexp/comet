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
mod tests {
    use super::*;
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
