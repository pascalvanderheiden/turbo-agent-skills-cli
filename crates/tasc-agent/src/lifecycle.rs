use crate::{
    gh::{Cancellation, GhSkillLifecycleGateway},
    security::inspect_source_candidate,
    service::{skill_content_digest, DiscoveredSkill, SkillAdapter, StateRepository},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    discovery::Diagnostic,
    security::{LifecycleSecurityEvidence, SecurityOverrideAudit},
    AdapterTarget, Installation, InstallationStatus, InventoryItem, LocalState, SkillIdentity,
    SourceRevision, TascProfile, LOCAL_STATE_VERSION,
};

const APPROVAL_TTL_MS: u128 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleApprovalRequirement {
    Mutation,
    SecurityFindings,
    SecurityOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecyclePlan {
    pub schema_version: u32,
    pub id: String,
    pub operation: String,
    pub profile: TascProfile,
    pub scope: String,
    pub identity: SkillIdentity,
    pub target: AdapterTarget,
    pub source_revision: SourceRevision,
    pub security: LifecycleSecurityEvidence,
    pub required_approval: LifecycleApprovalRequirement,
    pub expires_at_unix_ms: u128,
    pub force: bool,
    pub modified: bool,
    pub outdated: bool,
    pub existing: bool,
    pub baseline_content_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_override_reason: Option<String>,
}

impl LifecyclePlan {
    pub fn blocked(&self) -> bool {
        !self.security.required_checks_complete() && self.security_override_reason.is_none()
    }
}

#[derive(Debug)]
pub struct LifecycleApproval {
    plan_id: String,
    expires_at_unix_ms: u128,
}

impl LifecycleApproval {
    pub fn approve(plan: &LifecyclePlan) -> Result<Self, Diagnostic> {
        let now = now_unix_ms();
        if plan.expires_at_unix_ms <= now {
            return Err(usage(
                "approval_expired",
                "The lifecycle plan expired before approval.",
                "Generate and inspect a new dry-run plan.",
            ));
        }
        if plan.blocked() {
            return Err(usage(
                "security_override_required",
                "Required Security Policy checks are incomplete.",
                "Review the evidence and provide --security-override with a specific reason.",
            ));
        }
        Ok(Self {
            plan_id: plan.id.clone(),
            expires_at_unix_ms: plan.expires_at_unix_ms,
        })
    }

    fn consume(self, plan: &LifecyclePlan) -> Result<(), Diagnostic> {
        if self.plan_id != plan.id {
            return Err(usage(
                "approval_plan_mismatch",
                "Approval does not match this exact lifecycle plan.",
                "Generate and approve the current plan.",
            ));
        }
        if self.expires_at_unix_ms <= now_unix_ms() {
            return Err(usage(
                "approval_expired",
                "Lifecycle approval expired before mutation.",
                "Generate and approve a new plan.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleResult {
    pub operation: String,
    pub status: InstallationStatus,
    pub dry_run: bool,
    pub modified: bool,
    pub outdated: bool,
    pub existing: bool,
    pub installation: Option<Installation>,
    pub plan: String,
    pub lifecycle_plan: Option<LifecyclePlan>,
}

impl LifecycleResult {
    pub fn preview(plan: LifecyclePlan) -> Self {
        let status = if plan.modified {
            InstallationStatus::Modified
        } else if plan.outdated {
            InstallationStatus::Outdated
        } else if plan.operation == "update" && plan.existing {
            InstallationStatus::Installed
        } else {
            InstallationStatus::Planned
        };
        Self {
            operation: plan.operation.clone(),
            status,
            dry_run: true,
            modified: plan.modified,
            outdated: plan.outdated,
            existing: plan.existing,
            installation: None,
            plan: plan_summary(&plan),
            lifecycle_plan: Some(plan),
        }
    }
}

pub struct LifecycleService<A, G, R> {
    adapter: A,
    gateway: G,
    repository: R,
}

impl<A: SkillAdapter, G: GhSkillLifecycleGateway, R: StateRepository> LifecycleService<A, G, R> {
    pub fn new(adapter: A, gateway: G, repository: R) -> Self {
        Self {
            adapter,
            gateway,
            repository,
        }
    }

    pub async fn plan_install(
        &self,
        identity: SkillIdentity,
        target: AdapterTarget,
        revision: Option<&str>,
        force: bool,
        security_override_reason: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<LifecyclePlan, Diagnostic> {
        validate_revision(revision)?;
        validate_override_reason(security_override_reason)?;
        let state = self.state()?;
        let installation_id = InventoryItem::canonical_installation_id(&identity.id, &target.id);
        let skill_path = installation_path(&target, &identity.relative_path)?;
        let current = self.current_skill(&identity.id, &target.id)?;
        let expected_digest = content_digest_if_present(&skill_path).map_err(state_error)?;
        let existing_path = if expected_digest.is_some() {
            Some(skill_path.clone())
        } else {
            current.as_ref().map(|skill| PathBuf::from(&skill.path))
        };
        let current_digest = expected_digest.or(existing_path
            .as_deref()
            .map(content_digest_if_present)
            .transpose()
            .map_err(state_error)?
            .flatten());
        let modified = current_digest
            .as_deref()
            .zip(
                state
                    .installations
                    .iter()
                    .find(|record| record.id == installation_id)
                    .and_then(|record| record.content_digest.as_deref()),
            )
            .is_some_and(|(current, baseline)| current != baseline);
        let repository = repository_from_identity(&identity)?;
        let candidate = self
            .gateway
            .candidate(repository, &identity.relative_path, revision, cancel)
            .await?;
        let security = inspect_source_candidate(
            &identity,
            candidate.revision.clone(),
            candidate.files,
            candidate.unavailable_reason,
        );
        make_plan(
            "install",
            self.repository.profile(),
            identity,
            target,
            candidate.revision,
            security,
            force,
            modified,
            false,
            existing_path.is_some(),
            current_digest,
            security_override_reason,
        )
    }

    pub async fn apply_install(
        &self,
        plan: LifecyclePlan,
        approval: LifecycleApproval,
        cancel: &Cancellation,
    ) -> Result<LifecycleResult, Diagnostic> {
        validate_operation(&plan, "install")?;
        approval.consume(&plan)?;
        let mut state = self.state()?;
        validate_profile(&plan, &self.repository.profile())?;
        let skill_path = installation_path(&plan.target, &plan.identity.relative_path)?;
        let current_digest = content_digest_if_present(&skill_path).map_err(state_error)?;
        if current_digest != plan.baseline_content_digest {
            return Err(usage(
                "stale_lifecycle_plan",
                "Installation content changed after the plan was created.",
                "Generate and approve a new plan.",
            ));
        }
        if plan.existing && !plan.force {
            return Err(usage(
                if plan.modified {
                    "modified_installation"
                } else {
                    "installation_exists"
                },
                if plan.modified {
                    "Installation contains local modifications."
                } else {
                    "An installation already exists at the selected target."
                },
                "Inspect the plan and use --force before approving it.",
            ));
        }
        record_override_if_used(&mut state, &plan);
        if plan.security_override_reason.is_some() {
            self.repository.save(&state).map_err(state_error)?;
        }
        match self
            .gateway
            .install(
                repository_from_identity(&plan.identity)?,
                &plan.identity.relative_path,
                &plan.target.path,
                Some(&plan.source_revision.value),
                plan.force,
                cancel,
            )
            .await
        {
            Ok(output) => {
                let discovered = self
                    .current_skill(&plan.identity.id, &plan.target.id)
                    .and_then(|discovered| {
                        discovered.ok_or_else(|| {
                            operational(
                                "installation_not_found",
                                "GitHub CLI reported success but the Installation was not found.",
                            )
                        })
                    });
                match discovered {
                    Ok(discovered) => {
                        let pending = Installation {
                            id: InventoryItem::canonical_installation_id(
                                &plan.identity.id,
                                &plan.target.id,
                            ),
                            skill_id: plan.identity.id.clone(),
                            target_id: plan.target.id.clone(),
                            path: skill_path.to_string_lossy().into_owned(),
                            source_revision: Some(plan.source_revision.value.clone()),
                            content_digest: None,
                            status: InstallationStatus::Planned,
                            last_error: None,
                        };
                        let mut installation = installation_from_discovery(
                            &pending,
                            &discovered,
                            InstallationStatus::Installed,
                            None,
                        );
                        installation.source_revision = Some(plan.source_revision.value.clone());
                        ensure_identity_target(&mut state, &plan.identity, &plan.target);
                        replace_installation(&mut state, installation.clone());
                        state.watchlists.retain(|watchlist| {
                            watchlist.source.locator != plan.identity.source
                                || watchlist.path.as_deref().map_or(true, |path| {
                                    path.trim_end_matches("/SKILL.md")
                                        != plan.identity.relative_path.trim_end_matches("/SKILL.md")
                                })
                        });
                        if let Err(error) = self.repository.save(&state) {
                            return Err(state_error(error));
                        }
                        Ok(result_from_plan(
                            plan,
                            InstallationStatus::Installed,
                            false,
                            Some(installation),
                            output,
                        ))
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn plan_update(
        &self,
        selector: &str,
        force: bool,
        security_override_reason: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<LifecyclePlan, Diagnostic> {
        validate_override_reason(security_override_reason)?;
        let state = self.state()?;
        let installation = select_installation(&state, selector)?;
        let identity = identity_for(&state, &installation)?;
        let target = target_for(&state, &installation)?;
        let current = self.current_skill(&identity.id, &target.id)?;
        let Some(current) = current else {
            return Err(usage(
                "installation_missing",
                "Installation is missing from its Adapter Target.",
                "Reinstall the skill before updating it.",
            ));
        };
        let modified = installation
            .content_digest
            .as_deref()
            .map_or(true, |digest| digest != current.content_digest);
        let candidate = self
            .gateway
            .candidate(
                repository_from_identity(&identity)?,
                &identity.relative_path,
                None,
                cancel,
            )
            .await?;
        let outdated =
            installation.source_revision.as_deref() != Some(candidate.revision.value.as_str());
        let security = inspect_source_candidate(
            &identity,
            candidate.revision.clone(),
            candidate.files,
            candidate.unavailable_reason,
        );
        make_plan(
            "update",
            self.repository.profile(),
            identity,
            target,
            candidate.revision,
            security,
            force,
            modified,
            outdated,
            true,
            Some(current.content_digest),
            security_override_reason,
        )
    }

    pub async fn apply_update(
        &self,
        plan: LifecyclePlan,
        approval: LifecycleApproval,
        cancel: &Cancellation,
    ) -> Result<LifecycleResult, Diagnostic> {
        validate_operation(&plan, "update")?;
        approval.consume(&plan)?;
        let mut state = self.state()?;
        validate_profile(&plan, &self.repository.profile())?;
        let mut installation = select_installation(
            &state,
            &InventoryItem::canonical_installation_id(&plan.identity.id, &plan.target.id),
        )?;
        let current = self
            .current_skill(&plan.identity.id, &plan.target.id)?
            .ok_or_else(|| {
                usage(
                    "installation_missing",
                    "Installation is missing from its Adapter Target.",
                    "Reinstall the skill before updating it.",
                )
            })?;
        if Some(current.content_digest.clone()) != plan.baseline_content_digest {
            return Err(usage(
                "stale_lifecycle_plan",
                "Installation content changed after the plan was created.",
                "Generate and approve a new plan.",
            ));
        }
        if plan.modified && !plan.force {
            return Err(usage(
                "modified_installation",
                "Installation contains local modifications.",
                "Generate a force plan before approving the overwrite.",
            ));
        }
        if !plan.outdated && !plan.modified {
            return Ok(result_from_plan(
                plan,
                InstallationStatus::Installed,
                false,
                Some(installation),
                "Installation already matches the exact candidate revision.".to_string(),
            ));
        }
        record_override_if_used(&mut state, &plan);
        if plan.security_override_reason.is_some() {
            self.repository.save(&state).map_err(state_error)?;
        }
        match self
            .gateway
            .install(
                repository_from_identity(&plan.identity)?,
                &plan.identity.relative_path,
                &plan.target.path,
                Some(&plan.source_revision.value),
                true,
                cancel,
            )
            .await
        {
            Ok(output) => {
                let discovered = self
                    .current_skill(&plan.identity.id, &plan.target.id)
                    .and_then(|discovered| {
                        discovered.ok_or_else(|| {
                            operational(
                                "installation_not_found",
                                "GitHub CLI reported success but the Installation was not found.",
                            )
                        })
                    });
                match discovered {
                    Ok(discovered) => {
                        installation = installation_from_discovery(
                            &installation,
                            &discovered,
                            InstallationStatus::Installed,
                            None,
                        );
                        installation.source_revision = Some(plan.source_revision.value.clone());
                        replace_installation(&mut state, installation.clone());
                        self.repository.save(&state).map_err(state_error)?;
                        Ok(result_from_plan(
                            plan,
                            InstallationStatus::Installed,
                            false,
                            Some(installation),
                            output,
                        ))
                    }
                    Err(error) => {
                        self.record_failure(&mut state, installation, &error)?;
                        Err(error)
                    }
                }
            }
            Err(error) => {
                self.record_failure(&mut state, installation, &error)?;
                Err(error)
            }
        }
    }

    pub fn remove(
        &self,
        selector: &str,
        dry_run: bool,
        force: bool,
        approved: bool,
    ) -> Result<LifecycleResult, Diagnostic> {
        let mut state = self.state()?;
        let installation = select_installation(&state, selector)?;
        let identity = identity_for(&state, &installation)?;
        let target = target_for(&state, &installation)?;
        let current = self.current_skill(&identity.id, &target.id)?;
        let existing = current.is_some();
        let modified = current.as_ref().is_some_and(|skill| {
            installation
                .content_digest
                .as_deref()
                .map_or(true, |digest| digest != skill.content_digest)
        });
        if dry_run {
            return Ok(LifecycleResult {
                operation: "remove".to_string(),
                status: InstallationStatus::Planned,
                dry_run: true,
                modified,
                outdated: false,
                existing,
                installation: Some(installation.clone()),
                plan: format!("Remove {} from {}", identity.name, installation.path),
                lifecycle_plan: None,
            });
        }
        if modified && !force {
            return Err(usage(
                "modified_installation",
                "Installation contains local modifications.",
                "Pass --force --yes to remove modified content.",
            ));
        }
        require_approval(approved)?;
        let path = Path::new(&installation.path);
        if content_digest_if_present(path)
            .map_err(state_error)?
            .is_some()
        {
            let safe_path = safe_installation_path(&target, path)?;
            let backup = path.with_file_name(format!(
                ".{}.tasc-remove-{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                std::process::id()
            ));
            if backup.exists() {
                return Err(operational(
                    "removal_recovery_pending",
                    "A prior removal backup exists; recover it before retrying.",
                ));
            }
            fs::rename(&safe_path, &backup).map_err(|error| {
                operational(
                    "remove_failed",
                    &format!("Cannot stage Installation for removal: {error}"),
                )
            })?;
            state
                .installations
                .retain(|record| record.id != installation.id);
            if let Err(error) = self.repository.save(&state) {
                let _ = fs::rename(&backup, &safe_path);
                return Err(state_error(error));
            }
            if let Err(error) = fs::remove_dir_all(&backup) {
                let mut failed = installation.clone();
                failed.status = InstallationStatus::Failed;
                failed.last_error = Some(format!("cannot clean removal backup: {error}"));
                state.installations.push(failed);
                let _ = self.repository.save(&state);
                let _ = fs::rename(&backup, &safe_path);
                return Err(operational(
                    "remove_failed",
                    "Installation was staged but could not be fully removed; recovery was attempted.",
                ));
            }
        } else {
            state
                .installations
                .retain(|record| record.id != installation.id);
            self.repository.save(&state).map_err(state_error)?;
        }
        Ok(LifecycleResult {
            operation: "remove".to_string(),
            status: InstallationStatus::Missing,
            dry_run: false,
            modified,
            outdated: false,
            existing,
            installation: Some(installation),
            plan: "Installation removed.".to_string(),
            lifecycle_plan: None,
        })
    }

    fn state(&self) -> Result<LocalState, Diagnostic> {
        let state = self.repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            )));
        }
        Ok(state)
    }

    fn current_skill(
        &self,
        identity_id: &str,
        target_id: &str,
    ) -> Result<Option<DiscoveredSkill>, Diagnostic> {
        self.adapter.discover().map_err(state_error).map(|skills| {
            skills
                .into_iter()
                .find(|skill| skill.identity.id == identity_id && skill.target.id == target_id)
        })
    }

    fn record_failure(
        &self,
        state: &mut LocalState,
        mut installation: Installation,
        error: &Diagnostic,
    ) -> Result<(), Diagnostic> {
        installation.status = InstallationStatus::Failed;
        installation.last_error = Some(error.code.clone());
        replace_installation(state, installation);
        self.repository.save(state).map_err(state_error)
    }
}

#[allow(clippy::too_many_arguments)]
fn make_plan(
    operation: &str,
    profile: TascProfile,
    identity: SkillIdentity,
    target: AdapterTarget,
    source_revision: SourceRevision,
    security: LifecycleSecurityEvidence,
    force: bool,
    modified: bool,
    outdated: bool,
    existing: bool,
    baseline_content_digest: Option<String>,
    security_override_reason: Option<&str>,
) -> Result<LifecyclePlan, Diagnostic> {
    let required_approval = if !security.required_checks_complete() {
        LifecycleApprovalRequirement::SecurityOverride
    } else if security
        .findings
        .iter()
        .any(|finding| finding.deterministic)
    {
        LifecycleApprovalRequirement::SecurityFindings
    } else {
        LifecycleApprovalRequirement::Mutation
    };
    let expires_at_unix_ms = now_unix_ms().saturating_add(APPROVAL_TTL_MS);
    let scope = if Path::new(&target.path)
        .components()
        .any(|component| component.as_os_str() == ".github")
    {
        "project"
    } else {
        "user"
    }
    .to_string();
    let mut plan = LifecyclePlan {
        schema_version: 1,
        id: String::new(),
        operation: operation.to_string(),
        profile,
        scope,
        identity,
        target,
        source_revision,
        security,
        required_approval,
        expires_at_unix_ms,
        force,
        modified,
        outdated,
        existing,
        baseline_content_digest,
        security_override_reason: security_override_reason.map(str::trim).map(str::to_string),
    };
    let encoded = serde_json::to_vec(&plan).map_err(|error| {
        operational(
            "lifecycle_plan_failed",
            &format!("Cannot serialize lifecycle plan: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    plan.id = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(plan)
}

fn result_from_plan(
    plan: LifecyclePlan,
    status: InstallationStatus,
    dry_run: bool,
    installation: Option<Installation>,
    message: String,
) -> LifecycleResult {
    LifecycleResult {
        operation: plan.operation.clone(),
        status,
        dry_run,
        modified: plan.modified,
        outdated: plan.outdated,
        existing: installation.is_some() || plan.existing,
        installation,
        plan: message,
        lifecycle_plan: Some(plan),
    }
}

fn plan_summary(plan: &LifecyclePlan) -> String {
    format!(
        "{} {} from {} at {} into {} ({:?})",
        if plan.operation == "install" {
            "Install"
        } else {
            "Update"
        },
        plan.identity.name,
        plan.identity.source,
        plan.source_revision.value,
        plan.target.path,
        plan.required_approval
    )
}

fn validate_operation(plan: &LifecyclePlan, operation: &str) -> Result<(), Diagnostic> {
    if plan.operation == operation {
        Ok(())
    } else {
        Err(usage(
            "approval_plan_mismatch",
            "Lifecycle plan operation does not match the requested mutation.",
            "Generate and approve a new plan.",
        ))
    }
}

fn validate_profile(plan: &LifecyclePlan, profile: &TascProfile) -> Result<(), Diagnostic> {
    if plan.profile == *profile {
        Ok(())
    } else {
        Err(usage(
            "approval_plan_mismatch",
            "Lifecycle plan belongs to a different Profile.",
            "Generate and approve a plan in the selected Profile.",
        ))
    }
}

fn validate_override_reason(reason: Option<&str>) -> Result<(), Diagnostic> {
    if reason.is_some_and(|reason| reason.trim().is_empty() || reason.trim().len() > 500) {
        Err(usage(
            "invalid_security_override",
            "Security Override reason must contain 1 to 500 characters.",
            "Provide a specific, bounded reason for this one operation.",
        ))
    } else {
        Ok(())
    }
}

fn record_override_if_used(state: &mut LocalState, plan: &LifecyclePlan) {
    let Some(reason) = plan.security_override_reason.as_ref() else {
        return;
    };
    if plan.security.required_checks_complete() {
        return;
    }
    state.security_overrides.push(SecurityOverrideAudit {
        id: format!("security-override:{}", plan.id),
        operation: plan.operation.clone(),
        plan_id: plan.id.clone(),
        skill_id: plan.identity.id.clone(),
        target_id: plan.target.id.clone(),
        source_revision: plan.source_revision.clone(),
        reason: reason.clone(),
        approved_at_unix_ms: now_unix_ms(),
    });
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn content_digest_if_present(path: &Path) -> Result<Option<String>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot inspect '{}': {error}", path.display())),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "Installation path '{}' is not a safe directory",
            path.display()
        ));
    }
    skill_content_digest(path).map(Some)
}

fn installation_path(target: &AdapterTarget, relative_path: &str) -> Result<PathBuf, Diagnostic> {
    let name = install_name(relative_path)?;
    let mut path = PathBuf::from(&target.path);
    for component in name.split('/') {
        path.push(component);
    }
    Ok(path)
}

fn install_name(relative_path: &str) -> Result<String, Diagnostic> {
    if relative_path.starts_with('/')
        || relative_path.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.starts_with('-')
                || part.contains('\\')
        })
        || !relative_path.ends_with("/SKILL.md")
    {
        return Err(usage(
            "invalid_skill_path",
            "Installation source path is invalid.",
            "Use an exact repository-relative SKILL.md path from search.",
        ));
    }
    let parts: Vec<_> = relative_path
        .trim_end_matches("/SKILL.md")
        .split('/')
        .collect();
    let start = parts
        .iter()
        .rposition(|part| *part == "skills")
        .map(|index| index + 1);
    let name = match start {
        Some(index) if index < parts.len() => parts[index..].join("/"),
        _ => parts.last().unwrap_or(&"").to_string(),
    };
    if name.is_empty() {
        return Err(usage(
            "invalid_skill_path",
            "Installation source path is invalid.",
            "Use an exact repository-relative SKILL.md path from search.",
        ));
    }
    Ok(name)
}

fn repository_from_identity(identity: &SkillIdentity) -> Result<&str, Diagnostic> {
    let repository = identity
        .source
        .strip_prefix("https://github.com/")
        .ok_or_else(|| {
            usage(
                "invalid_source",
                "Installations require a GitHub source.",
                "Select a Skill Identity returned by GitHub search.",
            )
        })?;
    if repository.split('/').count() != 2
        || repository.contains(['?', '#', '@', ':'])
        || repository
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(usage(
            "invalid_source",
            "Installations require a GitHub owner/repository source.",
            "Select a Skill Identity returned by GitHub search.",
        ));
    }
    Ok(repository)
}

fn validate_revision(revision: Option<&str>) -> Result<(), Diagnostic> {
    if revision.is_some_and(|revision| {
        revision.is_empty()
            || revision.len() > 256
            || revision
                .chars()
                .any(|character| character.is_control() || character == '@')
    }) {
        return Err(usage(
            "invalid_revision",
            "Source Revision is invalid.",
            "Use a Git tag, branch, or commit SHA without control characters.",
        ));
    }
    Ok(())
}

fn require_approval(approved: bool) -> Result<(), Diagnostic> {
    if approved {
        Ok(())
    } else {
        Err(usage(
            "confirmation_required",
            "This mutation requires approval.",
            "Inspect with --dry-run, confirm interactively, or pass --yes.",
        ))
    }
}

fn select_installation(state: &LocalState, selector: &str) -> Result<Installation, Diagnostic> {
    let matches: Vec<_> = state
        .installations
        .iter()
        .filter(|installation| installation.id == selector || installation.skill_id == selector)
        .cloned()
        .collect();
    match matches.as_slice() {
        [installation] => Ok(installation.clone()),
        [] => Err(usage(
            "installation_not_found",
            "No tracked Installation matches the selector.",
            "Use `tasc skill list` to find an Installation ID.",
        )),
        _ => {
            let mut error = usage(
                "ambiguous_selection",
                "Selector matches multiple Installations.",
                "Use an exact Installation ID.",
            );
            error.candidates = matches.into_iter().map(|item| item.id).collect();
            error.candidates.sort();
            Err(error)
        }
    }
}

fn identity_for(
    state: &LocalState,
    installation: &Installation,
) -> Result<SkillIdentity, Diagnostic> {
    state
        .identities
        .iter()
        .find(|identity| identity.id == installation.skill_id)
        .cloned()
        .ok_or_else(|| state_error("Installation refers to a missing Skill Identity".to_string()))
}

fn target_for(
    state: &LocalState,
    installation: &Installation,
) -> Result<AdapterTarget, Diagnostic> {
    state
        .targets
        .iter()
        .find(|target| target.id == installation.target_id)
        .cloned()
        .ok_or_else(|| state_error("Installation refers to a missing Adapter Target".to_string()))
}

fn ensure_identity_target(
    state: &mut LocalState,
    identity: &SkillIdentity,
    target: &AdapterTarget,
) {
    if !state.identities.iter().any(|item| item.id == identity.id) {
        state.identities.push(identity.clone());
    }
    if !state.targets.iter().any(|item| item.id == target.id) {
        state.targets.push(target.clone());
    }
}

fn installation_from_discovery(
    previous: &Installation,
    discovered: &DiscoveredSkill,
    status: InstallationStatus,
    last_error: Option<String>,
) -> Installation {
    Installation {
        id: previous.id.clone(),
        skill_id: previous.skill_id.clone(),
        target_id: previous.target_id.clone(),
        path: discovered.path.clone(),
        source_revision: discovered.source_revision.clone(),
        content_digest: Some(discovered.content_digest.clone()),
        status,
        last_error,
    }
}

fn replace_installation(state: &mut LocalState, installation: Installation) {
    if let Some(existing) = state
        .installations
        .iter_mut()
        .find(|existing| existing.id == installation.id)
    {
        *existing = installation;
    } else {
        state.installations.push(installation);
    }
}

fn safe_installation_path(target: &AdapterTarget, path: &Path) -> Result<PathBuf, Diagnostic> {
    let root = fs::canonicalize(&target.path).map_err(|error| {
        operational(
            "remove_failed",
            &format!("Cannot resolve Adapter Target: {error}"),
        )
    })?;
    let candidate = fs::canonicalize(path).map_err(|error| {
        operational(
            "remove_failed",
            &format!("Cannot resolve Installation path: {error}"),
        )
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        operational(
            "remove_failed",
            &format!("Cannot inspect Installation path: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || !candidate.starts_with(&root)
        || candidate == root
        || Path::new(path)
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(operational(
            "unsafe_installation_path",
            "Installation path is outside its recorded Adapter Target.",
        ));
    }
    Ok(candidate)
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "local_state_failed",
        "operational",
        "filesystem",
        &message,
        "Inspect Local State and retry.",
    )
}

fn operational(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        "operational",
        "tasc",
        message,
        "Inspect the Installation and retry.",
    )
}

fn usage(code: &str, message: &str, remediation: &str) -> Diagnostic {
    Diagnostic::new(code, "usage", "tasc", message, remediation)
}
