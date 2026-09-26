//! Kubernetes-backed [`Repository`] and the controller that maintains the
//! `Valid` condition.
//!
//! The controller's reflector store doubles as the read cache for the UI
//! and metrics, so there is a single watch on `apikeys`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures::StreamExt;
use jiff::Timestamp;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{Condition, Time};
use kube::api::PostParams;
use kube::runtime::controller::{Action, Controller};
use kube::runtime::events::{Event, EventType, Recorder, Reporter};
use kube::runtime::reflector::{ObjectRef, Store};
use kube::runtime::watcher;
use kube::{Api, Client, Resource};
use tracing::{debug, info, warn};

use crate::crd::ApiKey;
use crate::repo::{AuditEvent, RepoError, Repository, StatusMutation};
use crate::validation::{PathAllowList, validate};

pub const VALID_CONDITION: &str = "Valid";
const MAX_STATUS_ATTEMPTS: usize = 3;

pub struct KubeRepository {
    api: Api<ApiKey>,
    namespace: String,
    store: Store<ApiKey>,
    recorder: Recorder,
    ready: Arc<AtomicBool>,
}

struct Ctx {
    api: Api<ApiKey>,
    allowed: PathAllowList,
}

impl KubeRepository {
    /// Starts the controller in the background and returns the repository
    /// backed by its cache.
    pub fn start(client: Client, namespace: &str, allowed: PathAllowList) -> Arc<Self> {
        let api: Api<ApiKey> = Api::namespaced(client.clone(), namespace);
        let controller = Controller::new(api.clone(), watcher::Config::default());
        let store = controller.store();
        let ready = Arc::new(AtomicBool::new(false));

        let ctx = Arc::new(Ctx {
            api: api.clone(),
            allowed,
        });
        tokio::spawn(
            controller
                .run(reconcile, error_policy, ctx)
                .for_each(|result| async move {
                    match result {
                        Ok((obj, _)) => debug!(name = %obj.name, "reconciled"),
                        Err(e) => warn!(error = %e, "reconcile failed"),
                    }
                }),
        );
        {
            let store = store.clone();
            let ready = ready.clone();
            tokio::spawn(async move {
                if store.wait_until_ready().await.is_ok() {
                    info!("ApiKey cache synced");
                    ready.store(true, Ordering::Relaxed);
                }
            });
        }

        let reporter = Reporter {
            controller: "clavium".into(),
            instance: std::env::var("POD_NAME").ok(),
        };
        Arc::new(KubeRepository {
            api,
            namespace: namespace.to_string(),
            store,
            recorder: Recorder::new(client, reporter),
            ready,
        })
    }
}

#[async_trait]
impl Repository for KubeRepository {
    fn list(&self) -> Vec<Arc<ApiKey>> {
        self.store.state()
    }

    fn get(&self, name: &str) -> Option<Arc<ApiKey>> {
        self.store
            .get(&ObjectRef::new(name).within(&self.namespace))
    }

    async fn update_status(
        &self,
        name: &str,
        mutate: StatusMutation<'_>,
    ) -> Result<ApiKey, RepoError> {
        let updated = replace_status_with(&self.api, name, &|obj| {
            mutate(obj.status.get_or_insert_with(Default::default));
            true
        })
        .await?;
        Ok(updated.expect("the mutation always writes"))
    }

    async fn record_event(&self, key: &ApiKey, event: AuditEvent) {
        let ev = Event {
            type_: if event.warning {
                EventType::Warning
            } else {
                EventType::Normal
            },
            reason: event.reason.to_string(),
            note: Some(event.note),
            action: event.reason.to_string(),
            secondary: None,
        };
        if let Err(e) = self.recorder.publish(&ev, &key.object_ref(&())).await {
            warn!(error = %e, key = key.name(), "failed to publish event");
        }
    }

    fn ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }
}

type ObjectMutation<'a> = &'a (dyn Fn(&mut ApiKey) -> bool + Send + Sync);

