use crate::{
    consistency::ConsistencyArtifactRepository,
    evaluation::RunArtifactRepository,
    service::{DiscoveredSkill, SkillAdapter, StateRepository},
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    consistency::{ConsistencyFinding, ConsistencyStatus, FindingSeverity, RevisionStatus},
    discovery::Diagnostic,
    evaluation::{EvaluationOutcome, RunArtifact},
    insights::{
        InsightEvidence, InsightEvidenceDetail, InsightEvidenceSource, InsightLifecycleAction,
        InsightLifecycleEvent, InsightPriority, InsightSnapshot, InsightStatus, InsightView,
        ProposalReversibility, ProposalRisk, ProposalType, RemediationProposal, Suppression,
        SuppressionScope,
    },
    InstallationStatus, InventoryItem, LocalState, LOCAL_STATE_VERSION,
};

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub trait InsightArtifactRepository {
    fn save(&self, snapshot: &InsightSnapshot) -> Result<(), String>;
    fn list(&self) -> Result<Vec<InsightSnapshot>, String>;
    fn get(&self, id: &str) -> Result<InsightSnapshot, String>;
}

pub trait InsightLifecycleRepository {
    fn append(&self, event: &InsightLifecycleEvent) -> Result<(), String>;
    fn list(&self) -> Result<Vec<InsightLifecycleEvent>, String>;
}

#[derive(Debug, Clone)]
pub struct FsInsightArtifactRepository {
    directory: PathBuf,
}

impl FsInsightArtifactRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl InsightArtifactRepository for FsInsightArtifactRepository {
    fn save(&self, snapshot: &InsightSnapshot) -> Result<(), String> {
        save_immutable(&self.directory, &snapshot.id, snapshot, "Insight Snapshot")
    }

    fn list(&self) -> Result<Vec<InsightSnapshot>, String> {
        let mut snapshots: Vec<InsightSnapshot> = list_json(&self.directory, "Insight Snapshot")?;
        snapshots.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(snapshots)
    }

    fn get(&self, id: &str) -> Result<InsightSnapshot, String> {
        get_json(&self.directory, id, "Insight Snapshot")
    }
}

#[derive(Debug, Clone)]
pub struct FsInsightLifecycleRepository {
    directory: PathBuf,
}

impl FsInsightLifecycleRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl InsightLifecycleRepository for FsInsightLifecycleRepository {
    fn append(&self, event: &InsightLifecycleEvent) -> Result<(), String> {
        save_immutable(&self.directory, &event.id, event, "Insight lifecycle event")
    }

    fn list(&self) -> Result<Vec<InsightLifecycleEvent>, String> {
        let mut events: Vec<InsightLifecycleEvent> =
            list_json(&self.directory, "Insight lifecycle event")?;
        events.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(events)
    }
}

pub struct InsightService<D, R, C, E, A, L> {
    adapter: D,
    state_repository: R,
    consistency_repository: C,
    evaluation_repository: E,
    artifact_repository: A,
    lifecycle_repository: L,
}

