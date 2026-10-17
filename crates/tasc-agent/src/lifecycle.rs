use crate::{
    gh::{Cancellation, GhSkillLifecycleGateway},
    service::{skill_content_digest, DiscoveredSkill, SkillAdapter, StateRepository},
};
use serde::Serialize;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tasc_core::{
    discovery::Diagnostic, AdapterTarget, Installation, InstallationStatus, InventoryItem,
    LocalState, SkillIdentity, LOCAL_STATE_VERSION,
};

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

    pub async fn install(
        &self,
        identity: SkillIdentity,
        target: AdapterTarget,
        revision: Option<&str>,
        dry_run: bool,
        force: bool,
        approved: bool,
        cancel: &Cancellation,
    ) -> Result<LifecycleResult, Diagnostic> {
        validate_revision(revision)?;
        let mut state = self.state()?;
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
        if dry_run {
            return Ok(LifecycleResult {
                operation: "install".to_string(),
                status: InstallationStatus::Planned,
                dry_run: true,
                modified,
                outdated: false,
                existing: existing_path.is_some(),
                installation: None,
                plan: format!(
                    "Install {} from {} into {}{}",
                    identity.name,
                    identity.source,
                    target.path,
                    revision
                        .map(|value| format!(" at {value}"))
                        .unwrap_or_default()
                ),
            });
        }
        if existing_path.is_some() && !force {
            return Err(usage(
                if modified {
                    "modified_installation"
                } else {
                    "installation_exists"
                },
                if modified {
                    "Installation contains local modifications."
                } else {
                    "An installation already exists at the selected target."
                },
                "Inspect the plan and pass --force --yes to overwrite it.",
            ));
        }
        require_approval(approved)?;
        ensure_identity_target(&mut state, &identity, &target);
        let pending = Installation {
            id: installation_id.clone(),
            skill_id: identity.id.clone(),
            target_id: target.id.clone(),
            path: skill_path.to_string_lossy().into_owned(),
            source_revision: revision.map(str::to_string),
            content_digest: None,
            status: InstallationStatus::Planned,
            last_error: None,
        };
        if let Some(index) = state
            .installations
            .iter()
            .position(|record| record.id == installation_id)
        {
            state.installations[index] = pending.clone();
        } else {
            state.installations.push(pending.clone());
        }
        match self
            .gateway
            .install(
                repository_from_identity(&identity)?,
                &identity.relative_path,
                &target.path,
                revision,
                force,
                cancel,
            )
            .await
        {
            Ok(output) => {
                let discovered =
                    self.current_skill(&identity.id, &target.id)
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
                        let installation = installation_from_discovery(
                            &pending,
                            &discovered,
                            InstallationStatus::Installed,
                            None,
                        );
                        replace_installation(&mut state, installation.clone());
                        if let Err(error) = self.repository.save(&state) {
                            return Err(state_error(error));
                        }
                        Ok(LifecycleResult {
                            operation: "install".to_string(),
                            status: InstallationStatus::Installed,
                            dry_run: false,
                            modified: false,
                            outdated: false,
                            existing: true,
                            installation: Some(installation),
                            plan: output,
                        })
                    }
                    Err(error) => {
                        self.record_failure(&mut state, pending, &error)?;
                        Err(error)
                    }
                }
            }
            Err(error) => {
                self.record_failure(&mut state, pending, &error)?;
                Err(error)
            }
        }
    }

    pub async fn update(
        &self,
        selector: &str,
        dry_run: bool,
        force: bool,
        approved: bool,
        cancel: &Cancellation,
    ) -> Result<LifecycleResult, Diagnostic> {
        let mut state = self.state()?;
        let mut installation = select_installation(&state, selector)?;
        let identity = identity_for(&state, &installation)?;
        let target = target_for(&state, &installation)?;
        let current = self.current_skill(&identity.id, &target.id)?;
        let Some(current) = current else {
            installation.status = InstallationStatus::Missing;
            return Ok(LifecycleResult {
                operation: "update".to_string(),
                status: InstallationStatus::Missing,
                dry_run,
                modified: false,
                outdated: false,
                existing: false,
                installation: Some(installation),
                plan: "Installation is missing from its Adapter Target.".to_string(),
            });
        };
        let modified = installation
            .content_digest
            .as_deref()
            .map_or(true, |digest| digest != current.content_digest);
        let plan = self
            .gateway
            .update_plan(
                &install_name(&identity.relative_path)?,
                &target.path,
                cancel,
            )
            .await?;
        let outdated = plan.outdated;
        if dry_run {
            return Ok(LifecycleResult {
                operation: "update".to_string(),
                status: if modified {
                    InstallationStatus::Modified
                } else if outdated {
                    InstallationStatus::Outdated
                } else {
                    InstallationStatus::Installed
                },
                dry_run: true,
                modified,
                outdated,
                existing: true,
                installation: Some(installation),
                plan: plan.rendered_text,
            });
        }
        if modified && !force {
            return Err(usage(
                "modified_installation",
                "Installation contains local modifications.",
                "Pass --force --yes to overwrite modified content.",
            ));
        }
        if !outdated && !modified {
            return Ok(LifecycleResult {
                operation: "update".to_string(),
                status: InstallationStatus::Installed,
                dry_run: false,
                modified: false,
                outdated: false,
                existing: true,
                installation: Some(installation),
                plan: plan.rendered_text,
            });
        }
        require_approval(approved)?;
        match self
            .gateway
            .update(
                &install_name(&identity.relative_path)?,
                &target.path,
                force,
                cancel,
            )
            .await
        {
            Ok(output) => {
                let discovered =
                    self.current_skill(&identity.id, &target.id)
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
                        replace_installation(&mut state, installation.clone());
                        self.repository.save(&state).map_err(state_error)?;
                        Ok(LifecycleResult {
                            operation: "update".to_string(),
                            status: InstallationStatus::Installed,
                            dry_run: false,
                            modified: false,
                            outdated: false,
                            existing: true,
                            installation: Some(installation),
                            plan: output,
                        })
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