/// Reads `name` fresh, applies `mutate` and writes the status back. The read
/// carries the resourceVersion, so a concurrent write makes the replace fail
/// with 409 instead of being lost; the whole cycle is then retried. Returns
/// `None` without writing if `mutate` returns false.
async fn replace_status_with(
    api: &Api<ApiKey>,
    name: &str,
    mutate: ObjectMutation<'_>,
) -> Result<Option<ApiKey>, RepoError> {
    for _ in 0..MAX_STATUS_ATTEMPTS {
        let mut obj = match api.get_status(name).await {
            Ok(obj) => obj,
            Err(kube::Error::Api(s)) if s.is_not_found() => {
                return Err(RepoError::NotFound(name.to_string()));
            }
            Err(e) => return Err(e.into()),
        };
        if !mutate(&mut obj) {
            return Ok(None);
        }
        match api.replace_status(name, &PostParams::default(), &obj).await {
            Ok(updated) => return Ok(Some(updated)),
            Err(kube::Error::Api(s)) if s.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(RepoError::Conflict)
}

/// Brings the `Valid` condition of `obj` up to date, keeping every other
/// condition as it is. Returns false if there was nothing to change.
pub fn update_valid_condition(obj: &mut ApiKey, allowed: &PathAllowList, now: Timestamp) -> bool {
    let Some(condition) = desired_valid_condition(obj, allowed, now) else {
        return false;
    };
    let generation = obj.metadata.generation;
    let status = obj.status.get_or_insert_with(Default::default);
    match status
        .conditions
        .iter_mut()
        .find(|c| c.type_ == VALID_CONDITION)
    {
        Some(existing) => *existing = condition,
        None => status.conditions.push(condition),
    }
    status.observed_generation = generation;
    true
}

/// Desired `Valid` condition for `key`, or `None` if the current one is up to date.
pub fn desired_valid_condition(
    key: &ApiKey,
    allowed: &PathAllowList,
    now: Timestamp,
) -> Option<Condition> {
    let problems = validate(&key.spec, allowed);
    let (status, reason, message) = if problems.is_empty() {
        ("True", "Valid", "Spec is valid".to_string())
    } else {
        ("False", "InvalidSpec", problems.join("; "))
    };
    let generation = key.metadata.generation;
    let existing = key.condition(VALID_CONDITION);
    if let Some(c) = existing
        && c.status == status
        && c.message == message
        && c.observed_generation == generation
    {
        return None;
    }
    let last_transition_time = match existing {
        Some(c) if c.status == status => c.last_transition_time.clone(),
        _ => Time(now),
    };
    Some(Condition {
        type_: VALID_CONDITION.into(),
        status: status.into(),
        reason: reason.into(),
        message,
        observed_generation: generation,
        last_transition_time,
    })
}

async fn reconcile(key: Arc<ApiKey>, ctx: Arc<Ctx>) -> Result<Action, RepoError> {
    // Cheap check against the cached copy first; the write itself works on a
    // fresh read, so conditions written by others since are kept.
    if desired_valid_condition(&key, &ctx.allowed, Timestamp::now()).is_none() {
        return Ok(Action::await_change());
    }
    let allowed = &ctx.allowed;
    replace_status_with(&ctx.api, key.name(), &|obj| {
        update_valid_condition(obj, allowed, Timestamp::now())
    })
    .await?;
    Ok(Action::await_change())
}

fn error_policy(_key: Arc<ApiKey>, _error: &RepoError, _ctx: Arc<Ctx>) -> Action {
    Action::requeue(std::time::Duration::from_secs(30))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::{ApiKeySpec, ApiKeyStatus};

    #[test]
    fn valid_condition_is_only_rewritten_on_change() {
        let now: Timestamp = "2026-09-01T00:00:00Z".parse().unwrap();
        let allowed = PathAllowList::allow_all();
        let mut key = ApiKey::new("k", ApiKeySpec::default());
        key.metadata.generation = Some(1);

        let first = desired_valid_condition(&key, &allowed, now).unwrap();
        assert_eq!(first.status, "True");

        key.status = Some(ApiKeyStatus {
            conditions: vec![first.clone()],
            ..Default::default()
        });
        assert!(desired_valid_condition(&key, &allowed, now).is_none());

        key.spec.rotation.max_age = Some("nope".into());
        key.metadata.generation = Some(2);
        let later: Timestamp = "2026-09-02T00:00:00Z".parse().unwrap();
        let second = desired_valid_condition(&key, &allowed, later).unwrap();
        assert_eq!(second.status, "False");
        assert_eq!(second.reason, "InvalidSpec");
        assert_eq!(second.last_transition_time, Time(later));
    }

    #[test]
    fn valid_condition_update_keeps_other_conditions() {
        let now: Timestamp = "2026-09-01T00:00:00Z".parse().unwrap();
        let allowed = PathAllowList::allow_all();
        // The fresh object carries a condition written by someone else after
        // the cached copy was taken.
        let other = Condition {
            type_: "Ready".into(),
            status: "True".into(),
            reason: "External".into(),
            message: "set by another controller".into(),
            observed_generation: Some(1),
            last_transition_time: Time(now),
        };
        let mut fresh = ApiKey::new("k", ApiKeySpec::default());
        fresh.metadata.generation = Some(1);
        fresh.status = Some(ApiKeyStatus {
            conditions: vec![other.clone()],
            ..Default::default()
        });
        assert!(update_valid_condition(&mut fresh, &allowed, now));
        let status = fresh.status.clone().unwrap();
        assert_eq!(status.conditions.len(), 2);
        assert_eq!(status.conditions[0], other);
        assert_eq!(status.conditions[1].type_, VALID_CONDITION);
        assert_eq!(status.observed_generation, Some(1));
        // Up to date: nothing to write.
        assert!(!update_valid_condition(&mut fresh, &allowed, now));
        // A spec change replaces Valid in place, still keeping the other one.
        fresh.spec.rotation.max_age = Some("nope".into());
        fresh.metadata.generation = Some(2);
        assert!(update_valid_condition(&mut fresh, &allowed, now));
        let status = fresh.status.unwrap();
        assert_eq!(status.conditions.len(), 2);
        assert_eq!(status.conditions[0], other);
        assert_eq!(status.conditions[1].status, "False");
    }
}