impl<
        D: SkillAdapter,
        R: StateRepository,
        C: ConsistencyArtifactRepository,
        E: RunArtifactRepository,
        A: InsightArtifactRepository,
        L: InsightLifecycleRepository,
    > InsightService<D, R, C, E, A, L>
{
    pub fn new(
        adapter: D,
        state_repository: R,
        consistency_repository: C,
        evaluation_repository: E,
        artifact_repository: A,
        lifecycle_repository: L,
    ) -> Self {
        Self {
            adapter,
            state_repository,
            consistency_repository,
            evaluation_repository,
            artifact_repository,
            lifecycle_repository,
        }
    }

    pub fn generate(&self) -> Result<Vec<InsightView>, Diagnostic> {
        let state = self.state()?;
        let discovered = self.adapter.discover().map_err(state_error)?;
        let consistency = self.consistency_repository.list().map_err(state_error)?;
        let evaluations = self.evaluation_repository.list().map_err(state_error)?;
        let candidates = derive_insights(&state, &discovered, &consistency, &evaluations);
        let existing_snapshots = self.artifact_repository.list().map_err(state_error)?;
        let mut events = self.lifecycle_repository.list().map_err(state_error)?;
        let prior_latest = latest_snapshots(&existing_snapshots, &events);
        let existing = existing_snapshots
            .iter()
            .cloned()
            .into_iter()
            .map(|snapshot| (snapshot.id.clone(), snapshot))
            .collect::<HashMap<_, _>>();
        let mut current = BTreeMap::<String, InsightSnapshot>::new();
        for candidate in candidates {
            let snapshot = if let Some(snapshot) = existing.get(&candidate.id) {
                snapshot.clone()
            } else {
                self.artifact_repository
                    .save(&candidate)
                    .map_err(state_error)?;
                candidate
            };
            current.insert(snapshot.insight_id.clone(), snapshot);
        }
        for snapshot in current.values() {
            if !latest_refresh_is(&events, &snapshot.insight_id, &snapshot.id, true) {
                let event = self.refresh_event(snapshot, true);
                self.lifecycle_repository
                    .append(&event)
                    .map_err(state_error)?;
                events.push(event);
            }
        }
        for (insight_id, (snapshot, _)) in prior_latest {
            if !current.contains_key(&insight_id)
                && !latest_refresh_is(&events, &insight_id, &snapshot.id, false)
            {
                let event = self.refresh_event(&snapshot, false);
                self.lifecycle_repository
                    .append(&event)
                    .map_err(state_error)?;
                events.push(event);
            }
        }
        let snapshots = self.artifact_repository.list().map_err(state_error)?;
        self.views(snapshots)
    }

    pub fn list(&self) -> Result<Vec<InsightView>, Diagnostic> {
        let snapshots = self.artifact_repository.list().map_err(state_error)?;
        self.views(snapshots)
    }

    pub fn show(&self, id: &str) -> Result<InsightView, Diagnostic> {
        let views = self.list()?;
        if let Some(view) = views.iter().find(|view| view.snapshot.id == id) {
            return Ok(view.clone());
        }
        views
            .into_iter()
            .find(|view| view.snapshot.insight_id == id && view.is_latest_snapshot)
            .ok_or_else(|| selection_error(&format!("No Insight Snapshot matches '{id}'.")))
    }

    pub fn set_status(
        &self,
        id: &str,
        status: InsightStatus,
        reason: Option<&str>,
        until_unix_ms: Option<u128>,
    ) -> Result<InsightLifecycleEvent, Diagnostic> {
        if matches!(
            status,
            InsightStatus::Stale | InsightStatus::Inconclusive | InsightStatus::Suppressed
        ) {
            return Err(selection_error(
                "This Insight status is derived or managed by the suppression commands.",
            ));
        }
        if status == InsightStatus::Snoozed && until_unix_ms.is_none() {
            return Err(selection_error(
                "Snoozing an Insight requires an expiration time.",
            ));
        }
        if until_unix_ms.is_some_and(|until| until <= now_unix_ms()) {
            return Err(selection_error(
                "Insight snooze expiration must be in the future.",
            ));
        }
        let view = self.show(id)?;
        if view.status == InsightStatus::Stale {
            return Err(selection_error(
                "Stale Insight Snapshots cannot receive lifecycle changes.",
            ));
        }
        if view.status == InsightStatus::Suppressed {
            return Err(selection_error(
                "Unsuppress this Insight before changing its lifecycle status.",
            ));
        }
        let event = self.event(
            &view.snapshot,
            InsightLifecycleAction::Status {
                status,
                previous_status: Some(view.status),
                reason: reason.map(str::to_string),
                until_unix_ms,
            },
        );
        self.lifecycle_repository
            .append(&event)
            .map_err(state_error)?;
        Ok(event)
    }

    pub fn suppress(
        &self,
        id: &str,
        evidence_id: Option<&str>,
        reason: &str,
        expires_at_unix_ms: u128,
    ) -> Result<InsightLifecycleEvent, Diagnostic> {
        if reason.trim().is_empty() {
            return Err(selection_error("A suppression reason is required."));
        }
        if expires_at_unix_ms <= now_unix_ms() {
            return Err(selection_error(
                "Suppression expiration must be in the future.",
            ));
        }
        let view = self.show(id)?;
        if view.status == InsightStatus::Stale {
            return Err(selection_error(
                "Stale Insight Snapshots cannot receive suppressions.",
            ));
        }
        if view.status == InsightStatus::Suppressed {
            return Err(selection_error(
                "Unsuppress this Insight before creating another suppression.",
            ));
        }
        if let Some(evidence_id) = evidence_id {
            if !view
                .snapshot
                .evidence
                .iter()
                .any(|evidence| evidence.evidence_id == evidence_id)
            {
                return Err(selection_error(
                    "The selected evidence does not contribute to this Insight.",
                ));
            }
        }
        let suppression = Suppression {
            id: format!(
                "suppression-{}",
                stable_hash(
                    format!(
                        "{}|{}|{}|{}|{}|{}",
                        view.snapshot.id,
                        evidence_id.unwrap_or(&view.snapshot.insight_id),
                        view.snapshot.evidence_version,
                        now_unix_ms(),
                        std::process::id(),
                        EVENT_SEQUENCE.load(Ordering::Relaxed)
                    )
                    .as_bytes(),
                )
            ),
            insight_id: view.snapshot.insight_id.clone(),
            snapshot_id: view.snapshot.id.clone(),
            scope: if evidence_id.is_some() {
                SuppressionScope::Evidence
            } else {
                SuppressionScope::Insight
            },
            scope_id: evidence_id.unwrap_or(&view.snapshot.insight_id).to_string(),
            previous_status: view.status,
            evidence_version: view.snapshot.evidence_version.clone(),
            reason: reason.to_string(),
            created_at_unix_ms: now_unix_ms(),
            expires_at_unix_ms,
        };
        let event = self.event(
            &view.snapshot,
            InsightLifecycleAction::Suppressed(suppression),
        );
        self.lifecycle_repository
            .append(&event)
            .map_err(state_error)?;
        Ok(event)
    }

    pub fn unsuppress(&self, id: &str, reason: &str) -> Result<InsightLifecycleEvent, Diagnostic> {
        if reason.trim().is_empty() {
            return Err(selection_error("An unsuppression reason is required."));
        }
        let view = self.show(id)?;
        let mut active_suppression = None;
        for event in self.lifecycle_repository.list().map_err(state_error)? {
            if event.insight_id != view.snapshot.insight_id
                || event.evidence_version != view.snapshot.evidence_version
            {
                continue;
            }
            match event.action {
                InsightLifecycleAction::Suppressed(suppression)
                    if suppression.expires_at_unix_ms > now_unix_ms() =>
                {
                    active_suppression = Some(suppression);
                }
                InsightLifecycleAction::Unsuppressed { suppression_id, .. }
                    if active_suppression
                        .as_ref()
                        .is_some_and(|active| active.id == suppression_id) =>
                {
                    active_suppression = None;
                }
                _ => {}
            }
        }
        let active_suppression = active_suppression
            .ok_or_else(|| selection_error("This Insight has no active suppression."))?;
        let event = self.event(
            &view.snapshot,
            InsightLifecycleAction::Unsuppressed {
                suppression_id: active_suppression.id,
                reason: reason.to_string(),
            },
        );
        self.lifecycle_repository
            .append(&event)
            .map_err(state_error)?;
        Ok(event)
    }

    pub fn proposals(&self, id: &str) -> Result<Vec<RemediationProposal>, Diagnostic> {
        let view = self.show(id)?;
        Ok(vec![proposal_for(&view.snapshot)])
    }

    fn state(&self) -> Result<LocalState, Diagnostic> {
        let state = self.state_repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            )));
        }
        Ok(state)
    }

    fn event(
        &self,
        snapshot: &InsightSnapshot,
        action: InsightLifecycleAction,
    ) -> InsightLifecycleEvent {
        let created_at_unix_ms = now_unix_ms();
        let sequence = EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        InsightLifecycleEvent {
            id: format!(
                "event-{created_at_unix_ms}-{}-{sequence:020}",
                std::process::id()
            ),
            insight_id: snapshot.insight_id.clone(),
            snapshot_id: snapshot.id.clone(),
            evidence_version: snapshot.evidence_version.clone(),
            created_at_unix_ms,
            action,
        }
    }

    fn refresh_event(&self, snapshot: &InsightSnapshot, active: bool) -> InsightLifecycleEvent {
        self.event(snapshot, InsightLifecycleAction::Refreshed { active })
    }

    fn views(&self, snapshots: Vec<InsightSnapshot>) -> Result<Vec<InsightView>, Diagnostic> {
        let events = self.lifecycle_repository.list().map_err(state_error)?;
        let latest = latest_snapshots(&snapshots, &events);
        let mut views = Vec::new();
        for snapshot in snapshots {
            let is_latest = latest
                .get(&snapshot.insight_id)
                .is_some_and(|(current, _)| current.id == snapshot.id);
            let latest_is_active = latest
                .get(&snapshot.insight_id)
                .is_some_and(|(_, active)| *active);
            let mut status = if !is_latest || !latest_is_active {
                InsightStatus::Stale
            } else {
                snapshot.evidence_status
            };
            if is_latest && latest_is_active {
                let evidence_status = snapshot.evidence_status;
                let mut active_suppression: Option<Suppression> = None;
                for event in events.iter().filter(|event| {
                    event.insight_id == snapshot.insight_id
                        && event.evidence_version == snapshot.evidence_version
                }) {
                    match &event.action {
                        InsightLifecycleAction::Status {
                            status: event_status,
                            previous_status,
                            until_unix_ms,
                            ..
                        } => {
                            status = if until_unix_ms.is_none_or(|until| until > now_unix_ms()) {
                                *event_status
                            } else {
                                previous_status.unwrap_or(evidence_status)
                            };
                        }
                        InsightLifecycleAction::Suppressed(suppression) => {
                            if suppression.evidence_version == snapshot.evidence_version
                                && suppression.expires_at_unix_ms > now_unix_ms()
                            {
                                active_suppression = Some(suppression.clone());
                                status = InsightStatus::Suppressed;
                            } else {
                                status = suppression.previous_status;
                                active_suppression = None;
                            }
                        }
                        InsightLifecycleAction::Unsuppressed { suppression_id, .. } => {
                            if active_suppression
                                .as_ref()
                                .is_some_and(|active| active.id == *suppression_id)
                            {
                                status = active_suppression
                                    .as_ref()
                                    .map_or(evidence_status, |active| active.previous_status);
                                active_suppression = None;
                            }
                        }
                        InsightLifecycleAction::Refreshed { .. } => {}
                    }
                }
                if active_suppression.is_some() {
                    status = InsightStatus::Suppressed;
                }
            }
            views.push(InsightView {
                is_latest_snapshot: is_latest,
                lifecycle_events: events
                    .iter()
                    .filter(|event| event.snapshot_id == snapshot.id)
                    .cloned()
                    .collect(),
                snapshot,
                status,
                priority_rank: 0,
            });
        }
        views.sort_by(compare_views);
        for (index, view) in views.iter_mut().enumerate() {
            view.priority_rank = index + 1;
        }
        Ok(views)
    }
}

