use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tasc_core::{
    discovery::{Capabilities, Diagnostic, SearchHit, SkillSource},
    SourceRevision,
};
use tokio::{io::AsyncReadExt, process::Command, time};

const OUTPUT_LIMIT: u64 = 1024 * 1024;
const SEARCH_FIELDS: &str = "repo,skillName,namespace,description,stars,path";

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub async fn cancelled(&self) {
        while !self.0.load(Ordering::Relaxed) {
            time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait GhSkillGateway {
    async fn capabilities(&self, cancel: &Cancellation) -> Result<Capabilities, Diagnostic>;
    async fn search(
        &self,
        query: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<SearchHit>, Diagnostic>;
    async fn preview(
        &self,
        repository: &str,
        path: &str,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic>;
    async fn revision(
        &self,
        source: &SkillSource,
        path: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Option<SourceRevision>, Diagnostic> {
        let _ = (source, path, cancel);
        Err(dependency(
            "gh_unsupported",
            "GitHub CLI revision observation is unavailable.",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlan {
    pub outdated: bool,
    pub rendered_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCandidate {
    pub revision: SourceRevision,
    pub files: Vec<CandidateFile>,
    pub unavailable_reason: Option<String>,
}

#[allow(async_fn_in_trait)]
pub trait GhSkillLifecycleGateway {
    async fn candidate(
        &self,
        repository: &str,
        path: &str,
        revision: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<SourceCandidate, Diagnostic>;
    async fn install(
        &self,
        repository: &str,
        path: &str,
        target: &str,
        revision: Option<&str>,
        force: bool,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic>;
    async fn update_plan(
        &self,
        skill: &str,
        target: &str,
        cancel: &Cancellation,
    ) -> Result<UpdatePlan, Diagnostic>;
    async fn update(
        &self,
        skill: &str,
        target: &str,
        force: bool,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic>;
}

pub struct GhSkill {
    executable: PathBuf,
    timeout: Duration,
    help: Mutex<BTreeMap<&'static str, String>>,
}

impl Default for GhSkill {
    fn default() -> Self {
        Self::new("gh".into(), Duration::from_secs(30))
    }
}

impl GhSkill {
    pub fn new(executable: PathBuf, timeout: Duration) -> Self {
        Self {
            executable,
            timeout,
            help: Mutex::new(BTreeMap::new()),
        }
    }

    async fn execute(&self, args: &[&str], cancel: &Cancellation) -> Result<String, Diagnostic> {
        if cancel.0.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let mut child = Command::new(&self.executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GH_HOST", "github.com")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "")
            .env("PAGER", "")
            .env_remove("GH_FORCE_TTY")
            .env("NO_COLOR", "1")
            .env("CLICOLOR", "0")
            .env_remove("GH_DEBUG")
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    dependency("gh_missing", "GitHub CLI executable was not found.")
                } else {
                    operational("gh_spawn", "Cannot start GitHub CLI.")
                }
            })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let execution = async {
            // Keep both pipes draining concurrently, with a separate bound on each.
            let (out, err, status) = tokio::try_join!(
                Self::read_bounded(stdout),
                Self::read_bounded(stderr),
                async {
                    child
                        .wait()
                        .await
                        .map_err(|_| operational("gh_wait", "Cannot wait for GitHub CLI."))
                }
            )?;
            if !status.success() {
                let text = String::from_utf8_lossy(&err).to_lowercase();
                return Err(
                    if status.code() == Some(4)
                        || text.contains("gh auth login")
                        || text.contains("auth required")
                        || text.contains("login required")
                        || text.contains("http 401")
                        || text.contains("gh_token")
                        || text.contains("gh token")
                    {
                        Diagnostic::new(
                            "gh_authentication",
                            "dependency",
                            "gh",
                            "GitHub authentication is unavailable.",
                            "Run `gh auth login` or configure GH_TOKEN.",
                        )
                    } else if text.contains("http 403") {
                        Diagnostic::new("gh_access_denied", "dependency", "gh", "GitHub denied access to the requested operation.", "Check gh authentication scopes, repository access, and API rate limits.")
                    } else if text.contains("http 404") || text.contains("404 not found") {
                        Diagnostic::new(
                            "gh_not_found",
                            "operational",
                            "gh",
                            "The requested GitHub source or path was not found.",
                            "Verify the source and skill-relative path.",
                        )
                    } else if text.contains("unknown command")
                        || text.contains("unknown flag")
                        || text.contains("unknown json field")
                    {
                        dependency(
                            "gh_unsupported",
                            "The required gh skill capability is unsupported.",
                        )
                    } else {
                        operational("gh_failed", "GitHub CLI operation failed.")
                    },
                );
            }
            String::from_utf8(out)
                .map_err(|_| operational("gh_malformed_output", "GitHub CLI output is not UTF-8."))
        };
        let result = tokio::select! {
            result = time::timeout(self.timeout, execution) => result.unwrap_or_else(|_| Err(operational("gh_timeout", "GitHub CLI operation timed out."))),
            _ = cancel.cancelled() => Err(cancelled()),
        };
        if result.is_err() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        result
    }

    async fn read_bounded(
        stream: impl tokio::io::AsyncRead + Unpin,
    ) -> Result<Vec<u8>, Diagnostic> {
        let mut bytes = Vec::new();
        stream
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| operational("gh_io", "Cannot read GitHub CLI output."))?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            return Err(operational(
                "gh_output_limit",
                "GitHub CLI output exceeded 1 MiB.",
            ));
        }
        Ok(bytes)
    }

    async fn probe(
        &self,
        capability: &'static str,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic> {
        if cancel.0.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        if let Some(help) = self
            .help
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(capability)
        {
            return Ok(help.clone());
        }
        let args = match capability {
            "version" => vec!["version"],
            "skill" => vec!["skill", "--help"],
            "search" => vec!["skill", "search", "--help"],
            "preview" => vec!["skill", "preview", "--help"],
            "install" => vec!["skill", "install", "--help"],
            "update" => vec!["skill", "update", "--help"],
            _ => unreachable!(),
        };
        let help = self.execute(&args, cancel).await?;
        self.help
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(capability, help.clone());
        Ok(help)
    }

    async fn require(
        &self,
        capability: &'static str,
        cancel: &Cancellation,
    ) -> Result<(), Diagnostic> {
        let version = self.probe("version", cancel).await?;
        if !version.starts_with("gh version ") {
            return Err(dependency(
                "gh_unsupported",
                "Unrecognized GitHub CLI version response.",
            ));
        }
        self.probe("skill", cancel).await?;
        let help = self.probe(capability, cancel).await?;
        let supported = match capability {
            "search" => {
                help.contains("--json")
                    && help.contains("--limit")
                    && SEARCH_FIELDS.split(',').all(|field| help.contains(field))
            }
            "preview" => help.contains("preview <repository>"),
            "install" => {
                help.contains("install <repository>")
                    && help.contains("--dir")
                    && help.contains("--pin")
                    && help.contains("--force")
            }
            "update" => {
                help.contains("update")
                    && help.contains("--dir")
                    && help.contains("--dry-run")
                    && help.contains("--all")
            }
            _ => false,
        };
        if !supported {
            return Err(dependency(
                "gh_unsupported",
                "Required gh skill flags or output fields are unavailable.",
            ));
        }
        Ok(())
    }
}

impl GhSkillGateway for GhSkill {
    async fn capabilities(&self, cancel: &Cancellation) -> Result<Capabilities, Diagnostic> {
        let version = self.probe("version", cancel).await?;
        async fn available(result: Result<(), Diagnostic>) -> Result<bool, Diagnostic> {
            match result {
                Ok(()) => Ok(true),
                Err(error) if error.code == "gh_unsupported" => Ok(false),
                Err(error) => Err(error),
            }
        }
        Ok(Capabilities {
            version: version.trim().into(),
            search: available(self.require("search", cancel).await).await?,
            preview: available(self.require("preview", cancel).await).await?,
        })
    }

    async fn search(
        &self,
        query: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<SearchHit>, Diagnostic> {
        self.require("search", cancel).await?;
        let output = self
            .execute(
                &[
                    "skill",
                    "search",
                    "--json",
                    SEARCH_FIELDS,
                    "--limit",
                    "100",
                    "--",
                    query,
                ],
                cancel,
            )
            .await?;
        serde_json::from_str(&output).map_err(|_| {
            operational(
                "gh_malformed_output",
                "GitHub CLI returned invalid search JSON.",
            )
        })
    }

    async fn preview(
        &self,
        repository: &str,
        path: &str,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic> {
        self.require("preview", cancel).await?;
        self.execute(&["skill", "preview", "--", repository, path], cancel)
            .await
    }

    async fn revision(
        &self,
        source: &SkillSource,
        path: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Option<SourceRevision>, Diagnostic> {
        let repository = source.locator.trim_start_matches("https://github.com/");
        let endpoint = format!("repos/{repository}/commits");
        let mut args = vec![
            "api",
            endpoint.as_str(),
            "--method",
            "GET",
            "--field",
            "per_page=1",
        ];
        let path_field = path.map(|path| format!("path={path}"));
        if let Some(path_field) = &path_field {
            args.extend(["--field", path_field.as_str()]);
        }
        args.extend(["--jq", ".[0].sha // empty"]);
        let output = self.execute(&args, cancel).await?;
        let sha = output.trim();
        if sha.is_empty() {
            return Ok(None);
        }
        if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(operational(
                "gh_malformed_output",
                "GitHub CLI returned an invalid source revision.",
            ));
        }
        Ok(Some(SourceRevision::commit(sha.to_ascii_lowercase())))
    }
}

impl GhSkillLifecycleGateway for GhSkill {
    async fn candidate(
        &self,
        repository: &str,
        path: &str,
        revision: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<SourceCandidate, Diagnostic> {
        #[derive(Deserialize)]
        struct Tree {
            truncated: bool,
            tree: Vec<TreeEntry>,
        }

        #[derive(Deserialize)]
        struct TreeEntry {
            path: String,
            mode: String,
            #[serde(rename = "type")]
            kind: String,
            sha: String,
            size: Option<u64>,
        }

        let endpoint = format!("repos/{repository}/commits");
        let mut args = vec![
            "api",
            endpoint.as_str(),
            "--method",
            "GET",
            "--field",
            "per_page=1",
        ];
        let revision_field = revision.map(|revision| format!("sha={revision}"));
        if let Some(revision_field) = &revision_field {
            args.extend(["--field", revision_field.as_str()]);
        }
        args.extend(["--jq", ".[0].sha // empty"]);
        let revision_output = self.execute(&args, cancel).await?;
        let revision = revision_output.trim();
        if !matches!(revision.len(), 40 | 64)
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(operational(
                "security_revision_unavailable",
                "The exact candidate Source Revision could not be resolved.",
            ));
        }
        let revision = revision.to_ascii_lowercase();
        let tree_endpoint = format!("repos/{repository}/git/trees/{revision}?recursive=1");
        let tree_output = match self.execute(&["api", tree_endpoint.as_str()], cancel).await {
            Ok(output) => output,
            Err(error) => {
                return Ok(SourceCandidate {
                    revision: SourceRevision::commit(revision),
                    files: Vec::new(),
                    unavailable_reason: Some(format!("{}: {}", error.code, error.message)),
                })
            }
        };
        let tree: Tree = match serde_json::from_str(&tree_output) {
            Ok(tree) => tree,
            Err(_) => {
                return Ok(SourceCandidate {
                    revision: SourceRevision::commit(revision),
                    files: Vec::new(),
                    unavailable_reason: Some(
                        "GitHub CLI returned invalid candidate tree JSON.".to_string(),
                    ),
                })
            }
        };
        if tree.truncated {
            return Ok(SourceCandidate {
                revision: SourceRevision::commit(revision),
                files: Vec::new(),
                unavailable_reason: Some(
                    "The candidate source tree was truncated and cannot be checked completely."
                        .to_string(),
                ),
            });
        }
        let root = path.strip_suffix("/SKILL.md").ok_or_else(|| {
            operational(
                "security_check_failed",
                "The candidate skill path must end in SKILL.md.",
            )
        })?;
        let prefix = format!("{root}/");
        let mut entries = tree
            .tree
            .into_iter()
            .filter(|entry| entry.kind == "blob" && entry.path.starts_with(&prefix))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        if !entries.iter().any(|entry| entry.path == path) {
            return Ok(SourceCandidate {
                revision: SourceRevision::commit(revision),
                files: Vec::new(),
                unavailable_reason: Some(
                    "The exact candidate revision does not contain the selected SKILL.md."
                        .to_string(),
                ),
            });
        }
        if entries.len() > 5_000
            || entries
                .iter()
                .try_fold(0_u64, |total, entry| {
                    total.checked_add(entry.size.unwrap_or_default())
                })
                .is_none_or(|total| total > 50 * 1024 * 1024)
        {
            return Ok(SourceCandidate {
                revision: SourceRevision::commit(revision),
                files: Vec::new(),
                unavailable_reason: Some(
                    "The candidate exceeds the bounded Security Check limits.".to_string(),
                ),
            });
        }
        let mut files = Vec::with_capacity(entries.len());
        let mut unavailable_reason = None;
        for entry in entries {
            let blob_endpoint = format!("repos/{repository}/git/blobs/{}", entry.sha);
            let encoded = match self
                .execute(&["api", blob_endpoint.as_str(), "--jq", ".content"], cancel)
                .await
            {
                Ok(encoded) => encoded,
                Err(error) => {
                    unavailable_reason = Some(format!("{}: {}", error.code, error.message));
                    continue;
                }
            };
            let compact = encoded
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect::<Vec<_>>();
            let bytes = match BASE64.decode(compact) {
                Ok(bytes) => bytes,
                Err(_) => {
                    unavailable_reason =
                        Some("GitHub CLI returned invalid candidate blob content.".to_string());
                    continue;
                }
            };
            files.push(CandidateFile {
                path: entry
                    .path
                    .strip_prefix(&prefix)
                    .unwrap_or(&entry.path)
                    .to_string(),
                bytes,
                executable: entry.mode == "100755",
            });
        }
        Ok(SourceCandidate {
            revision: SourceRevision::commit(revision),
            files,
            unavailable_reason,
        })
    }

    async fn install(
        &self,
        repository: &str,
        path: &str,
        target: &str,
        revision: Option<&str>,
        force: bool,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic> {
        self.require("install", cancel).await?;
        let mut args = vec!["skill", "install", "--dir", target];
        if let Some(revision) = revision {
            args.extend(["--pin", revision]);
        }
        if force {
            args.push("--force");
        }
        args.extend(["--", repository, path]);
        self.execute(&args, cancel).await
    }

    async fn update_plan(
        &self,
        skill: &str,
        target: &str,
        cancel: &Cancellation,
    ) -> Result<UpdatePlan, Diagnostic> {
        self.require("update", cancel).await?;
        let output = self
            .execute(
                &["skill", "update", "--dir", target, "--dry-run", "--", skill],
                cancel,
            )
            .await?;
        Ok(UpdatePlan {
            outdated: output.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with('•') || line.starts_with("* ")
            }),
            rendered_text: output,
        })
    }

    async fn update(
        &self,
        skill: &str,
        target: &str,
        force: bool,
        cancel: &Cancellation,
    ) -> Result<String, Diagnostic> {
        self.require("update", cancel).await?;
        let mut args = vec!["skill", "update", "--dir", target, "--all"];
        if force {
            args.push("--force");
        }
        args.extend(["--", skill]);
        self.execute(&args, cancel).await
    }
}

fn cancelled() -> Diagnostic {
    Diagnostic::new(
        "cancelled",
        "cancelled",
        "tasc",
        "Operation cancelled.",
        "Retry when ready.",
    )
}

fn dependency(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        "dependency",
        "gh",
        message,
        "Install or upgrade GitHub CLI with gh skill support.",
    )
}

fn operational(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        "operational",
        "gh",
        message,
        "Check GitHub CLI connectivity and retry.",
    )
}
