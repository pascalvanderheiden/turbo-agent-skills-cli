use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    discovery::{Diagnostic, Envelope},
    AdapterTarget, Installation, InstallationStatus, LocalState, ProfileScope, SkillIdentity,
    TascProfile, LOCAL_STATE_VERSION,
};

use crate::service::{skill_content_digest, StateRepository};

pub const COMMAND_SKILL_ID: &str = "skill:tasc://first-party/command-skill/tasc-command";
pub const COMMAND_SKILL_SOURCE: &str = "tasc://first-party/command-skill";
pub const COMMAND_SKILL_VERSION: &str = env!("CARGO_PKG_VERSION");
const COMMAND_SKILL_PATH: &str = "tasc-command";
const POLICY_SEMANTICS_VERSION: u32 = 1;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PACKAGE_FILES: &[(&str, &[u8])] = &[
    (
        "SKILL.md",
        include_bytes!("../../../skills/tasc-command/SKILL.md"),
    ),
    (
        "adapters/copilot.json",
        include_bytes!("../../../skills/tasc-command/adapters/copilot.json"),
    ),
    (
        "manifest.json",
        include_bytes!("../../../skills/tasc-command/manifest.json"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandSkillPlan {
    pub schema_version: u32,
    pub id: String,
    pub operation: String,
    pub profile: TascProfile,
    pub scope: String,
    pub target: AdapterTarget,
    pub path: String,
    pub version: String,
    pub package_digest: String,
    pub required_approval: String,
    pub installed_digest: Option<String>,
    pub modified: bool,
    pub existing: bool,
    pub force: bool,
    baseline_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSkillResult {
    pub operation: String,
    pub status: InstallationStatus,
    pub dry_run: bool,
    pub modified: bool,
    pub installation: Option<Installation>,
    pub plan: CommandSkillPlan,
}

#[derive(Debug)]
pub struct CommandSkillApproval {
    plan_id: String,
}

impl CommandSkillApproval {
    pub fn approve(plan: &CommandSkillPlan) -> Result<Self, Diagnostic> {
        if !matches!(
            plan.operation.as_str(),
            "install" | "update" | "repair" | "uninstall"
        ) {
            return Err(usage(
                "invalid_command_skill_operation",
                "Unsupported Command Skill lifecycle operation.",
                "Generate a new Command Skill lifecycle plan.",
            ));
        }
        Ok(Self {
            plan_id: plan.id.clone(),
        })
    }

    fn consume(self, plan: &CommandSkillPlan) -> Result<(), Diagnostic> {
        if self.plan_id != plan.id {
            return Err(usage(
                "approval_plan_mismatch",
                "Approval does not match this exact Command Skill plan.",
                "Generate and approve the current plan.",
            ));
        }
        Ok(())
    }
}

pub struct CommandSkillService<R> {
    repository: R,
}

impl<R: StateRepository> CommandSkillService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn plan(
        &self,
        operation: &str,
        scope: &str,
        target: AdapterTarget,
        force: bool,
    ) -> Result<CommandSkillPlan, Diagnostic> {
        validate_scope_target(scope, &target)?;
        if !matches!(operation, "install" | "update" | "repair" | "uninstall") {
            return Err(usage(
                "invalid_command_skill_operation",
                "Unsupported Command Skill lifecycle operation.",
                "Choose install, update, repair, or uninstall.",
            ));
        }
        let state = self.state()?;
        let path = PathBuf::from(&target.path).join(COMMAND_SKILL_PATH);
        ensure_no_symlink(&path)?;
        let installation_id =
            tasc_core::InventoryItem::canonical_installation_id(COMMAND_SKILL_ID, &target.id);
        let record = state
            .installations
            .iter()
            .find(|installation| installation.id == installation_id);
        let existing = path.exists();
        let installed_digest = existing
            .then(|| skill_content_digest(&path).map_err(state_error))
            .transpose()?;
        let baseline_digest = record.and_then(|installation| installation.content_digest.clone());
        let modified = if existing {
            record.is_none()
                || installed_digest
                    .as_deref()
                    .zip(baseline_digest.as_deref())
                    .is_none_or(|(current, baseline)| current != baseline)
        } else {
            false
        };

        match operation {
            "install" if existing || record.is_some() => {
                return Err(usage(
                    "command_skill_exists",
                    "A Command Skill Installation already exists at the selected Adapter Target.",
                    "Use skill command update or repair to manage the tracked Installation.",
                ));
            }
            "update" if record.is_none() => {
                return Err(usage(
                    "command_skill_not_tracked",
                    "No tracked Command Skill Installation exists at the selected Adapter Target.",
                    "Install the Command Skill first.",
                ));
            }
            "update" if !existing => {
                return Err(usage(
                    "command_skill_missing",
                    "The tracked Command Skill is missing from its Adapter Target.",
                    "Use skill command repair to restore the package.",
                ));
            }
            "repair" if record.is_none() => {
                return Err(usage(
                    "command_skill_not_tracked",
                    "No tracked Command Skill Installation exists at the selected Adapter Target.",
                    "Install the Command Skill first.",
                ));
            }
            "uninstall" if record.is_none() => {
                return Err(usage(
                    "command_skill_not_tracked",
                    "No tracked Command Skill Installation exists at the selected Adapter Target.",
                    "Install the Command Skill before uninstalling it.",
                ));
            }
            _ => {}
        }

        let id = format!("command-skill-plan-{}", unique_suffix());
        Ok(CommandSkillPlan {
            schema_version: 1,
            id,
            operation: operation.to_string(),
            profile: self.repository.profile(),
            scope: scope.to_string(),
            target,
            path: path.to_string_lossy().into_owned(),
            version: COMMAND_SKILL_VERSION.to_string(),
            package_digest: package_digest(),
            required_approval: "mutation".to_string(),
            installed_digest,
            modified,
            existing,
            force,
            baseline_digest,
        })
    }

    pub fn apply(
        &self,
        plan: CommandSkillPlan,
        approval: CommandSkillApproval,
    ) -> Result<CommandSkillResult, Diagnostic> {
        approval.consume(&plan)?;
        validate_scope_target(&plan.scope, &plan.target)?;
        if plan.profile != self.repository.profile() {
            return Err(usage(
                "profile_changed",
                "The selected TASC Profile changed after the Command Skill plan was created.",
                "Generate and approve a new plan for the selected Profile.",
            ));
        }
        if plan.package_digest != package_digest() || plan.version != COMMAND_SKILL_VERSION {
            return Err(usage(
                "stale_command_skill_package",
                "The bundled Command Skill changed after the plan was created.",
                "Generate and approve a new plan.",
            ));
        }
        let path = PathBuf::from(&plan.path);
        ensure_no_symlink(&path)?;
        let current_digest = if path.exists() {
            Some(skill_content_digest(&path).map_err(state_error)?)
        } else {
            None
        };
        if current_digest != plan.installed_digest {
            return Err(usage(
                "stale_command_skill_plan",
                "Command Skill content changed after the plan was created.",
                "Generate and approve a new plan.",
            ));
        }
        let expected_modified = current_digest
            .as_deref()
            .zip(plan.baseline_digest.as_deref())
            .is_none_or(|(current, baseline)| current != baseline);
        if plan.existing && expected_modified != plan.modified {
            return Err(usage(
                "stale_command_skill_plan",
                "The Command Skill modification status changed after the plan was created.",
                "Generate and approve a new plan.",
            ));
        }
        if plan.modified && !plan.force {
            return Err(usage(
                "modified_command_skill",
                "The Command Skill contains user modifications.",
                "Inspect the plan and pass --force only if you approve replacing or removing those changes.",
            ));
        }
        if plan.operation == "update"
            && !plan.modified
            && self
                .state()?
                .installations
                .iter()
                .find(|installation| installation.id == installation_id(&plan.target))
                .and_then(|installation| installation.source_revision.as_deref())
                == Some(COMMAND_SKILL_VERSION)
            && current_digest.as_deref() == Some(plan.package_digest.as_str())
        {
            let installation = self
                .state()?
                .installations
                .into_iter()
                .find(|installation| installation.id == installation_id(&plan.target));
            return Ok(CommandSkillResult {
                operation: plan.operation.clone(),
                status: InstallationStatus::Installed,
                dry_run: false,
                modified: false,
                installation,
                plan,
            });
        }

        let mut state = self.state()?;
        let prior_state = state.clone();
        let id = installation_id(&plan.target);
        let backup = if path.exists() {
            Some(
                transaction_parent(&path).join(format!(".tasc-command-backup-{}", unique_suffix())),
            )
        } else {
            None
        };
        if let Some(backup) = &backup {
            fs::rename(&path, backup).map_err(|error| state_error(error.to_string()))?;
        }

        let mut installation = None;
        let operation_result = match plan.operation.as_str() {
            "uninstall" => {
                state.installations.retain(|item| item.id != id);
                Ok(())
            }
            _ => (|| {
                write_package(&path).map_err(state_error)?;
                let content_digest = skill_content_digest(&path).map_err(state_error)?;
                let record = Installation {
                    id: id.clone(),
                    skill_id: COMMAND_SKILL_ID.to_string(),
                    target_id: plan.target.id.clone(),
                    path: path.to_string_lossy().into_owned(),
                    source_revision: Some(COMMAND_SKILL_VERSION.to_string()),
                    content_digest: Some(content_digest),
                    status: InstallationStatus::Installed,
                    last_error: None,
                };
                if !state
                    .identities
                    .iter()
                    .any(|identity| identity.id == COMMAND_SKILL_ID)
                {
                    state.identities.push(command_skill_identity());
                }
                if !state
                    .targets
                    .iter()
                    .any(|target| target.id == plan.target.id)
                {
                    state.targets.push(plan.target.clone());
                }
                state.installations.retain(|item| item.id != id);
                state.installations.push(record.clone());
                installation = Some(record);
                Ok(())
            })(),
        };
        if let Err(error) = operation_result {
            rollback_path(&path, backup.as_deref());
            return Err(error);
        }
        if let Err(error) = self.repository.save(&state) {
            rollback_path(&path, backup.as_deref());
            let _ = self.repository.save(&prior_state);
            return Err(state_error(error));
        }
        if let Some(backup) = backup {
            let _ = fs::remove_dir_all(backup);
        }
        Ok(CommandSkillResult {
            operation: plan.operation.clone(),
            status: if plan.operation == "uninstall" {
                InstallationStatus::Missing
            } else {
                InstallationStatus::Installed
            },
            dry_run: false,
            modified: plan.modified,
            installation,
            plan,
        })
    }

    pub fn list(&self) -> Result<Vec<Installation>, Diagnostic> {
        let state = self.state()?;
        Ok(state
            .installations
            .into_iter()
            .filter(|installation| installation.skill_id == COMMAND_SKILL_ID)
            .collect())
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
}

pub fn capabilities(skill_version: Option<&str>, policy_version: Option<u32>) -> Value {
    let manifest: Value =
        serde_json::from_slice(include_bytes!("../../../skills/tasc-command/manifest.json"))
            .expect("bundled Command Skill manifest is valid JSON");
    let bundled_version = manifest["version"].as_str().unwrap_or_default();
    let requested_skill_version = skill_version.unwrap_or(bundled_version);
    let policy_compatible =
        policy_version.unwrap_or(POLICY_SEMANTICS_VERSION) == POLICY_SEMANTICS_VERSION;
    let version_compatible = requested_skill_version == COMMAND_SKILL_VERSION;
    let degraded = !version_compatible || !policy_compatible;
    let mut result = json!({
        "cliVersion": COMMAND_SKILL_VERSION,
        "skillVersion": requested_skill_version,
        "policySemanticsVersion": POLICY_SEMANTICS_VERSION,
        "compatibility": if degraded { "degraded" } else { "compatible" },
        "capabilities": manifest["capabilities"],
        "supportedAdapters": ["copilot"],
        "warnings": []
    });
    let warnings = result["warnings"].as_array_mut().expect("warnings array");
    if !version_compatible {
        warnings.push(json!(format!(
            "Command Skill version {requested_skill_version} differs from TASC CLI version {COMMAND_SKILL_VERSION}; only advertised capabilities are supported."
        )));
    }
    if !policy_compatible {
        warnings.push(json!(
            "Command Skill policy semantics are unknown; security-sensitive and mutating operations are disabled."
        ));
        result["capabilities"] = json!(["inventory.read", "profile.read"]);
    }
    serde_json::to_value(Envelope::new("skill command capabilities", result))
        .expect("capability response serializes")
}

pub fn command_skill_identity() -> SkillIdentity {
    SkillIdentity {
        id: COMMAND_SKILL_ID.to_string(),
        name: "TASC Command Skill".to_string(),
        source: COMMAND_SKILL_SOURCE.to_string(),
        relative_path: COMMAND_SKILL_PATH.to_string(),
    }
}

pub fn package_digest() -> String {
    let mut digest = Sha256::new();
    for (path, bytes) in PACKAGE_FILES {
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update(bytes);
        digest.update([0]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn profile_scope(scope: &str) -> Result<ProfileScope, Diagnostic> {
    match scope {
        "user" => Ok(ProfileScope::User),
        "project" => Ok(ProfileScope::Project),
        _ => Err(usage(
            "invalid_command_skill_scope",
            "Command Skill installation scope must be user or project.",
            "Pass --scope user or --scope project.",
        )),
    }
}

pub fn supported_target(scope: &str) -> Result<AdapterTarget, Diagnostic> {
    crate::CopilotAdapter::target_for_plan(scope)
        .map_err(|error| dependency("adapter_target_unavailable", &error))
}

pub fn detected_adapters(scope: &str) -> Value {
    let target_path = match scope {
        "project" => std::env::current_dir()
            .ok()
            .map(|path| path.join(".github").join("skills")),
        "user" => std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .map(|path| path.join(".copilot").join("skills")),
        _ => None,
    };
    let detected = target_path.as_ref().is_some_and(|path| path.exists());
    json!({
        "scope": scope,
        "adapters": [{
            "id": "copilot",
            "supported": true,
            "detected": detected,
            "target": target_path.map(|path| path.to_string_lossy().into_owned())
        }]
    })
}

fn validate_scope_target(scope: &str, target: &AdapterTarget) -> Result<(), Diagnostic> {
    profile_scope(scope)?;
    let expected = crate::CopilotAdapter::target_for_plan(scope)
        .map_err(|error| dependency("adapter_target_unavailable", &error))?;
    if target.assistant != expected.assistant
        || target.id != expected.id
        || target.path != expected.path
    {
        return Err(usage(
            "unsupported_command_skill_target",
            "The selected Adapter Target does not match the explicit scope.",
            "Select the Copilot Adapter Target using --scope user or --scope project.",
        ));
    }
    Ok(())
}

fn installation_id(target: &AdapterTarget) -> String {
    tasc_core::InventoryItem::canonical_installation_id(COMMAND_SKILL_ID, &target.id)
}

fn ensure_no_symlink(path: &Path) -> Result<(), Diagnostic> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(usage(
            "unsafe_command_skill_path",
            "The Command Skill destination is not a regular directory.",
            "Inspect the Adapter Target and remove the conflicting path manually.",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(state_error(error.to_string())),
    }
}

fn write_package(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Command Skill destination has no parent".to_string())?;
    let staging_parent = transaction_parent(path);
    fs::create_dir_all(staging_parent).map_err(|error| error.to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = staging_parent.join(format!(".tasc-command-install-{}", unique_suffix()));
    fs::create_dir(&temporary).map_err(|error| error.to_string())?;
    for (relative, contents) in PACKAGE_FILES {
        let file_path = temporary.join(relative);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::write(file_path, contents) {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error.to_string());
        }
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_dir_all(temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn transaction_parent(path: &Path) -> &Path {
    path.parent()
        .and_then(|parent| parent.parent())
        .or_else(|| path.parent())
        .unwrap_or_else(|| Path::new("."))
}

fn rollback_path(path: &Path, backup: Option<&Path>) {
    if path.exists() {
        let _ = fs::remove_dir_all(path);
    }
    if let Some(backup) = backup {
        let _ = fs::rename(backup, path);
    }
}

fn unique_suffix() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{time}-{sequence}", std::process::id())
}

fn usage(code: &str, message: &str, remediation: &str) -> Diagnostic {
    Diagnostic::new(code, "usage", "tasc", message, remediation)
}

fn dependency(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        "dependency",
        "tasc",
        message,
        "Check the selected Adapter Target and install the required dependency.",
    )
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "command_skill_state_unavailable",
        "operational",
        "tasc",
        &message,
        "Inspect the selected Profile and filesystem permissions, then retry.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_digest_is_stable_and_capabilities_fail_closed_for_unknown_policy() {
        assert_eq!(package_digest(), package_digest());
        let capabilities = capabilities(Some(COMMAND_SKILL_VERSION), Some(2));
        assert_eq!(capabilities["result"]["compatibility"], "degraded");
        assert_eq!(
            capabilities["result"]["capabilities"],
            json!(["inventory.read", "profile.read"])
        );
        assert_eq!(
            capabilities["result"]["warnings"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn package_digest_matches_the_persisted_installation_digest() {
        let root =
            std::env::temp_dir().join(format!("tasc-command-skill-digest-{}", unique_suffix()));
        for (relative, contents) in PACKAGE_FILES {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        assert_eq!(package_digest(), skill_content_digest(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capability_mismatch_warns_and_advertises_compatible_contract() {
        let capabilities = capabilities(Some("0.0.1"), Some(POLICY_SEMANTICS_VERSION));
        assert_eq!(capabilities["result"]["compatibility"], "degraded");
        assert!(capabilities["result"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("lifecycle.approve")));
        assert_eq!(
            capabilities["result"]["warnings"].as_array().unwrap().len(),
            1
        );
    }
}