fn latest_refresh_is(
    events: &[InsightLifecycleEvent],
    insight_id: &str,
    snapshot_id: &str,
    active: bool,
) -> bool {
    events
        .iter()
        .filter(|event| event.insight_id == insight_id)
        .filter_map(|event| match &event.action {
            InsightLifecycleAction::Refreshed {
                active: refresh_active,
            } => Some((event, *refresh_active)),
            _ => None,
        })
        .max_by(|(left, _), (right, _)| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        })
        .is_some_and(|(event, refresh_active)| {
            event.snapshot_id == snapshot_id && refresh_active == active
        })
}

fn latest_snapshots(
    snapshots: &[InsightSnapshot],
    events: &[InsightLifecycleEvent],
) -> BTreeMap<String, (InsightSnapshot, bool)> {
    let by_id = snapshots
        .iter()
        .map(|snapshot| (snapshot.id.as_str(), snapshot))
        .collect::<HashMap<_, _>>();
    let mut latest_refresh = BTreeMap::<String, (&InsightLifecycleEvent, bool)>::new();
    for event in events {
        let InsightLifecycleAction::Refreshed { active } = &event.action else {
            continue;
        };
        let replace = latest_refresh
            .get(&event.insight_id)
            .is_none_or(|(current, _)| {
                current.created_at_unix_ms < event.created_at_unix_ms
                    || (current.created_at_unix_ms == event.created_at_unix_ms
                        && current.id < event.id)
            });
        if replace {
            latest_refresh.insert(event.insight_id.clone(), (event, *active));
        }
    }
    let mut latest = BTreeMap::new();
    for (insight_id, (event, active)) in latest_refresh {
        if let Some(snapshot) = by_id.get(event.snapshot_id.as_str()) {
            latest.insert(insight_id, ((*snapshot).clone(), active));
        }
    }
    let refreshed_ids = latest.keys().cloned().collect::<HashSet<_>>();
    for snapshot in snapshots {
        if refreshed_ids.contains(&snapshot.insight_id) {
            continue;
        }
        let replace = latest.get(&snapshot.insight_id).is_none_or(|(current, _)| {
            current.created_at_unix_ms < snapshot.created_at_unix_ms
                || (current.created_at_unix_ms == snapshot.created_at_unix_ms
                    && current.id < snapshot.id)
        });
        if replace {
            latest.insert(snapshot.insight_id.clone(), (snapshot.clone(), true));
        }
    }
    latest
}

fn derive_insights(
    state: &LocalState,
    discovered: &[DiscoveredSkill],
    consistency: &[tasc_core::consistency::ConsistencyArtifact],
    evaluations: &[RunArtifact],
) -> Vec<InsightSnapshot> {
    let mut snapshots = Vec::new();
    for installation in &state.installations {
        let discovered_skill = discovered.iter().find(|skill| {
            skill.identity.id == installation.skill_id && skill.target.id == installation.target_id
        });
        let status = match discovered_skill {
            None => InstallationStatus::Missing,
            Some(_) if installation.status == InstallationStatus::Failed => {
                InstallationStatus::Failed
            }
            Some(_) if installation.content_digest.is_none() => InstallationStatus::Unverified,
            Some(skill)
                if installation
                    .content_digest
                    .as_ref()
                    .is_some_and(|digest| digest != &skill.content_digest) =>
            {
                InstallationStatus::Modified
            }
            Some(_) => installation.status,
        };
        if !matches!(
            status,
            InstallationStatus::Modified
                | InstallationStatus::Missing
                | InstallationStatus::Outdated
                | InstallationStatus::Failed
                | InstallationStatus::Unverified
        ) {
            continue;
        }
        let identity = state
            .identities
            .iter()
            .find(|item| item.id == installation.skill_id);
        let skill_name = identity.map_or(installation.skill_id.as_str(), |item| item.name.as_str());
        let summary = format!(
            "Installation '{}' for '{}' is {}.",
            installation.id,
            skill_name,
            format!("{status:?}").to_lowercase()
        );
        snapshots.push(snapshot(
            InsightEvidence {
                source: InsightEvidenceSource::Lifecycle,
                source_id: installation.id.clone(),
                evidence_id: installation.id.clone(),
                skill_ids: vec![installation.skill_id.clone()],
                affected_installations: 1,
                revision: discovered_skill
                    .map(|skill| format!("content:{}", skill.content_digest))
                    .or_else(|| installation.source_revision.clone())
                    .or_else(|| installation.content_digest.clone()),
                summary: summary.clone(),
                supporting_evidence: vec![InsightEvidenceDetail {
                    kind: "installation_status".to_string(),
                    skill_id: Some(installation.skill_id.clone()),
                    path: Some(installation.path.clone()),
                    line: None,
                    excerpt: format!(
                        "recorded={:?}; observed_content_digest={}",
                        status,
                        discovered_skill
                            .map_or("unavailable", |skill| skill.content_digest.as_str())
                    ),
                }],
                severity: installation_severity(status).to_string(),
                confidence: if status == InstallationStatus::Unverified {
                    "inferred".to_string()
                } else {
                    "observed".to_string()
                },
                confidence_score: if status == InstallationStatus::Unverified {
                    60
                } else {
                    100
                },
                urgency: match status {
                    InstallationStatus::Missing | InstallationStatus::Failed => 3,
                    InstallationStatus::Modified | InstallationStatus::Outdated => 2,
                    InstallationStatus::Unverified
                    | InstallationStatus::Planned
                    | InstallationStatus::Installed => 1,
                },
                observed_at_unix_ms: None,
            },
            skill_name.to_string(),
            summary,
            format!(
                "Inspect the installation state before choosing a lifecycle operation for '{}'.",
                skill_name
            ),
            if status == InstallationStatus::Unverified {
                InsightStatus::Inconclusive
            } else {
                InsightStatus::New
            },
        ));
        if let Some(snapshot) = snapshots.last_mut() {
            snapshot.scope = vec![
                installation.skill_id.clone(),
                installation.id.clone(),
                installation.target_id.clone(),
                installation.path.clone(),
            ];
        }
    }
    for skill in discovered {
        let installation_id =
            InventoryItem::canonical_installation_id(&skill.identity.id, &skill.target.id);
        if state
            .installations
            .iter()
            .any(|installation| installation.id == installation_id)
        {
            continue;
        }
        let summary = format!(
            "Discovered '{}' at '{}' is not recorded as an Installation.",
            skill.identity.name, skill.path
        );
        snapshots.push(snapshot(
            InsightEvidence {
                source: InsightEvidenceSource::Lifecycle,
                source_id: installation_id.clone(),
                evidence_id: installation_id.clone(),
                skill_ids: vec![skill.identity.id.clone()],
                affected_installations: 1,
                revision: Some(format!("content:{}", skill.content_digest)),
                summary: summary.clone(),
                supporting_evidence: vec![InsightEvidenceDetail {
                    kind: "untracked_installation".to_string(),
                    skill_id: Some(skill.identity.id.clone()),
                    path: Some(skill.path.clone()),
                    line: None,
                    excerpt: summary.clone(),
                }],
                severity: "info".to_string(),
                confidence: "observed".to_string(),
                confidence_score: 100,
                urgency: 1,
                observed_at_unix_ms: None,
            },
            format!("Untracked Installation: {}", skill.identity.name),
            summary,
            format!(
                "Review '{}' and adopt it only if you intend to track this existing Installation.",
                skill.identity.name
            ),
            InsightStatus::New,
        ));
        if let Some(snapshot) = snapshots.last_mut() {
            snapshot.scope = vec![
                skill.identity.id.clone(),
                installation_id.clone(),
                skill.target.id.clone(),
                skill.path.clone(),
            ];
        }
    }

    let mut latest_evaluations = BTreeMap::<String, &RunArtifact>::new();
    for artifact in evaluations {
        let key = artifact.identity.id.clone();
        let replace = latest_evaluations
            .get(&key)
            .is_none_or(|current| current.created_at_unix_ms <= artifact.created_at_unix_ms);
        if replace {
            latest_evaluations.insert(key, artifact);
        }
    }
    for artifact in latest_evaluations.into_values() {
        let current_digest = discovered
            .iter()
            .find(|skill| skill.identity.id == artifact.identity.id)
            .map(|skill| skill.content_digest.as_str());
        let stale = artifact
            .package_digest
            .as_deref()
            .is_some_and(|evaluated| current_digest != Some(evaluated));
        let (status, severity) = if stale {
            (InsightStatus::Stale, "warning")
        } else {
            match artifact.outcome {
                EvaluationOutcome::Failed => (InsightStatus::New, "blocking"),
                EvaluationOutcome::Inconclusive | EvaluationOutcome::NotRun => {
                    (InsightStatus::Inconclusive, "info")
                }
                EvaluationOutcome::Passed => continue,
            }
        };
        let summary = format!(
            "Evaluation '{}' for '{}' is {}{}.",
            artifact.id,
            artifact.identity.name,
            format!("{:?}", artifact.outcome).to_lowercase(),
            if stale {
                " for an older package revision"
            } else {
                ""
            }
        );
        snapshots.push(snapshot(
            InsightEvidence {
                source: InsightEvidenceSource::Evaluation,
                source_id: artifact.id.clone(),
                evidence_id: format!("evaluation:{}", artifact.identity.id),
                skill_ids: vec![artifact.identity.id.clone()],
                affected_installations: installation_breadth(
                    state,
                    std::slice::from_ref(&artifact.identity.id),
                ),
                revision: Some(format!(
                    "evaluated:{};current:{}",
                    artifact.package_digest.as_deref().unwrap_or("unknown"),
                    current_digest.unwrap_or("missing")
                )),
                summary: summary.clone(),
                supporting_evidence: artifact
                    .diagnostics
                    .iter()
                    .map(|diagnostic| InsightEvidenceDetail {
                        kind: diagnostic.code.clone(),
                        skill_id: Some(artifact.identity.id.clone()),
                        path: diagnostic.path.clone(),
                        line: None,
                        excerpt: diagnostic.message.clone(),
                    })
                    .collect(),
                severity: severity.to_string(),
                confidence: "observed".to_string(),
                confidence_score: 100,
                urgency: if status == InsightStatus::Stale {
                    0
                } else if artifact.outcome == EvaluationOutcome::Failed {
                    2
                } else {
                    1
                },
                observed_at_unix_ms: Some(artifact.completed_at_unix_ms),
            },
            artifact.identity.name.clone(),
            summary,
            format!(
                "Review Evaluation diagnostics, then rerun against the current package revision for '{}'.",
                artifact.identity.name
            ),
            status,
        ));
        if let Some(snapshot) = snapshots.last_mut() {
            snapshot.scope = vec![artifact.identity.id.clone()];
            if let Some(path) = &artifact.package_path {
                snapshot.scope.push(path.clone());
            }
        }
    }

    let mut latest_checks = BTreeMap::<String, &tasc_core::consistency::ConsistencyArtifact>::new();
    for artifact in consistency {
        let scope = consistency_scope_key(artifact);
        let replace = latest_checks
            .get(&scope)
            .is_none_or(|current| current.completed_at_unix_ms <= artifact.completed_at_unix_ms);
        if replace {
            latest_checks.insert(scope, artifact);
        }
    }
    let mut latest_findings = BTreeMap::<
        String,
        (
            &ConsistencyFinding,
            u128,
            String,
            &tasc_core::consistency::ConsistencyArtifact,
        ),
    >::new();
    let mut latest_inconclusive =
        BTreeMap::<String, (String, String, u128, String, Vec<String>)>::new();
    for artifact in latest_checks.values() {
        for finding in &artifact.findings {
            let replace = latest_findings
                .get(&finding.id)
                .is_none_or(|(_, timestamp, _, _)| *timestamp <= artifact.completed_at_unix_ms);
            if replace {
                latest_findings.insert(
                    finding.id.clone(),
                    (
                        finding,
                        artifact.completed_at_unix_ms,
                        artifact.id.clone(),
                        artifact,
                    ),
                );
            }
        }
        for category in &artifact.categories {
            if category.status != ConsistencyStatus::Inconclusive {
                continue;
            }
            let key = format!("{:?}", category.category);
            let replace = latest_inconclusive
                .get(&key)
                .is_none_or(|(_, _, timestamp, _, _)| *timestamp <= artifact.completed_at_unix_ms);
            if replace {
                latest_inconclusive.insert(
                    key,
                    (
                        format!("{:?}", category.category),
                        category.reason.clone(),
                        artifact.completed_at_unix_ms,
                        artifact.id.clone(),
                        artifact
                            .skills
                            .iter()
                            .map(|skill| skill.identity.id.clone())
                            .collect(),
                    ),
                );
            }
        }
    }
    for (finding, timestamp, artifact_id, artifact) in latest_findings.into_values() {
        if finding.status != ConsistencyStatus::Found {
            continue;
        }
        let artifact_revisions = finding
            .skill_ids
            .iter()
            .map(|skill_id| {
                let recorded = artifact
                    .skills
                    .iter()
                    .find(|skill| skill.identity.id == *skill_id)
                    .map(|skill| skill.content_digest.as_str());
                let current = discovered
                    .iter()
                    .find(|skill| skill.identity.id == *skill_id)
                    .map(|skill| skill.content_digest.as_str());
                (skill_id.as_str(), recorded, current)
            })
            .collect::<Vec<_>>();
        let locally_stale = artifact_revisions.iter().any(|(_, recorded, current)| {
            recorded.is_none() || current.is_none() || recorded != current
        });
        let status = if finding.revision_status == RevisionStatus::Stale || locally_stale {
            InsightStatus::Stale
        } else {
            InsightStatus::New
        };
        let summary = format!(
            "{} ({:?}, {:?} confidence).",
            finding.claim, finding.severity, finding.confidence
        );
        snapshots.push(snapshot(
            InsightEvidence {
                source: InsightEvidenceSource::Consistency,
                source_id: artifact_id,
                evidence_id: finding.id.clone(),
                skill_ids: finding.skill_ids.clone(),
                affected_installations: installation_breadth(state, &finding.skill_ids),
                revision: Some(format!(
                    "finding:{:?};{}",
                    finding.revision_status,
                    artifact_revisions
                        .iter()
                        .map(|(skill_id, recorded, current)| format!(
                            "{skill_id}@check:{}@current:{}",
                            recorded.unwrap_or("missing"),
                            current.unwrap_or("missing")
                        ))
                        .collect::<Vec<_>>()
                        .join("|")
                )),
                summary: summary.clone(),
                supporting_evidence: finding
                    .evidence
                    .iter()
                    .map(|item| InsightEvidenceDetail {
                        kind: item.kind.clone(),
                        skill_id: Some(item.skill_id.clone()),
                        path: Some(item.path.clone()),
                        line: item.line,
                        excerpt: item.excerpt.clone(),
                    })
                    .collect(),
                severity: format!("{:?}", finding.severity).to_lowercase(),
                confidence: format!("{:?}", finding.confidence).to_lowercase(),
                confidence_score: finding.confidence_score,
                urgency: if status == InsightStatus::Stale {
                    0
                } else {
                    match finding.severity {
                        FindingSeverity::Blocking => 2,
                        FindingSeverity::Warning => 1,
                        FindingSeverity::Info => 0,
                    }
                },
                observed_at_unix_ms: Some(timestamp),
            },
            format!("Consistency finding: {}", finding.claim),
            summary,
            finding.recommendation.clone(),
            status,
        ));
        if let Some(snapshot) = snapshots.last_mut() {
            snapshot.scope = finding.skill_ids.clone();
        }
    }
    for (category, reason, timestamp, artifact_id, skill_ids) in latest_inconclusive.into_values() {
        let summary = format!("{category} analysis is inconclusive: {reason}");
        snapshots.push(snapshot(
            InsightEvidence {
                source: InsightEvidenceSource::Consistency,
                source_id: artifact_id,
                evidence_id: format!("category:{category}"),
                affected_installations: installation_breadth(state, &skill_ids),
                skill_ids,
                revision: None,
                summary: summary.clone(),
                supporting_evidence: vec![InsightEvidenceDetail {
                    kind: "inconclusive_category".to_string(),
                    skill_id: None,
                    path: None,
                    line: None,
                    excerpt: reason.clone(),
                }],
                severity: "info".to_string(),
                confidence: "inferred".to_string(),
                confidence_score: 60,
                urgency: 0,
                observed_at_unix_ms: Some(timestamp),
            },
            format!("Consistency analysis incomplete: {category}"),
            summary,
            "Provide the declarations or evidence needed for this deterministic check, then rerun Consistency.".to_string(),
            InsightStatus::Inconclusive,
        ));
    }
    snapshots.sort_by(|left, right| left.insight_id.cmp(&right.insight_id));
    snapshots
}

fn consistency_scope_key(artifact: &tasc_core::consistency::ConsistencyArtifact) -> String {
    let mut skills = artifact
        .skills
        .iter()
        .map(|skill| skill.identity.id.clone())
        .collect::<Vec<_>>();
    skills.sort();
    if skills.is_empty() {
        skills = artifact.selectors.clone();
        skills.sort();
    }
    format!("{}|{}", artifact.all_installed, skills.join("|"))
}

fn snapshot(
    evidence: InsightEvidence,
    title: String,
    summary: String,
    recommendation: String,
    evidence_status: InsightStatus,
) -> InsightSnapshot {
    let evidence_json = serde_json::to_vec(&evidence).unwrap_or_default();
    let evidence_version = stable_hash(&evidence_json);
    let insight_id = format!(
        "insight-{}",
        stable_hash(format!("{:?}|{}", evidence.source, evidence.evidence_id).as_bytes(),)
    );
    let priority = priority_for(&evidence);
    let snapshot_version = serde_json::to_vec(&(
        &evidence_version,
        &title,
        &summary,
        &recommendation,
        evidence_status,
        &priority,
    ))
    .unwrap_or_default();
    let id = format!(
        "snapshot-{}",
        stable_hash(format!("{insight_id}|{}", stable_hash(&snapshot_version)).as_bytes())
    );
    InsightSnapshot {
        schema_version: 1,
        id,
        insight_id,
        created_at_unix_ms: now_unix_ms(),
        title,
        summary,
        recommendation,
        evidence_status,
        scope: evidence.skill_ids.clone(),
        priority,
        evidence: vec![evidence],
        evidence_version,
    }
}

fn priority_for(evidence: &InsightEvidence) -> InsightPriority {
    let blocking_impact = match evidence.severity.as_str() {
        "blocking" => 3,
        "warning" => 2,
        _ => 1,
    };
    let age_days = evidence.observed_at_unix_ms.map_or(0, |timestamp| {
        now_unix_ms().saturating_sub(timestamp) / 86_400_000
    });
    let evidence_freshness = 100_u128.saturating_sub(age_days).min(100) as u8;
    InsightPriority {
        blocking_impact,
        confidence: evidence.confidence_score,
        urgency: evidence.urgency,
        affected_installations: evidence.affected_installations,
        evidence_freshness,
    }
}

fn compare_views(left: &InsightView, right: &InsightView) -> std::cmp::Ordering {
    let left_priority = &left.snapshot.priority;
    let right_priority = &right.snapshot.priority;
    right_priority
        .blocking_impact
        .cmp(&left_priority.blocking_impact)
        .then_with(|| right_priority.confidence.cmp(&left_priority.confidence))
        .then_with(|| right_priority.urgency.cmp(&left_priority.urgency))
        .then_with(|| {
            right_priority
                .affected_installations
                .cmp(&left_priority.affected_installations)
        })
        .then_with(|| {
            right_priority
                .evidence_freshness
                .cmp(&left_priority.evidence_freshness)
        })
        .then_with(|| left.snapshot.insight_id.cmp(&right.snapshot.insight_id))
}

fn proposal_for(snapshot: &InsightSnapshot) -> RemediationProposal {
    let source = snapshot
        .evidence
        .first()
        .map(|evidence| evidence.source)
        .unwrap_or(InsightEvidenceSource::Lifecycle);
    let (proposal_type, risk, reversibility, prerequisite, verification) = match source {
        InsightEvidenceSource::Lifecycle => (
            ProposalType::ReviewInstallation,
            ProposalRisk::Medium,
            ProposalReversibility::ReviewRequired,
            "Confirm the target path and inspect local changes before any lifecycle operation.",
            "Reconcile the selected Installation with its observed target and content digest.",
        ),
        InsightEvidenceSource::Evaluation => (
            ProposalType::ReevaluateSkill,
            ProposalRisk::Low,
            ProposalReversibility::Reversible,
            "Confirm that the package revision and Evaluation inputs are still available.",
            "Run the same Evaluation against the selected current package and inspect its diagnostics.",
        ),
        InsightEvidenceSource::Consistency => (
            ProposalType::ReviewConsistencyFinding,
            ProposalRisk::Low,
            ProposalReversibility::NotApplicable,
            "Review each cited skill and source evidence before changing declarations or instructions.",
            "Rerun Consistency on the same selected skill set and compare the resulting findings.",
        ),
    };
    RemediationProposal {
        id: format!(
            "proposal-{}",
            stable_hash(format!("{}|{:?}", snapshot.id, source).as_bytes())
        ),
        insight_id: snapshot.insight_id.clone(),
        snapshot_id: snapshot.id.clone(),
        proposal_type,
        rationale: snapshot.recommendation.clone(),
        scope: snapshot.scope.clone(),
        prerequisites: vec![prerequisite.to_string()],
        affected_evidence: snapshot
            .evidence
            .iter()
            .map(|evidence| evidence.evidence_id.clone())
            .collect(),
        risk,
        reversibility,
        expected_verification: vec![verification.to_string()],
    }
}

fn installation_severity(status: InstallationStatus) -> &'static str {
    match status {
        InstallationStatus::Missing | InstallationStatus::Failed => "blocking",
        InstallationStatus::Modified | InstallationStatus::Outdated => "warning",
        InstallationStatus::Unverified => "info",
        InstallationStatus::Planned | InstallationStatus::Installed => "info",
    }
}

fn installation_breadth(state: &LocalState, skill_ids: &[String]) -> usize {
    state
        .installations
        .iter()
        .filter(|installation| skill_ids.contains(&installation.skill_id))
        .map(|installation| installation.id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn save_immutable<T: Serialize>(
    directory: &Path,
    id: &str,
    value: &T,
    label: &str,
) -> Result<(), String> {
    let path = artifact_path(directory, id)?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create {label} directory: {error}"))?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize {label}: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!("{label}s are immutable; this ID already exists")
            } else {
                format!("cannot create {label}: {error}")
            }
        })?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("cannot write {label}: {error}"));
    }
    Ok(())
}

fn list_json<T: DeserializeOwned + Serialize>(
    directory: &Path,
    label: &str,
) -> Result<Vec<T>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot list {label}s: {error}")),
    };
    let mut results = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read {label} entry: {error}"))?
            .path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {label}: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("{label}s must be regular, non-symlink files"));
        }
        let value = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("cannot read {label}: {error}"))?,
        )
        .map_err(|error| format!("invalid {label}: {error}"))?;
        let serialized =
            serde_json::to_value(&value).map_err(|error| format!("invalid {label}: {error}"))?;
        let stored_id = serialized
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{label} is missing its ID"))?;
        if path.file_stem().and_then(|name| name.to_str()) != Some(stored_id) {
            return Err(format!("{label} ID does not match its filename"));
        }
        results.push(value);
    }
    Ok(results)
}

fn get_json<T: DeserializeOwned + Serialize>(
    directory: &Path,
    id: &str,
    label: &str,
) -> Result<T, String> {
    let path = artifact_path(directory, id)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("no {label} has ID '{id}'")
        } else {
            format!("cannot inspect {label}: {error}")
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label}s must be regular, non-symlink files"));
    }
    let value: T = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("cannot read {label}: {error}"))?,
    )
    .map_err(|error| format!("invalid {label}: {error}"))?;
    let serialized =
        serde_json::to_value(&value).map_err(|error| format!("invalid {label}: {error}"))?;
    let stored_id = serialized
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{label} is missing its ID"))?;
    if stored_id != id {
        return Err(format!("{label} ID does not match its filename"));
    }
    Ok(value)
}

fn artifact_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("invalid Insight artifact ID".to_string());
    }
    Ok(directory.join(format!("{id}.json")))
}

fn stable_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn selection_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "insight_selection",
        "usage",
        "insight",
        message,
        "Use an Insight ID from `tasc insight list` and inspect its source evidence.",
    )
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "insight_failed",
        "operational",
        "filesystem",
        &message,
        "Inspect Local State and Insight artifacts, then retry.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasc_core::{
        consistency::{ConsistencySkillSnapshot, EvidenceConfidence, FindingSeverity},
        SkillIdentity,
    };

    #[test]
    fn deterministic_priority_orders_blocking_then_confidence_then_freshness() {
        let base = InsightEvidence {
            source: InsightEvidenceSource::Consistency,
            source_id: "check-1".to_string(),
            evidence_id: "finding".to_string(),
            skill_ids: vec!["skill".to_string()],
            affected_installations: 1,
            revision: Some("rev".to_string()),
            summary: "summary".to_string(),
            supporting_evidence: vec![],
            severity: "warning".to_string(),
            confidence: "declared".to_string(),
            confidence_score: 85,
            urgency: 1,
            observed_at_unix_ms: Some(now_unix_ms()),
        };
        let mut blocking = base.clone();
        blocking.severity = "blocking".to_string();
        let left = snapshot(
            base,
            "warning".to_string(),
            "warning".to_string(),
            "review".to_string(),
            InsightStatus::New,
        );
        let right = snapshot(
            blocking,
            "blocking".to_string(),
            "blocking".to_string(),
            "review".to_string(),
            InsightStatus::New,
        );
        assert_eq!(
            compare_views(
                &InsightView {
                    snapshot: right,
                    status: InsightStatus::New,
                    priority_rank: 0,
                    is_latest_snapshot: true,
                    lifecycle_events: vec![],
                },
                &InsightView {
                    snapshot: left,
                    status: InsightStatus::New,
                    priority_rank: 0,
                    is_latest_snapshot: true,
                    lifecycle_events: vec![],
                },
            ),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn derived_finding_keeps_source_evidence_and_stable_priority() {
        let finding = ConsistencyFinding {
            id: "finding:ownership_overlap:abc".to_string(),
            category: tasc_core::consistency::ConsistencyCategory::OwnershipOverlap,
            status: ConsistencyStatus::Found,
            revision_status: RevisionStatus::Current,
            severity: FindingSeverity::Blocking,
            confidence: EvidenceConfidence::Observed,
            confidence_score: 100,
            claim: "Two skills claim the same command.".to_string(),
            skill_ids: vec!["skill:a".to_string()],
            evidence: vec![tasc_core::consistency::ConsistencyEvidence {
                layer: tasc_core::consistency::EvidenceLayer::Deterministic,
                kind: "ownership_claim".to_string(),
                skill_id: "skill:a".to_string(),
                path: "alpha/SKILL.md".to_string(),
                line: Some(4),
                excerpt: "claims the deploy command".to_string(),
            }],
            recommendation: "Review ownership.".to_string(),
        };
        let identity = SkillIdentity {
            id: "skill:a".to_string(),
            name: "Alpha".to_string(),
            source: "local".to_string(),
            relative_path: "alpha".to_string(),
        };
        let artifact = tasc_core::consistency::ConsistencyArtifact {
            schema_version: 1,
            id: "check-1".to_string(),
            created_at_unix_ms: 100,
            completed_at_unix_ms: 100,
            selectors: vec![],
            all_installed: false,
            analyzer: "test".to_string(),
            analyzer_version: "1".to_string(),
            rule_version: "1".to_string(),
            skills: vec![ConsistencySkillSnapshot {
                identity: identity.clone(),
                source_revision: None,
                content_digest: "same-digest".to_string(),
                path: "alpha".to_string(),
            }],
            categories: vec![],
            findings: vec![finding],
            inferred_evidence_status: ConsistencyStatus::NotRun,
        };
        let state = LocalState {
            identities: vec![identity.clone()],
            installations: vec![tasc_core::Installation {
                id: "installation:skill:a@target:a".to_string(),
                skill_id: identity.id.clone(),
                target_id: "target:a".to_string(),
                path: "/skills/a".to_string(),
                source_revision: None,
                content_digest: Some("same-digest".to_string()),
                status: InstallationStatus::Installed,
                last_error: None,
            }],
            ..LocalState::default()
        };
        let discovered = vec![DiscoveredSkill {
            identity,
            target: tasc_core::AdapterTarget {
                id: "target:a".to_string(),
                assistant: "copilot".to_string(),
                path: "/skills/a".to_string(),
            },
            path: "/skills/a".to_string(),
            description: "An example skill.".to_string(),
            source_revision: None,
            content_digest: "same-digest".to_string(),
        }];
        let snapshots = derive_insights(&state, &discovered, &[artifact.clone()], &[]);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].evidence[0].evidence_id,
            "finding:ownership_overlap:abc"
        );
        assert_eq!(snapshots[0].priority.blocking_impact, 3);
        assert!(snapshots[0].recommendation.contains("Review ownership"));
        assert_eq!(snapshots[0].scope, vec!["skill:a"]);
        assert_eq!(snapshots[0].evidence_status, InsightStatus::New);
        assert_eq!(
            snapshots[0].evidence[0].supporting_evidence[0]
                .path
                .as_deref(),
            Some("alpha/SKILL.md")
        );
        assert_eq!(
            snapshots[0].evidence[0].supporting_evidence[0].line,
            Some(4)
        );
        let mut changed = discovered.clone();
        changed[0].content_digest = "new-digest".to_string();
        let stale = derive_insights(&state, &changed, &[artifact.clone()], &[]);
        let stale_finding = stale
            .iter()
            .find(|snapshot| snapshot.evidence[0].source == InsightEvidenceSource::Consistency)
            .unwrap();
        assert_eq!(stale_finding.evidence_status, InsightStatus::Stale);
        let follow_up = tasc_core::consistency::ConsistencyArtifact {
            id: "check-2".to_string(),
            created_at_unix_ms: 200,
            completed_at_unix_ms: 200,
            findings: vec![],
            ..artifact.clone()
        };
        assert!(derive_insights(&state, &discovered, &[artifact, follow_up], &[]).is_empty());
    }

    #[test]
    fn evaluation_snapshots_are_stale_when_the_local_package_revision_changes() {
        let identity = SkillIdentity {
            id: "skill:a".to_string(),
            name: "Alpha".to_string(),
            source: "local".to_string(),
            relative_path: "alpha".to_string(),
        };
        let artifact = RunArtifact {
            schema_version: 1,
            id: "run-1".to_string(),
            created_at_unix_ms: 100,
            completed_at_unix_ms: 100,
            selector: identity.id.clone(),
            identity: identity.clone(),
            source_revision: None,
            target: None,
            installation_id: None,
            package_path: None,
            package_digest: Some("old-digest".to_string()),
            analyzer: tasc_core::evaluation::EvaluationAnalyzer {
                name: "test".to_string(),
                version: "1".to_string(),
                configuration: vec![],
            },
            policy: tasc_core::evaluation::EvaluationPolicy {
                mode: "test".to_string(),
                package_validation: "smoke".to_string(),
                scenario_execution: "not_run".to_string(),
            },
            outcome: EvaluationOutcome::Failed,
            package_outcome: EvaluationOutcome::Failed,
            execution_outcome: EvaluationOutcome::NotRun,
            diagnostics: vec![],
        };
        let state = LocalState {
            identities: vec![identity.clone()],
            installations: vec![
                tasc_core::Installation {
                    id: "installation:a@one".to_string(),
                    skill_id: identity.id.clone(),
                    target_id: "one".to_string(),
                    path: "/skills/a-one".to_string(),
                    source_revision: None,
                    content_digest: None,
                    status: InstallationStatus::Installed,
                    last_error: None,
                },
                tasc_core::Installation {
                    id: "installation:a@two".to_string(),
                    skill_id: identity.id.clone(),
                    target_id: "two".to_string(),
                    path: "/skills/a-two".to_string(),
                    source_revision: None,
                    content_digest: None,
                    status: InstallationStatus::Installed,
                    last_error: None,
                },
            ],
            ..LocalState::default()
        };
        let discovered = vec![DiscoveredSkill {
            identity,
            target: tasc_core::AdapterTarget {
                id: "one".to_string(),
                assistant: "copilot".to_string(),
                path: "/skills/a-one".to_string(),
            },
            path: "/skills/a-one".to_string(),
            description: "An example skill.".to_string(),
            source_revision: None,
            content_digest: "new-digest".to_string(),
        }];

        let snapshots = derive_insights(&state, &discovered, &[], &[artifact]);
        let evaluation = snapshots
            .iter()
            .find(|snapshot| snapshot.evidence[0].source == InsightEvidenceSource::Evaluation)
            .unwrap();
        assert_eq!(evaluation.evidence_status, InsightStatus::Stale);
        assert_eq!(evaluation.evidence[0].evidence_id, "evaluation:skill:a");
        assert_eq!(evaluation.priority.affected_installations, 2);
    }
}
