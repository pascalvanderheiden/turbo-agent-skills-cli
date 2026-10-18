use clap::{Args, Parser, Subcommand, ValueEnum};
use std::collections::BTreeMap;
use std::io::IsTerminal;
use tasc_agent::{
    authoring::{FsSkillDraftRepository, SkillAuthoringService, SkillDraftInput},
    consistency::{
        ConsistencyArtifactRepository, ConsistencyService, FsConsistencyArtifactRepository,
    },
    discovery::{apply_model_ranking, DiscoveryService},
    evaluation::{EvaluationService, FsRunArtifactRepository, RunArtifactRepository},
    gh::{Cancellation, GhSkill},
    insights::{FsInsightArtifactRepository, FsInsightLifecycleRepository, InsightService},
    lifecycle::{LifecycleResult, LifecycleService},
    security::{
        FsSecurityArtifactRepository, FsSecuritySuppressionRepository, SecurityArtifactRepository,
        SecurityService,
    },
    service::{FsStateRepository, SkillAdapter},
    AgentService, CopilotAdapter, FsWatchCheckRepository, WatchService,
};
use tasc_core::{
    consistency::ConsistencyStatus,
    discovery::{Diagnostic, Envelope, RerankingStatus},
    evaluation::EvaluationOutcome,
    insights::InsightStatus,
    security::{SecurityCheckStatus, SecuritySeverity},
    InstallationStatus, Inventory, InventoryQuery, LocalState, ProvenanceConfidence, SkillIdentity,
    StateStatus, TascProfile,
};

mod agent;

#[derive(Parser)]
#[command(name = "tasc", version, about = "TASC: manage local agent skills")]
struct Cli {
    #[arg(long, global = true, value_enum, default_value = "human")]
    format: OutputFormat,
    /// Reject operations requiring GitHub access before starting gh.
    #[arg(long, global = true)]
    offline: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a Copilot SDK conversational session.
    Agent,
    /// Record and inspect source metadata without installing skills.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Strictly search public GitHub skills by exact fields or keywords.
    Search(SearchArgs),
    /// Discover skills by intent across local and remote sources.
    Discover(DiscoverArgs),
    /// Show skills discovered by local assistant adapters.
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
    /// Show the status of versioned local state.
    State {
        #[command(subcommand)]
        command: StateCommands,
    },
    /// Audit the selected Profile without changing state or evidence.
    #[command(visible_alias = "doctor")]
    Audit(JsonOutput),
    /// Summarize comparable inventory and evidence for the selected Profile.
    #[command(visible_alias = "statistics")]
    Stats(JsonOutput),
    /// Manage watched GitHub sources and skill-relative paths.
    Watch {
        #[command(subcommand)]
        command: WatchCommands,
    },
    /// Run and inspect deterministic package smoke Evaluations.
    Eval {
        #[command(subcommand)]
        command: EvalCommands,
    },
    /// Analyze an explicitly selected set of skills for cross-skill consistency.
    Consistency {
        #[command(subcommand)]
        command: ConsistencyCommands,
    },
    /// Analyze selected local skill content for static security risks.
    Security {
        #[command(subcommand)]
        command: SecurityCommands,
    },
    /// Rank local lifecycle, Evaluation, and Consistency evidence.
    Insight {
        #[command(subcommand)]
        command: InsightCommands,
    },
}

#[derive(Subcommand)]
enum EvalCommands {
    /// Validate a selected Skill package without model-backed execution.
    Run {
        selector: String,
        #[arg(long)]
        revision: Option<String>,
        /// Require the optional Waza dependency; scenario execution remains explicit and separate.
        #[arg(long)]
        waza: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// List immutable Evaluation Run Artifacts.
    List(JsonOutput),
    /// Show an immutable Evaluation Run Artifact.
    Show {
        id: String,
        #[command(flatten)]
        output: JsonOutput,
    },
}

#[derive(Subcommand)]
enum ConsistencyCommands {
    /// Analyze two or more selected skills, or explicitly include every installed skill.
    Check {
        #[arg(value_name = "SKILL")]
        selectors: Vec<String>,
        /// Explicitly analyze every skill discovered in installed Copilot targets.
        #[arg(long, conflicts_with = "selectors")]
        all_installed: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// List immutable Consistency Check Artifacts.
    List(JsonOutput),
    /// Show an immutable Consistency Check Artifact.
    Show {
        id: String,
        #[command(flatten)]
        output: JsonOutput,
    },
}

#[derive(Subcommand)]
enum SecurityCommands {
    /// Analyze explicitly selected Skills, Installations, or Skill Workspace paths.
    Check {
        #[arg(value_name = "SKILL")]
        selectors: Vec<String>,
        /// Explicitly analyze every skill discovered in installed Copilot targets.
        #[arg(long, conflicts_with = "selectors")]
        all_installed: bool,
        /// Return a finding exit code for deterministic findings at this severity or higher.
        #[arg(long, value_enum)]
        fail_on: Option<SecuritySeverityArg>,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// List immutable Security Check Artifacts.
    List(JsonOutput),
    /// Show an immutable Security Check Artifact.
    Show {
        id: String,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Suppress exactly one finding fingerprint for a bounded period.
    Suppress {
        fingerprint: String,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value_t = 30)]
        expires_in_days: u64,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// End an active suppression for exactly one finding fingerprint.
    Unsuppress {
        fingerprint: String,
        #[arg(long)]
        reason: String,
        #[command(flatten)]
        output: JsonOutput,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum SecuritySeverityArg {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl From<SecuritySeverityArg> for SecuritySeverity {
    fn from(severity: SecuritySeverityArg) -> Self {
        match severity {
            SecuritySeverityArg::Info => Self::Info,
            SecuritySeverityArg::Low => Self::Low,
            SecuritySeverityArg::Medium => Self::Medium,
            SecuritySeverityArg::High => Self::High,
            SecuritySeverityArg::Critical => Self::Critical,
        }
    }
}

#[derive(Subcommand)]
enum InsightCommands {
    /// Create immutable snapshots from currently available evidence.
    Generate(JsonOutput),
    /// List ranked Insight snapshots and their lifecycle status.
    List {
        /// Include Insights hidden from the default actionable list, including suppressed ones.
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Show a snapshot or the latest snapshot for an Insight.
    Show {
        id: String,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Record an auditable Insight lifecycle status change.
    Status {
        id: String,
        #[arg(value_enum)]
        status: InsightStatusArg,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        snooze_days: Option<u64>,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Suppress an Insight or one contributing evidence item until expiration.
    Suppress {
        id: String,
        #[arg(long)]
        evidence: Option<String>,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value_t = 30)]
        expires_in_days: u64,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Record an auditable end to the active suppression.
    Unsuppress {
        id: String,
        #[arg(long)]
        reason: String,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Show a bounded proposal without applying it.
    Propose {
        id: String,
        #[command(flatten)]
        output: JsonOutput,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum InsightStatusArg {
    New,
    Acknowledged,
    Snoozed,
    Resolved,
}

#[derive(Subcommand)]
enum SkillCommands {
    /// Create a guided Skill Draft in an explicitly selected Skill Workspace.
    Create {
        #[arg(long, value_name = "PATH")]
        workspace: std::path::PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        purpose: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        triggers: Option<String>,
        #[arg(long)]
        non_goals: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Deterministically validate a Skill Draft in its selected Workspace.
    Validate {
        id: String,
        #[arg(long, value_name = "PATH")]
        workspace: std::path::PathBuf,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// List durable Skill Drafts, optionally scoped to a Workspace.
    Drafts {
        #[arg(long, value_name = "PATH")]
        workspace: Option<std::path::PathBuf>,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Propose or apply the bounded missing-description Workspace repair.
    Remediate {
        id: String,
        #[arg(long, value_name = "PATH")]
        workspace: std::path::PathBuf,
        #[arg(long)]
        description: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Strictly search public GitHub skills by exact fields or keywords.
    Search(SearchArgs),
    /// Preview upstream files without installing or executing them.
    Preview {
        selector: String,
        #[arg(long)]
        source: Option<String>,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Install a selected GitHub skill into a Copilot target.
    Install {
        selector: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        scope: Option<InstallScope>,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Update one tracked Installation.
    Update {
        selector: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Remove one tracked Installation.
    Remove {
        selector: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// List discovered skills and whether they are tracked in Local State.
    List(InventoryArgs),
    /// Explicitly record a discovered installation without changing its files.
    Adopt { id: String },
    /// Track selected discovered installations, or every untracked installation.
    Track {
        #[arg(value_name = "INSTALLATION")]
        selectors: Vec<String>,
        #[arg(long, conflicts_with = "selectors")]
        all: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum InstallScope {
    Project,
    User,
}

#[derive(Args)]
struct SearchArgs {
    query: String,
    #[command(flatten)]
    output: JsonOutput,
}

#[derive(Args)]
struct DiscoverArgs {
    query: String,
    /// Explicitly ask Copilot to rerank the deterministic results.
    #[arg(long)]
    rerank: bool,
    #[command(flatten)]
    output: JsonOutput,
}

#[derive(Subcommand)]
enum SourceCommands {
    List(JsonOutput),
    Show {
        selector: String,
        #[command(flatten)]
        output: JsonOutput,
    },
    Add {
        locator: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Probe GitHub CLI search and preview support without changing state.
    Capabilities(JsonOutput),
}

#[derive(Subcommand)]
enum StateCommands {
    /// Show Local State counts and untracked discoveries.
    Status(JsonOutput),
}

#[derive(Subcommand)]
enum WatchCommands {
    /// Add a GitHub source or source-relative path to the Watchlist.
    Add {
        locator: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// List Watchlist targets and their latest observed revisions.
    List(JsonOutput),
    /// Remove a Watchlist ID or an unambiguous source selector.
    Remove {
        selector: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        output: JsonOutput,
    },
    /// Compare selected targets (or every target) with their last observed revisions.
    Check {
        selector: Option<String>,
        #[command(flatten)]
        output: JsonOutput,
    },
}

#[derive(Args)]
struct JsonOutput {
    /// Render versioned JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct InventoryArgs {
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    scope: Option<String>,
    #[arg(long)]
    assistant: Option<String>,
    #[arg(long)]
    provenance: Option<String>,
    #[arg(long)]
    tracking: Option<String>,
    #[arg(long)]
    watch: Option<String>,
    #[arg(long, value_name = "FIELD")]
    sort: Option<String>,
    #[arg(long, default_value_t = 0)]
    offset: usize,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    detail: Option<String>,
    #[command(flatten)]
    output: JsonOutput,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    let json = cli.format == OutputFormat::Json
        || match &cli.command {
            Commands::Source { command } => match command {
                SourceCommands::List(output) | SourceCommands::Capabilities(output) => output.json,
                SourceCommands::Show { output, .. } | SourceCommands::Add { output, .. } => {
                    output.json
                }
            },
            Commands::Search(args)
            | Commands::Skill {
                command: SkillCommands::Search(args),
            } => args.output.json,
            Commands::Discover(args) => args.output.json,
            Commands::Skill {
                command: SkillCommands::Preview { output, .. },
            } => output.json,
            Commands::Skill {
                command: SkillCommands::List(args),
            } => args.output.json,
            Commands::Skill {
                command:
                    SkillCommands::Create { output, .. }
                    | SkillCommands::Validate { output, .. }
                    | SkillCommands::Drafts { output, .. }
                    | SkillCommands::Remediate { output, .. }
                    | SkillCommands::Install { output, .. }
                    | SkillCommands::Update { output, .. }
                    | SkillCommands::Remove { output, .. }
                    | SkillCommands::Track { output, .. },
            } => output.json,
            Commands::Watch { command } => match command {
                WatchCommands::Add { output, .. }
                | WatchCommands::Remove { output, .. }
                | WatchCommands::Check { output, .. }
                | WatchCommands::List(output) => output.json,
            },
            Commands::Eval { command } => match command {
                EvalCommands::Run { output, .. } | EvalCommands::Show { output, .. } => output.json,
                EvalCommands::List(output) => output.json,
            },
            Commands::Consistency { command } => match command {
                ConsistencyCommands::Check { output, .. }
                | ConsistencyCommands::Show { output, .. } => output.json,
                ConsistencyCommands::List(output) => output.json,
            },
            Commands::Security { command } => match command {
                SecurityCommands::Check { output, .. }
                | SecurityCommands::Show { output, .. }
                | SecurityCommands::Suppress { output, .. }
                | SecurityCommands::Unsuppress { output, .. } => output.json,
                SecurityCommands::List(output) => output.json,
            },
            Commands::Insight { command } => match command {
                InsightCommands::Generate(output)
                | InsightCommands::Show { output, .. }
                | InsightCommands::Status { output, .. }
                | InsightCommands::Suppress { output, .. }
                | InsightCommands::Unsuppress { output, .. }
                | InsightCommands::Propose { output, .. } => output.json,
                InsightCommands::List { output, .. } => output.json,
            },
            Commands::Audit(output) | Commands::Stats(output) => output.json,
            _ => false,
        };
    let command = match &cli.command {
        Commands::Source {
            command: SourceCommands::List(_),
        } => "source list",
        Commands::Source {
            command: SourceCommands::Show { .. },
        } => "source show",
        Commands::Source {
            command: SourceCommands::Add { .. },
        } => "source add",
        Commands::Source {
            command: SourceCommands::Capabilities(_),
        } => "source capabilities",
        Commands::Watch {
            command: WatchCommands::Add { .. },
        } => "watch add",
        Commands::Watch {
            command: WatchCommands::List(_),
        } => "watch list",
        Commands::Watch {
            command: WatchCommands::Remove { .. },
        } => "watch remove",
        Commands::Watch {
            command: WatchCommands::Check { .. },
        } => "watch check",
        Commands::Eval {
            command: EvalCommands::Run { .. },
        } => "eval run",
        Commands::Eval {
            command: EvalCommands::List(_),
        } => "eval list",
        Commands::Eval {
            command: EvalCommands::Show { .. },
        } => "eval show",
        Commands::Consistency {
            command: ConsistencyCommands::Check { .. },
        } => "consistency check",
        Commands::Consistency {
            command: ConsistencyCommands::List(_),
        } => "consistency list",
        Commands::Consistency {
            command: ConsistencyCommands::Show { .. },
        } => "consistency show",
        Commands::Security {
            command: SecurityCommands::Check { .. },
        } => "security check",
        Commands::Security {
            command: SecurityCommands::List(_),
        } => "security list",
        Commands::Security {
            command: SecurityCommands::Show { .. },
        } => "security show",
        Commands::Security {
            command: SecurityCommands::Suppress { .. },
        } => "security suppress",
        Commands::Security {
            command: SecurityCommands::Unsuppress { .. },
        } => "security unsuppress",
        Commands::Insight {
            command: InsightCommands::Generate(_),
        } => "insight generate",
        Commands::Insight {
            command: InsightCommands::List { .. },
        } => "insight list",
        Commands::Insight {
            command: InsightCommands::Show { .. },
        } => "insight show",
        Commands::Insight {
            command: InsightCommands::Status { .. },
        } => "insight status",
        Commands::Insight {
            command: InsightCommands::Suppress { .. },
        } => "insight suppress",
        Commands::Insight {
            command: InsightCommands::Unsuppress { .. },
        } => "insight unsuppress",
        Commands::Insight {
            command: InsightCommands::Propose { .. },
        } => "insight propose",
        Commands::Audit(_) => "audit",
        Commands::Stats(_) => "stats",
        Commands::Agent => "agent",
        Commands::Search(_)
        | Commands::Skill {
            command: SkillCommands::Search(_),
        } => "skill search",
        Commands::Discover(_) => "discover",
        Commands::Skill {
            command: SkillCommands::Preview { .. },
        } => "skill preview",
        Commands::Skill {
            command: SkillCommands::Create { .. },
        } => "skill create",
        Commands::Skill {
            command: SkillCommands::Validate { .. },
        } => "skill validate",
        Commands::Skill {
            command: SkillCommands::Drafts { .. },
        } => "skill drafts",
        Commands::Skill {
            command: SkillCommands::Remediate { .. },
        } => "skill remediate",
        Commands::Skill {
            command: SkillCommands::Install { .. },
        } => "skill install",
        Commands::Skill {
            command: SkillCommands::Update { .. },
        } => "skill update",
        Commands::Skill {
            command: SkillCommands::Remove { .. },
        } => "skill remove",
        Commands::Skill {
            command: SkillCommands::Track { .. },
        } => "skill track",
        _ => "local",
    };
    let cancel = Cancellation::default();
    let signal_cancel = cancel.clone();
    let signal = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_cancel.cancel();
        }
    });
    let result = run(cli, json, command, &cancel).await;
    signal.abort();
    if let Err(error) = result {
        if json {
            eprintln!(
                "{}",
                serde_json::json!({
                    "schemaVersion": 1, "command": command, "diagnostic": error
                })
            );
        } else {
            eprintln!("tasc: {}", safe_text(&error.to_string()));
        }
        std::process::exit(error.exit_code());
    }
}

async fn run(
    cli: Cli,
    json: bool,
    command_name: &str,
    cancel: &Cancellation,
) -> Result<(), Diagnostic> {
    let (state_path, _) = FsStateRepository::selected_profile_path().map_err(local_error)?;
    let discovery = DiscoveryService::new(
        GhSkill::default(),
        FsStateRepository::new(state_path.clone()),
    );
    let remote = matches!(
        &cli.command,
        Commands::Search(_)
            | Commands::Skill {
                command: SkillCommands::Search(_)
                    | SkillCommands::Preview { .. }
                    | SkillCommands::Install { .. }
                    | SkillCommands::Update { .. }
            }
            | Commands::Source {
                command: SourceCommands::Capabilities(_)
            }
            | Commands::Watch {
                command: WatchCommands::Check { .. }
            }
    );
    if cli.offline && remote {
        return Err(Diagnostic::new(
            "offline",
            "usage",
            "tasc",
            "This command requires GitHub access.",
            "Remove --offline to explicitly allow the operation.",
        ));
    }
    let value = match cli.command {
        Commands::Agent => return agent::run(cli.offline).await,
        Commands::Source {
            command: SourceCommands::List(_),
        } => serde_json::json!({ "sources": discovery.sources()? }),
        Commands::Source {
            command: SourceCommands::Show { selector, .. },
        } => serde_json::json!({ "source": discovery.source(&selector)? }),
        Commands::Source {
            command:
                SourceCommands::Add {
                    locator,
                    dry_run,
                    yes,
                    ..
                },
        } => serde_json::json!({
            "source": discovery.add_source(&locator, dry_run, yes)?, "dryRun": dry_run
        }),
        Commands::Source {
            command: SourceCommands::Capabilities(_),
        } => serde_json::json!(discovery.capabilities(cancel).await?),
        Commands::Search(args)
        | Commands::Skill {
            command: SkillCommands::Search(args),
        } => serde_json::json!({
            "query": args.query, "items": discovery.search(&args.query, cancel).await?
        }),
        Commands::Discover(args) => {
            let local = CopilotAdapter::local().discover().map_err(local_error)?;
            let mut result = discovery
                .discover(&args.query, local, !cli.offline, cancel)
                .await?;
            if args.rerank {
                if cli.offline {
                    result.reranking = RerankingStatus::Unavailable;
                    result.diagnostics.push(Diagnostic::new(
                        "model_reranking_unavailable",
                        "dependency",
                        "tasc",
                        "Model-assisted reranking was not run because offline mode is enabled.",
                        "Remove --offline to explicitly allow Copilot model access.",
                    ));
                } else {
                    match agent::rerank_discovery(&args.query, &result.items, cancel).await {
                        Ok(ids) => {
                            if let Err(diagnostic) = apply_model_ranking(&mut result, &ids) {
                                result.reranking = RerankingStatus::Unavailable;
                                result.diagnostics.push(diagnostic);
                            }
                        }
                        Err(diagnostic) => {
                            result.reranking = RerankingStatus::Unavailable;
                            result.diagnostics.push(diagnostic);
                        }
                    }
                }
            }
            serde_json::to_value(result).map_err(|error| local_error(error.to_string()))?
        }
        Commands::Skill {
            command: SkillCommands::Preview {
                selector, source, ..
            },
        } => serde_json::json!(
            discovery
                .preview(&selector, source.as_deref(), cancel)
                .await?
        ),
        Commands::Watch { command } => run_watch(command, state_path, cancel).await?,
        command @ (Commands::Audit(_) | Commands::Stats(_)) => {
            return run_local(
                command,
                state_path,
                cli.format == OutputFormat::Json,
                cancel,
            )
            .await;
        }
        Commands::Eval { command } => {
            return run_local(
                Commands::Eval { command },
                state_path,
                cli.format == OutputFormat::Json,
                cancel,
            )
            .await;
        }
        Commands::Insight { command } => {
            return run_local(
                Commands::Insight { command },
                state_path,
                cli.format == OutputFormat::Json,
                cancel,
            )
            .await;
        }
        command => {
            return run_local(
                command,
                state_path,
                cli.format == OutputFormat::Json,
                cancel,
            )
            .await;
        }
    };
    let envelope = Envelope::new(command_name, value);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope)
                .map_err(|error| local_error(error.to_string()))?
        );
    } else {
        println!(
            "{} (schema version {})",
            command_name, envelope.schema_version
        );
        render_fields(&envelope.result, 0);
    }
    Ok(())
}

async fn run_watch(
    command: WatchCommands,
    state_path: std::path::PathBuf,
    cancel: &Cancellation,
) -> Result<serde_json::Value, Diagnostic> {
    let artifacts = FsWatchCheckRepository::new(state_path.with_extension("watch-checks"));
    let service = WatchService::new(
        GhSkill::default(),
        FsStateRepository::new(state_path),
        artifacts,
    );
    match command {
        WatchCommands::Add {
            locator,
            path,
            dry_run,
            yes,
            ..
        } => Ok(serde_json::json!({
            "watchlist": service.add(&locator, path.as_deref(), dry_run, yes)?,
            "dryRun": dry_run
        })),
        WatchCommands::List(_) => Ok(serde_json::json!({
            "watchlists": service.list()?
        })),
        WatchCommands::Remove {
            selector,
            dry_run,
            yes,
            ..
        } => Ok(serde_json::json!({
            "removed": service.remove(&selector, dry_run, yes)?,
            "dryRun": dry_run
        })),
        WatchCommands::Check { selector, .. } => Ok(serde_json::json!({
            "checks": service.check(selector.as_deref(), cancel).await?
        })),
    }
}
fn safe_text(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(serde::Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum AuditSeverity {
    Error,
    Warning,
    Optimization,
}

#[derive(serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EvidenceFreshness {
    Current,
    Stale,
    Unavailable,
    Unknown,
}

#[derive(serde::Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum OutcomeStatus {
    Passed,
    Failed,
    Inconclusive,
    NotRun,
    Unavailable,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditFinding {
    code: String,
    severity: AuditSeverity,
    subject: String,
    message: String,
}

#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct AuditSummary {
    errors: usize,
    warnings: usize,
    optimizations: usize,
    inventory_count: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionDiagnostic {
    code: String,
    component: String,
    status: String,
    message: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditReport {
    command: &'static str,
    schema_version: u32,
    generated_at_unix_ms: u128,
    profile: TascProfile,
    summary: AuditSummary,
    findings: Vec<AuditFinding>,
    diagnostics: Vec<ProjectionDiagnostic>,
    exit_code: i32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceOutcome {
    source: String,
    id: String,
    outcome: OutcomeStatus,
    freshness: EvidenceFreshness,
    execution_outcome: Option<EvaluationOutcome>,
}

#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct StatsSummary {
    identities: usize,
    targets: usize,
    installations: usize,
    inventory_items: usize,
    watched_interests: usize,
    audit_findings: AuditSummary,
    scope: BTreeMap<String, usize>,
    tracking: BTreeMap<String, usize>,
    watchlist: BTreeMap<String, usize>,
    provenance: BTreeMap<String, usize>,
    provenance_confidence: BTreeMap<String, usize>,
    lifecycle: BTreeMap<String, usize>,
    repositories: BTreeMap<String, usize>,
    comparable_outcomes: BTreeMap<String, usize>,
    evidence_freshness: BTreeMap<String, usize>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsReport {
    command: &'static str,
    schema_version: u32,
    generated_at_unix_ms: u128,
    profile: TascProfile,
    summary: StatsSummary,
    outcomes: Vec<EvidenceOutcome>,
    diagnostics: Vec<ProjectionDiagnostic>,
    exit_code: i32,
}

fn projection_inputs(
    repository: &FsStateRepository,
    service: &AgentService<CopilotAdapter, FsStateRepository>,
) -> (
    Option<LocalState>,
    Option<Inventory>,
    Vec<ProjectionDiagnostic>,
) {
    let mut diagnostics = Vec::new();
    let state = match repository.load_read_only() {
        Ok(state) => Some(state),
        Err(error) => {
            diagnostics.push(ProjectionDiagnostic {
                code: "local_state_unreadable".to_string(),
                component: "local_state".to_string(),
                status: "unavailable".to_string(),
                message: error,
            });
            None
        }
    };
    let inventory = state
        .as_ref()
        .and_then(|state| match service.inventory_from_state(state) {
            Ok(inventory) => Some(inventory),
            Err(error) => {
                diagnostics.push(ProjectionDiagnostic {
                    code: "inventory_unavailable".to_string(),
                    component: "inventory".to_string(),
                    status: "unavailable".to_string(),
                    message: error,
                });
                None
            }
        });
    (state, inventory, diagnostics)
}

fn audit_report(
    state_path: &std::path::Path,
    profile: TascProfile,
    repository: &FsStateRepository,
    service: &AgentService<CopilotAdapter, FsStateRepository>,
) -> AuditReport {
    let (state, inventory, mut diagnostics) = projection_inputs(repository, service);
    let evaluations = FsRunArtifactRepository::new(state_path.with_extension("evaluations")).list();
    let consistencies =
        FsConsistencyArtifactRepository::new(state_path.with_extension("consistency-checks"))
            .list();
    let security =
        FsSecurityArtifactRepository::new(state_path.with_extension("security-checks")).list();
    let mut findings = Vec::new();
    if state.is_none() {
        findings.push(AuditFinding {
            code: "local_state_unreadable".to_string(),
            severity: AuditSeverity::Error,
            subject: profile.id.clone(),
            message: "Local State could not be read as a supported profile.".to_string(),
        });
    }
    if let Some(state) = state.as_ref() {
        append_state_findings(state, &mut findings);
    }
    if let Some(inventory) = inventory.as_ref() {
        append_inventory_findings(inventory, &mut findings);
    }
    if let Err(error) = &evaluations {
        push_artifact_diagnostic(&mut diagnostics, "evaluations", error);
    }
    if let Err(error) = &consistencies {
        push_artifact_diagnostic(&mut diagnostics, "consistency_checks", error);
    }
    if let Err(error) = &security {
        push_artifact_diagnostic(&mut diagnostics, "security_checks", error);
    }
    let evaluations = evaluations.unwrap_or_default();
    let consistencies = consistencies.unwrap_or_default();
    let security = security.unwrap_or_default();
    append_evidence_findings(
        &evaluations,
        &consistencies,
        &security,
        inventory.as_ref(),
        &mut findings,
    );
    sort_findings(&mut findings);
    let mut summary = summarize_findings(&findings);
    summary.inventory_count = inventory.as_ref().map_or(0, |inventory| inventory.total);
    let exit_code = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.status == "unavailable")
    {
        4
    } else if summary.errors > 0 {
        1
    } else {
        0
    };
    AuditReport {
        command: "audit",
        schema_version: 1,
        generated_at_unix_ms: unix_ms(),
        profile,
        summary,
        findings,
        diagnostics,
        exit_code,
    }
}

fn stats_report(
    state_path: &std::path::Path,
    profile: TascProfile,
    repository: &FsStateRepository,
    service: &AgentService<CopilotAdapter, FsStateRepository>,
) -> StatsReport {
    let (state, inventory, mut diagnostics) = projection_inputs(repository, service);
    let evaluations = FsRunArtifactRepository::new(state_path.with_extension("evaluations")).list();
    let consistencies =
        FsConsistencyArtifactRepository::new(state_path.with_extension("consistency-checks"))
            .list();
    let security =
        FsSecurityArtifactRepository::new(state_path.with_extension("security-checks")).list();
    for (component, result) in [
        ("evaluations", evaluations.as_ref().map(|_| ())),
        ("consistency_checks", consistencies.as_ref().map(|_| ())),
        ("security_checks", security.as_ref().map(|_| ())),
    ] {
        if let Err(error) = result {
            push_artifact_diagnostic(&mut diagnostics, component, error);
        }
    }
    let evaluations = evaluations.unwrap_or_default();
    let consistencies = consistencies.unwrap_or_default();
    let security = security.unwrap_or_default();
    let mut findings = Vec::new();
    if state.is_none() {
        findings.push(AuditFinding {
            code: "local_state_unreadable".to_string(),
            severity: AuditSeverity::Error,
            subject: profile.id.clone(),
            message: "Local State could not be read as a supported profile.".to_string(),
        });
    }
    if let Some(state) = state.as_ref() {
        append_state_findings(state, &mut findings);
    }
    if let Some(inventory) = inventory.as_ref() {
        append_inventory_findings(inventory, &mut findings);
    }
    append_evidence_findings(
        &evaluations,
        &consistencies,
        &security,
        inventory.as_ref(),
        &mut findings,
    );
    sort_findings(&mut findings);
    let mut audit_findings = summarize_findings(&findings);
    audit_findings.inventory_count = inventory.as_ref().map_or(0, |inventory| inventory.total);
    let mut summary = StatsSummary {
        identities: state.as_ref().map_or(0, |state| state.identities.len()),
        targets: state.as_ref().map_or(0, |state| state.targets.len()),
        installations: state.as_ref().map_or(0, |state| state.installations.len()),
        inventory_items: inventory.as_ref().map_or(0, |inventory| inventory.total),
        watched_interests: 0,
        audit_findings,
        ..StatsSummary::default()
    };
    if let Some(inventory) = inventory.as_ref() {
        for item in &inventory.items {
            increment(&mut summary.scope, &item.scope);
            increment(&mut summary.tracking, &item.tracking_state);
            increment(&mut summary.watchlist, &item.watch_state);
            increment(
                &mut summary.provenance,
                &serde_json::to_value(item.provenance.kind)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string()),
            );
            increment(
                &mut summary.provenance_confidence,
                &serde_json::to_value(item.provenance.confidence)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unavailable".to_string()),
            );
            if let Some(status) = item.lifecycle_state {
                increment(
                    &mut summary.lifecycle,
                    &serde_json::to_value(status)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_string))
                        .unwrap_or_else(|| "unknown".to_string()),
                );
            }
            increment(
                &mut summary.repositories,
                &repository_key(&item.identity.source),
            );
            if item.kind == "watchlist" {
                summary.watched_interests += 1;
            }
        }
    }
    let outcomes = evidence_outcomes(
        &evaluations,
        &consistencies,
        &security,
        inventory.as_ref(),
        &mut diagnostics,
    );
    for outcome in &outcomes {
        let status = serde_json::to_value(&outcome.outcome)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let freshness = freshness_name(outcome.freshness);
        increment(&mut summary.evidence_freshness, freshness);
        if matches!(outcome.freshness, EvidenceFreshness::Current) {
            increment(&mut summary.comparable_outcomes, &status);
        }
    }
    let exit_code = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.status == "unavailable")
    {
        4
    } else {
        0
    };
    StatsReport {
        command: "stats",
        schema_version: 1,
        generated_at_unix_ms: unix_ms(),
        profile,
        summary,
        outcomes,
        diagnostics,
        exit_code,
    }
}

fn append_state_findings(state: &LocalState, findings: &mut Vec<AuditFinding>) {
    for (kind, ids) in [
        (
            "identity",
            state
                .identities
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "target",
            state
                .targets
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "installation",
            state
                .installations
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "watchlist",
            state
                .watchlists
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
        ),
    ] {
        let mut seen = std::collections::BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                push_finding(
                    findings,
                    "conflicting_records",
                    AuditSeverity::Error,
                    id,
                    format!("Local State contains duplicate {kind} records."),
                );
            }
        }
    }
    for installation in &state.installations {
        if !state
            .identities
            .iter()
            .any(|identity| identity.id == installation.skill_id)
        {
            push_finding(
                findings,
                "broken_identity_reference",
                AuditSeverity::Error,
                &installation.id,
                "Installation refers to a missing Skill Identity.".to_string(),
            );
        }
        if !state
            .targets
            .iter()
            .any(|target| target.id == installation.target_id)
        {
            push_finding(
                findings,
                "broken_target_reference",
                AuditSeverity::Error,
                &installation.id,
                "Installation refers to a missing Adapter Target.".to_string(),
            );
        }
        if !std::path::Path::new(&installation.path).exists() {
            push_finding(
                findings,
                "missing_installation",
                AuditSeverity::Error,
                &installation.id,
                "Recorded installation path is missing.".to_string(),
            );
        }
    }
    for target in &state.targets {
        if !std::path::Path::new(&target.path).is_dir() {
            push_finding(
                findings,
                "missing_target",
                AuditSeverity::Error,
                &target.id,
                "Adapter Target directory is missing or unavailable.".to_string(),
            );
        }
    }
}

fn append_inventory_findings(inventory: &Inventory, findings: &mut Vec<AuditFinding>) {
    for item in &inventory.items {
        if item.kind == "installation" && !item.tracked {
            push_finding(
                findings,
                "untracked_installation",
                AuditSeverity::Warning,
                &item.action_id,
                "Discovered installation is not recorded in Local State.".to_string(),
            );
        }
        match item.lifecycle_state {
            Some(InstallationStatus::Missing) => push_finding(
                findings,
                "missing_installation",
                AuditSeverity::Error,
                &item.action_id,
                "Recorded installation target is missing.".to_string(),
            ),
            Some(InstallationStatus::Modified) => push_finding(
                findings,
                "modified_installation",
                AuditSeverity::Warning,
                &item.action_id,
                "Installation content differs from its recorded digest.".to_string(),
            ),
            Some(InstallationStatus::Failed) => push_finding(
                findings,
                "failed_installation",
                AuditSeverity::Error,
                &item.action_id,
                "Installation is recorded in a failed lifecycle state.".to_string(),
            ),
            Some(InstallationStatus::Outdated) => push_finding(
                findings,
                "outdated_installation",
                AuditSeverity::Warning,
                &item.action_id,
                "Installation is recorded as outdated.".to_string(),
            ),
            Some(InstallationStatus::Unverified | InstallationStatus::Planned) => push_finding(
                findings,
                "unverified_installation",
                AuditSeverity::Optimization,
                &item.action_id,
                "Installation lacks a verified current content baseline.".to_string(),
            ),
            _ => {}
        }
        if item.provenance.confidence == ProvenanceConfidence::Unavailable {
            push_finding(
                findings,
                "provenance_unavailable",
                AuditSeverity::Optimization,
                &item.action_id,
                "Installation provenance is not available.".to_string(),
            );
        }
    }
}

fn append_evidence_findings(
    evaluations: &[tasc_core::evaluation::RunArtifact],
    consistencies: &[tasc_core::consistency::ConsistencyArtifact],
    security: &[tasc_core::security::SecurityArtifact],
    inventory: Option<&Inventory>,
    findings: &mut Vec<AuditFinding>,
) {
    for artifact in evaluations {
        if evidence_freshness_evaluation(artifact, inventory) == EvidenceFreshness::Stale {
            push_finding(
                findings,
                "stale_evidence",
                AuditSeverity::Warning,
                &artifact.id,
                "Evaluation recorded inputs differ from the currently available inputs."
                    .to_string(),
            );
        }
    }
    for artifact in consistencies {
        if evidence_freshness_consistency(artifact, inventory) == EvidenceFreshness::Stale {
            push_finding(
                findings,
                "stale_evidence",
                AuditSeverity::Warning,
                &artifact.id,
                "Consistency evidence recorded inputs differ from the currently available inputs."
                    .to_string(),
            );
        } else if evidence_freshness_consistency(artifact, inventory) == EvidenceFreshness::Current
        {
            for finding in &artifact.findings {
                if finding.status == ConsistencyStatus::Found
                    && finding.revision_status != tasc_core::consistency::RevisionStatus::Stale
                {
                    push_finding(
                        findings,
                        "unresolved_finding",
                        if finding.severity == tasc_core::consistency::FindingSeverity::Blocking {
                            AuditSeverity::Error
                        } else {
                            AuditSeverity::Warning
                        },
                        &finding.id,
                        "A current Consistency Check contains an unresolved finding.".to_string(),
                    );
                }
            }
        }
    }
    for artifact in security {
        if evidence_freshness_security(artifact, inventory) == EvidenceFreshness::Stale {
            push_finding(
                findings,
                "stale_evidence",
                AuditSeverity::Warning,
                &artifact.id,
                "Security evidence recorded content differs from the current content.".to_string(),
            );
        } else if evidence_freshness_security(artifact, inventory) == EvidenceFreshness::Current {
            for finding in &artifact.findings {
                if !artifact
                    .suppressed_fingerprints
                    .contains(&finding.fingerprint)
                {
                    push_finding(
                        findings,
                        "unresolved_finding",
                        if matches!(
                            finding.severity,
                            tasc_core::security::SecuritySeverity::High
                                | tasc_core::security::SecuritySeverity::Critical
                        ) {
                            AuditSeverity::Error
                        } else {
                            AuditSeverity::Warning
                        },
                        &finding.id,
                        "A current Security Check contains an unresolved finding.".to_string(),
                    );
                }
            }
        }
    }
}

fn evidence_freshness_evaluation(
    artifact: &tasc_core::evaluation::RunArtifact,
    inventory: Option<&Inventory>,
) -> EvidenceFreshness {
    let Some(inventory) = inventory else {
        return EvidenceFreshness::Unavailable;
    };
    let Some(item) = inventory_installation(
        inventory,
        &artifact.identity.id,
        artifact.installation_id.as_deref(),
        artifact.package_path.as_deref(),
    ) else {
        return EvidenceFreshness::Stale;
    };
    let mut compared = false;
    if let Some(recorded) = artifact.source_revision.as_ref() {
        if let Some(current) = item.source_revision.as_ref() {
            compared = true;
            if recorded.value != *current {
                return EvidenceFreshness::Stale;
            }
        }
    }
    if let Some(path) = artifact.package_path.as_deref() {
        if std::path::Path::new(path) != std::path::Path::new(&item.path) {
            return EvidenceFreshness::Stale;
        }
        if let Some(recorded) = artifact.package_digest.as_ref() {
            match tasc_agent::service::skill_content_digest(std::path::Path::new(&item.path)) {
                Ok(current) => {
                    compared = true;
                    if &current != recorded {
                        return EvidenceFreshness::Stale;
                    }
                }
                Err(_) => return EvidenceFreshness::Unavailable,
            }
        }
    }
    if compared {
        EvidenceFreshness::Current
    } else {
        EvidenceFreshness::Unknown
    }
}

fn evidence_freshness_consistency(
    artifact: &tasc_core::consistency::ConsistencyArtifact,
    inventory: Option<&Inventory>,
) -> EvidenceFreshness {
    let mut compared = false;
    for skill in &artifact.skills {
        let Some(inventory) = inventory else {
            return EvidenceFreshness::Unavailable;
        };
        let Some(current_item) =
            inventory_installation(inventory, &skill.identity.id, None, Some(&skill.path))
        else {
            return EvidenceFreshness::Stale;
        };
        match tasc_agent::service::skill_content_digest(std::path::Path::new(&current_item.path)) {
            Ok(current) => {
                compared = true;
                if current != skill.content_digest {
                    return EvidenceFreshness::Stale;
                }
            }
            Err(_) => return EvidenceFreshness::Unavailable,
        }
    }
    if compared {
        EvidenceFreshness::Current
    } else {
        EvidenceFreshness::Unknown
    }
}

fn evidence_freshness_security(
    artifact: &tasc_core::security::SecurityArtifact,
    inventory: Option<&Inventory>,
) -> EvidenceFreshness {
    let mut compared = false;
    for skill in &artifact.skills {
        let Some(inventory) = inventory else {
            return EvidenceFreshness::Unavailable;
        };
        let Some(current_item) =
            inventory_installation(inventory, &skill.identity.id, None, Some(&skill.path))
        else {
            return EvidenceFreshness::Stale;
        };
        let Some(recorded) = skill.content_digest.as_ref() else {
            continue;
        };
        match tasc_agent::service::skill_content_digest(std::path::Path::new(&current_item.path)) {
            Ok(current) => {
                compared = true;
                if &current != recorded {
                    return EvidenceFreshness::Stale;
                }
            }
            Err(_) => return EvidenceFreshness::Unavailable,
        }
    }
    if compared {
        EvidenceFreshness::Current
    } else {
        EvidenceFreshness::Unknown
    }
}

fn inventory_installation<'a>(
    inventory: &'a Inventory,
    identity_id: &str,
    installation_id: Option<&str>,
    path: Option<&str>,
) -> Option<&'a tasc_core::InventoryItem> {
    inventory.items.iter().find(|item| {
        item.kind == "installation"
            && item.identity.id == identity_id
            && installation_id.is_none_or(|id| item.installation_id == id)
            && path.is_none_or(|path| item.path == path)
    })
}

fn evidence_outcomes(
    evaluations: &[tasc_core::evaluation::RunArtifact],
    consistencies: &[tasc_core::consistency::ConsistencyArtifact],
    security: &[tasc_core::security::SecurityArtifact],
    inventory: Option<&Inventory>,
    diagnostics: &mut Vec<ProjectionDiagnostic>,
) -> Vec<EvidenceOutcome> {
    let mut outcomes = Vec::new();
    for artifact in evaluations {
        outcomes.push(EvidenceOutcome {
            source: "evaluation".to_string(),
            id: artifact.id.clone(),
            outcome: evaluation_outcome(artifact.outcome),
            freshness: evidence_freshness_evaluation(artifact, inventory),
            execution_outcome: Some(artifact.execution_outcome),
        });
    }
    for artifact in consistencies {
        outcomes.push(EvidenceOutcome {
            source: "consistency".to_string(),
            id: artifact.id.clone(),
            outcome: consistency_outcome(artifact),
            freshness: evidence_freshness_consistency(artifact, inventory),
            execution_outcome: None,
        });
    }
    for artifact in security {
        outcomes.push(EvidenceOutcome {
            source: "security".to_string(),
            id: artifact.id.clone(),
            outcome: match artifact.status {
                SecurityCheckStatus::Complete if artifact.findings.is_empty() => {
                    OutcomeStatus::Passed
                }
                SecurityCheckStatus::Complete => OutcomeStatus::Failed,
                SecurityCheckStatus::Partial => OutcomeStatus::Inconclusive,
            },
            freshness: evidence_freshness_security(artifact, inventory),
            execution_outcome: None,
        });
    }
    for component in [
        "local_state",
        "inventory",
        "evaluations",
        "consistency_checks",
        "security_checks",
    ] {
        if diagnostics.iter().any(|diagnostic| {
            diagnostic.component == component && diagnostic.status == "unavailable"
        }) {
            outcomes.push(EvidenceOutcome {
                source: component.to_string(),
                id: String::new(),
                outcome: OutcomeStatus::Unavailable,
                freshness: EvidenceFreshness::Unavailable,
                execution_outcome: None,
            });
        }
    }
    outcomes.sort_by(|left, right| (&left.source, &left.id).cmp(&(&right.source, &right.id)));
    outcomes
}

fn evaluation_outcome(outcome: EvaluationOutcome) -> OutcomeStatus {
    match outcome {
        EvaluationOutcome::Passed => OutcomeStatus::Passed,
        EvaluationOutcome::Failed => OutcomeStatus::Failed,
        EvaluationOutcome::Inconclusive => OutcomeStatus::Inconclusive,
        EvaluationOutcome::NotRun => OutcomeStatus::NotRun,
    }
}

fn consistency_outcome(artifact: &tasc_core::consistency::ConsistencyArtifact) -> OutcomeStatus {
    if artifact
        .categories
        .iter()
        .any(|category| category.status == ConsistencyStatus::Found)
    {
        OutcomeStatus::Failed
    } else if artifact
        .categories
        .iter()
        .any(|category| category.status == ConsistencyStatus::Inconclusive)
    {
        OutcomeStatus::Inconclusive
    } else if artifact
        .categories
        .iter()
        .all(|category| category.status == ConsistencyStatus::NotRun)
    {
        OutcomeStatus::NotRun
    } else {
        OutcomeStatus::Passed
    }
}

fn push_artifact_diagnostic(
    diagnostics: &mut Vec<ProjectionDiagnostic>,
    component: &str,
    error: &str,
) {
    diagnostics.push(ProjectionDiagnostic {
        code: "evidence_unavailable".to_string(),
        component: component.to_string(),
        status: "unavailable".to_string(),
        message: error.to_string(),
    });
}

fn push_finding(
    findings: &mut Vec<AuditFinding>,
    code: &str,
    severity: AuditSeverity,
    subject: &str,
    message: String,
) {
    findings.push(AuditFinding {
        code: code.to_string(),
        severity,
        subject: subject.to_string(),
        message,
    });
}

fn sort_findings(findings: &mut Vec<AuditFinding>) {
    findings.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.subject.cmp(&right.subject))
            .then_with(|| severity_rank(&left.severity).cmp(&severity_rank(&right.severity)))
    });
    findings.dedup_by(|left, right| left.code == right.code && left.subject == right.subject);
}

fn severity_rank(severity: &AuditSeverity) -> u8 {
    match severity {
        AuditSeverity::Error => 0,
        AuditSeverity::Warning => 1,
        AuditSeverity::Optimization => 2,
    }
}

fn summarize_findings(findings: &[AuditFinding]) -> AuditSummary {
    let mut summary = AuditSummary::default();
    for finding in findings {
        match finding.severity {
            AuditSeverity::Error => summary.errors += 1,
            AuditSeverity::Warning => summary.warnings += 1,
            AuditSeverity::Optimization => summary.optimizations += 1,
        }
    }
    summary
}

fn increment(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_default() += 1;
}

fn freshness_name(freshness: EvidenceFreshness) -> &'static str {
    match freshness {
        EvidenceFreshness::Current => "current",
        EvidenceFreshness::Stale => "stale",
        EvidenceFreshness::Unavailable => "unavailable",
        EvidenceFreshness::Unknown => "unknown",
    }
}

fn repository_key(source: &str) -> String {
    let source = source
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let source = source.strip_prefix("github.com/").unwrap_or(source);
    if source.starts_with("file:") || source.starts_with('/') || source.starts_with("local:") {
        "local".to_string()
    } else {
        source
            .trim_end_matches(".git")
            .trim_end_matches('/')
            .to_string()
    }
}

fn render_projection_report<T: serde::Serialize>(
    report: &T,
    json: bool,
    command: &str,
    exit_code: i32,
) -> Result<(), Diagnostic> {
    let value = serde_json::to_value(report).map_err(|error| local_error(error.to_string()))?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|error| local_error(error.to_string()))?
        );
    } else {
        println!("{command} (schema version 1)");
        render_fields(&value, 0);
    }
    match exit_code {
        0 => Ok(()),
        1 => Err(Diagnostic::new(
            "audit_errors",
            "finding",
            "audit",
            "Audit detected error findings.",
            "Inspect the findings and repair the affected profile records or installations.",
        )),
        _ => Err(Diagnostic::new(
            "projection_unavailable",
            "filesystem",
            command,
            "One or more profile inputs could not be inspected.",
            "Inspect the diagnostics and retry when the profile and evidence are readable.",
        )),
    }
}

fn run_audit(
    state_path: std::path::PathBuf,
    repository: FsStateRepository,
    service: &AgentService<CopilotAdapter, FsStateRepository>,
    json: bool,
) -> Result<(), Diagnostic> {
    let profile = repository.profile();
    let report = audit_report(&state_path, profile, &repository, service);
    render_projection_report(&report, json, "audit", report.exit_code)
}

fn run_stats(
    state_path: std::path::PathBuf,
    repository: FsStateRepository,
    service: &AgentService<CopilotAdapter, FsStateRepository>,
    json: bool,
) -> Result<(), Diagnostic> {
    let profile = repository.profile();
    let report = stats_report(&state_path, profile, &repository, service);
    render_projection_report(&report, json, "stats", report.exit_code)
}

fn render_fields(value: &serde_json::Value, indent: usize) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                print!("{:indent$}{key}: ", "");
                if let Some(text) = value.as_str() {
                    println!("{}", safe_text(text));
                } else if value.is_object() || value.is_array() {
                    println!();
                    render_fields(value, indent + 2);
                } else {
                    println!("{value}");
                }
            }
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                println!("{:indent$}(none)", "");
            }
            for item in items {
                println!("{:indent$}-", "");
                render_fields(item, indent + 2);
            }
        }
        _ => println!("{:indent$}{value}", ""),
    }
}

fn local_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "local_state_failed",
        "operational",
        "filesystem",
        &message,
        "Inspect Local State and retry.",
    )
}

async fn run_local(
    command: Commands,
    state_path: std::path::PathBuf,
    json: bool,
    cancel: &Cancellation,
) -> Result<(), Diagnostic> {
    let repository = FsStateRepository::new(state_path.clone());
    let service = AgentService::new(CopilotAdapter::local(), repository.clone());
    match command {
        Commands::Audit(output) => run_audit(state_path, repository, &service, output.json || json),
        Commands::Stats(output) => run_stats(state_path, repository, &service, output.json || json),
        Commands::Insight { command } => {
            let service = InsightService::new(
                CopilotAdapter::local(),
                repository,
                FsConsistencyArtifactRepository::new(
                    state_path.with_extension("consistency-checks"),
                ),
                FsRunArtifactRepository::new(state_path.with_extension("evaluations")),
                FsInsightArtifactRepository::new(state_path.with_extension("insights")),
                FsInsightLifecycleRepository::new(state_path.with_extension("insight-lifecycle")),
            );
            match command {
                InsightCommands::Generate(output) => render_local_artifact(
                    serde_json::json!({ "insights": service.generate()? }),
                    "insight generate",
                    output.json || json,
                ),
                InsightCommands::List { all, output } => {
                    let mut insights = service.list()?;
                    if !all {
                        insights.retain(|view| view.status != InsightStatus::Suppressed);
                    }
                    render_local_artifact(
                        serde_json::json!({ "insights": insights }),
                        "insight list",
                        output.json || json,
                    )
                }
                InsightCommands::Show { id, output } => render_local_artifact(
                    serde_json::to_value(service.show(&id)?)
                        .map_err(|error| local_error(error.to_string()))?,
                    "insight show",
                    output.json || json,
                ),
                InsightCommands::Status {
                    id,
                    status,
                    reason,
                    snooze_days,
                    output,
                } => {
                    if matches!(status, InsightStatusArg::Snoozed) != snooze_days.is_some() {
                        return Err(Diagnostic::new(
                            "invalid_insight_status",
                            "usage",
                            "insight",
                            "Only snoozed status accepts --snooze-days, and snoozed status requires it.",
                            "Pass --snooze-days with `snoozed`, or omit it for other statuses.",
                        ));
                    }
                    let status = match status {
                        InsightStatusArg::New => InsightStatus::New,
                        InsightStatusArg::Acknowledged => InsightStatus::Acknowledged,
                        InsightStatusArg::Snoozed => InsightStatus::Snoozed,
                        InsightStatusArg::Resolved => InsightStatus::Resolved,
                    };
                    let until = snooze_days.map(|days| {
                        unix_ms().saturating_add(u128::from(days).saturating_mul(86_400_000))
                    });
                    render_local_artifact(
                        serde_json::to_value(service.set_status(
                            &id,
                            status,
                            reason.as_deref(),
                            until,
                        )?)
                        .map_err(|error| local_error(error.to_string()))?,
                        "insight status",
                        output.json || json,
                    )
                }
                InsightCommands::Suppress {
                    id,
                    evidence,
                    reason,
                    expires_in_days,
                    output,
                } => {
                    let expires = unix_ms()
                        .saturating_add(u128::from(expires_in_days).saturating_mul(86_400_000));
                    render_local_artifact(
                        serde_json::to_value(service.suppress(
                            &id,
                            evidence.as_deref(),
                            &reason,
                            expires,
                        )?)
                        .map_err(|error| local_error(error.to_string()))?,
                        "insight suppress",
                        output.json || json,
                    )
                }
                InsightCommands::Unsuppress { id, reason, output } => render_local_artifact(
                    serde_json::to_value(service.unsuppress(&id, &reason)?)
                        .map_err(|error| local_error(error.to_string()))?,
                    "insight unsuppress",
                    output.json || json,
                ),
                InsightCommands::Propose { id, output } => render_local_artifact(
                    serde_json::json!({ "proposals": service.proposals(&id)? }),
                    "insight propose",
                    output.json || json,
                ),
            }
        }
        Commands::Eval { command } => {
            let service = EvaluationService::new(
                CopilotAdapter::local(),
                repository,
                FsRunArtifactRepository::new(state_path.with_extension("evaluations")),
            );
            match command {
                EvalCommands::Run {
                    selector,
                    revision,
                    waza,
                    output,
                } => {
                    let artifact = service.run(&selector, revision.as_deref(), waza)?;
                    render_evaluation(
                        serde_json::to_value(artifact)
                            .map_err(|error| local_error(error.to_string()))?,
                        "eval run",
                        output.json || json,
                    )
                }
                EvalCommands::List(output) => render_evaluation(
                    serde_json::json!({ "runs": service.list()? }),
                    "eval list",
                    output.json || json,
                ),
                EvalCommands::Show { id, output } => {
                    let artifact = service.show(&id)?;
                    render_evaluation(
                        serde_json::to_value(artifact)
                            .map_err(|error| local_error(error.to_string()))?,
                        "eval show",
                        output.json || json,
                    )
                }
            }
        }
        Commands::Consistency { command } => {
            let service = ConsistencyService::new(
                CopilotAdapter::local(),
                repository,
                FsConsistencyArtifactRepository::new(
                    state_path.with_extension("consistency-checks"),
                ),
                FsRunArtifactRepository::new(state_path.with_extension("evaluations")),
                FsWatchCheckRepository::new(state_path.with_extension("watch-checks")),
            );
            match command {
                ConsistencyCommands::Check {
                    selectors,
                    all_installed,
                    output,
                } => {
                    let artifact = service.check(&selectors, all_installed)?;
                    let has_blocking_finding = artifact.findings.iter().any(|finding| {
                        finding.severity == tasc_core::consistency::FindingSeverity::Blocking
                    });
                    render_local_artifact(
                        serde_json::to_value(&artifact)
                            .map_err(|error| local_error(error.to_string()))?,
                        "consistency check",
                        output.json || json,
                    )?;
                    if has_blocking_finding {
                        return Err(Diagnostic::new(
                            "consistency_findings",
                            "finding",
                            "consistency",
                            "The Consistency Check found one or more blocking findings.",
                            "Review the immutable Check Artifact and resolve or explicitly govern the findings.",
                        ));
                    }
                    Ok(())
                }
                ConsistencyCommands::List(output) => render_local_artifact(
                    serde_json::json!({ "checks": service.list()? }),
                    "consistency list",
                    output.json || json,
                ),
                ConsistencyCommands::Show { id, output } => {
                    let artifact = service.show(&id)?;
                    render_local_artifact(
                        serde_json::to_value(artifact)
                            .map_err(|error| local_error(error.to_string()))?,
                        "consistency show",
                        output.json || json,
                    )
                }
            }
        }
        Commands::Security { command } => {
            let service = SecurityService::new(
                CopilotAdapter::local(),
                repository,
                FsSecurityArtifactRepository::new(state_path.with_extension("security-checks")),
                FsSecuritySuppressionRepository::new(
                    state_path.with_extension("security-suppressions"),
                ),
            );
            match command {
                SecurityCommands::Check {
                    selectors,
                    all_installed,
                    fail_on,
                    output,
                } => {
                    let fail_on = fail_on.map(SecuritySeverity::from);
                    let artifact = service.check(&selectors, all_installed, fail_on)?;
                    let blocked = artifact.policy.fail_on.is_some_and(|threshold| {
                        artifact.findings.iter().any(|finding| {
                            finding.deterministic
                                && finding.confidence_score >= artifact.policy.minimum_confidence
                                && finding.severity >= threshold
                                && !artifact
                                    .suppressed_fingerprints
                                    .contains(&finding.fingerprint)
                        })
                    });
                    render_local_artifact(
                        serde_json::to_value(&artifact)
                            .map_err(|error| local_error(error.to_string()))?,
                        "security check",
                        output.json || json,
                    )?;
                    if blocked {
                        return Err(Diagnostic::new(
                                "security_findings",
                                "finding",
                                "security",
                                "The Security Check found policy-selected high-confidence findings.",
                                "Review the immutable Security Check Artifact and explicitly address the findings.",
                            ));
                    }
                    Ok(())
                }
                SecurityCommands::List(output) => render_local_artifact(
                    serde_json::json!({ "checks": service.list()? }),
                    "security list",
                    output.json || json,
                ),
                SecurityCommands::Show { id, output } => {
                    let artifact = service.show(&id)?;
                    render_local_artifact(
                        serde_json::to_value(artifact)
                            .map_err(|error| local_error(error.to_string()))?,
                        "security show",
                        output.json || json,
                    )
                }
                SecurityCommands::Suppress {
                    fingerprint,
                    reason,
                    expires_in_days,
                    output,
                } => render_local_artifact(
                    serde_json::to_value(service.suppress(
                        &fingerprint,
                        &reason,
                        expires_in_days,
                    )?)
                    .map_err(|error| local_error(error.to_string()))?,
                    "security suppress",
                    output.json || json,
                ),
                SecurityCommands::Unsuppress {
                    fingerprint,
                    reason,
                    output,
                } => render_local_artifact(
                    serde_json::to_value(service.unsuppress(&fingerprint, &reason)?)
                        .map_err(|error| local_error(error.to_string()))?,
                    "security unsuppress",
                    output.json || json,
                ),
            }
        }
        Commands::Skill {
            command:
                SkillCommands::Create {
                    workspace,
                    name,
                    purpose,
                    description,
                    triggers,
                    non_goals,
                    dry_run,
                    yes,
                    output,
                },
        } => {
            let missing = [
                ("name", name.as_deref()),
                ("purpose", purpose.as_deref()),
                ("description", description.as_deref()),
                ("triggers", triggers.as_deref()),
                ("non-goals", non_goals.as_deref()),
            ]
            .into_iter()
            .filter_map(|(field, value)| {
                value
                    .is_none_or(|value| value.trim().is_empty())
                    .then_some(field)
            })
            .collect::<Vec<_>>();
            if !missing.is_empty() && dry_run && !std::io::stdin().is_terminal() {
                return render_local_artifact(
                    serde_json::json!({
                        "created": false,
                        "workspace": workspace,
                        "questions": missing.iter().map(|field| authoring_question(field)).collect::<Vec<_>>(),
                    }),
                    "skill create",
                    output.json || json,
                );
            }
            let input = SkillDraftInput {
                workspace,
                name: prompt_value(name, "Skill name (lowercase slug)")?,
                purpose: prompt_value(purpose, "What should the skill help accomplish?")?,
                description: prompt_value(description, "Describe the skill in one sentence")?,
                triggers: prompt_value(triggers, "When should the skill activate?")?,
                non_goals: prompt_value(non_goals, "What should the skill not do?")?,
            };
            let service = SkillAuthoringService::new(
                CopilotAdapter::local(),
                repository,
                FsSkillDraftRepository::new(state_path.with_extension("drafts")),
            );
            let plan = service.plan(&input).map_err(local_error)?;
            if dry_run {
                return render_local_artifact(
                    serde_json::json!({ "created": false, "plan": plan }),
                    "skill create",
                    output.json || json,
                );
            }
            if !yes {
                confirm_action(
                    &format!(
                        "Create an incomplete Skill Draft at {}:\n{}",
                        plan.skill_path, plan.preview
                    ),
                    "Write this Skill Draft to the selected Workspace?",
                )?;
            }
            let draft = service.create(&input, true).map_err(local_error)?;
            let follow_up_actions = if draft.status == tasc_core::authoring::SkillDraftStatus::Ready
            {
                vec!["install", "share", "evaluation"]
            } else {
                Vec::new()
            };
            render_local_artifact(
                serde_json::json!({
                    "draft": draft,
                    "followUpActions": follow_up_actions,
                    "actionsExecuted": [],
                }),
                "skill create",
                output.json || json,
            )
        }
        Commands::Skill {
            command:
                SkillCommands::Validate {
                    id,
                    workspace,
                    output,
                },
        } => {
            let service = SkillAuthoringService::new(
                CopilotAdapter::local(),
                repository,
                FsSkillDraftRepository::new(state_path.with_extension("drafts")),
            );
            let validation = service
                .validate_report(&id, &workspace)
                .map_err(local_error)?;
            render_local_artifact(
                serde_json::json!({ "validation": validation }),
                "skill validate",
                output.json || json,
            )
        }
        Commands::Skill {
            command: SkillCommands::Drafts { workspace, output },
        } => {
            let service = SkillAuthoringService::new(
                CopilotAdapter::local(),
                repository,
                FsSkillDraftRepository::new(state_path.with_extension("drafts")),
            );
            let drafts = service.list(workspace.as_deref()).map_err(local_error)?;
            render_local_artifact(
                serde_json::json!({ "drafts": drafts }),
                "skill drafts",
                output.json || json,
            )
        }
        Commands::Skill {
            command:
                SkillCommands::Remediate {
                    id,
                    workspace,
                    description,
                    dry_run,
                    yes,
                    output,
                },
        } => {
            let service = SkillAuthoringService::new(
                CopilotAdapter::local(),
                repository,
                FsSkillDraftRepository::new(state_path.with_extension("drafts")),
            );
            let plan = service
                .plan_missing_description(&id, &workspace, &description)
                .map_err(local_error)?;
            if dry_run {
                return render_local_artifact(
                    serde_json::json!({ "approved": false, "plan": plan }),
                    "skill remediate",
                    output.json || json,
                );
            }
            if !yes {
                let rendered_plan = serde_json::to_string_pretty(&plan).map_err(|error| {
                    local_error(format!("cannot render remediation plan: {error}"))
                })?;
                confirm_action(&rendered_plan, "Apply this Workspace remediation?")?;
            }
            let draft = service
                .apply_remediation(&plan, true)
                .map_err(local_error)?;
            render_local_artifact(
                serde_json::json!({ "draft": draft }),
                "skill remediate",
                output.json || json,
            )
        }
        Commands::Skill {
            command: SkillCommands::List(args),
        } => {
            if !json && !args.output.json && std::io::stdin().is_terminal() {
                prompt_tracking_onboarding(&service)?;
            }
            let mut inventory = service.inventory().map_err(local_error)?;
            inventory.profile = Some(repository.profile());
            let inventory = inventory
                .select(&InventoryQuery {
                    status: args.status,
                    scope: args.scope,
                    assistant: args.assistant,
                    provenance: args.provenance,
                    tracking: args.tracking,
                    watch: args.watch,
                    sort: args.sort,
                    offset: args.offset,
                    limit: args.limit,
                    detail: args.detail,
                })
                .map_err(local_error)?;
            render_inventory(&inventory, args.output.json || json).map_err(local_error)
        }
        Commands::Skill {
            command: SkillCommands::Adopt { id },
        } => {
            let installation = service.adopt(&id).map_err(local_error)?;
            println!(
                "Adopted {} at {} ({})",
                installation.skill_id, installation.path, installation.target_id
            );
            Ok(())
        }
        Commands::Skill {
            command:
                SkillCommands::Track {
                    selectors,
                    all,
                    output,
                },
        } => {
            let installations = service.track(&selectors, all).map_err(local_error)?;
            let count = installations.len();
            render_local_artifact(
                serde_json::json!({
                    "tracked": installations,
                    "count": count,
                }),
                "skill track",
                output.json || json,
            )
        }
        Commands::State {
            command: StateCommands::Status(output),
        } => render_status(&service.status().map_err(local_error)?, output.json || json)
            .map_err(local_error),
        Commands::Skill {
            command:
                SkillCommands::Install {
                    selector,
                    source,
                    scope,
                    revision,
                    dry_run,
                    yes,
                    force,
                    output,
                },
        } => {
            let discovery = DiscoveryService::new(GhSkill::default(), repository.clone());
            let preview = discovery
                .preview(&selector, source.as_deref(), cancel)
                .await?;
            let relative_path = preview.path;
            let name = relative_path
                .trim_end_matches("/SKILL.md")
                .rsplit('/')
                .next()
                .unwrap_or(&selector)
                .to_string();
            let identity = SkillIdentity {
                id: format!("skill:{}/{}", preview.source.locator, relative_path),
                name,
                source: preview.source.locator,
                relative_path,
            };
            let scope = match scope {
                Some(scope) => scope,
                None if in_git_repository() => InstallScope::Project,
                None => {
                    return Err(Diagnostic::new(
                        "target_required",
                        "usage",
                        "tasc",
                        "Install scope must be selected outside a Git repository.",
                        "Pass --scope project or --scope user.",
                    ))
                }
            };
            let target = CopilotAdapter::target(match scope {
                InstallScope::Project => "project",
                InstallScope::User => "user",
            })
            .map_err(local_error)?;
            let lifecycle =
                LifecycleService::new(CopilotAdapter::local(), GhSkill::default(), repository);
            if dry_run {
                let result = lifecycle
                    .install(
                        identity,
                        target,
                        revision.as_deref(),
                        true,
                        force,
                        false,
                        cancel,
                    )
                    .await?;
                return render_lifecycle(&result, "skill install", output.json || json);
            }
            let plan = lifecycle
                .install(
                    identity.clone(),
                    target.clone(),
                    revision.as_deref(),
                    true,
                    force,
                    false,
                    cancel,
                )
                .await?;
            if plan.existing && !force {
                let result = lifecycle
                    .install(
                        identity,
                        target,
                        revision.as_deref(),
                        false,
                        false,
                        false,
                        cancel,
                    )
                    .await?;
                return render_lifecycle(&result, "skill install", output.json || json);
            }
            let approved = if yes {
                true
            } else {
                confirm_plan(&plan, "Install this skill?")?
            };
            let result = lifecycle
                .install(
                    identity,
                    target,
                    revision.as_deref(),
                    false,
                    force,
                    approved,
                    cancel,
                )
                .await?;
            render_lifecycle(&result, "skill install", output.json || json)
        }
        Commands::Skill {
            command:
                SkillCommands::Update {
                    selector,
                    dry_run,
                    yes,
                    force,
                    output,
                },
        } => {
            let lifecycle =
                LifecycleService::new(CopilotAdapter::local(), GhSkill::default(), repository);
            let plan = lifecycle
                .update(&selector, true, force, false, cancel)
                .await?;
            if dry_run || plan.status == tasc_core::InstallationStatus::Missing {
                let mut result = plan;
                result.dry_run = dry_run;
                return render_lifecycle(&result, "skill update", output.json || json);
            }
            if plan.modified && !force {
                return lifecycle
                    .update(&selector, false, false, false, cancel)
                    .await
                    .and_then(|result| {
                        render_lifecycle(&result, "skill update", output.json || json)
                    });
            }
            if plan.status == tasc_core::InstallationStatus::Installed && !plan.outdated {
                return render_lifecycle(&plan, "skill update", output.json || json);
            }
            let approved = if yes {
                true
            } else {
                confirm_plan(&plan, "Apply this update?")?
            };
            let result = lifecycle
                .update(&selector, false, force, approved, cancel)
                .await?;
            render_lifecycle(&result, "skill update", output.json || json)
        }
        Commands::Skill {
            command:
                SkillCommands::Remove {
                    selector,
                    dry_run,
                    yes,
                    force,
                    output,
                },
        } => {
            let lifecycle =
                LifecycleService::new(CopilotAdapter::local(), GhSkill::default(), repository);
            let plan = lifecycle.remove(&selector, true, force, false)?;
            if dry_run {
                return render_lifecycle(&plan, "skill remove", output.json || json);
            }
            if plan.modified && !force {
                return lifecycle
                    .remove(&selector, false, false, false)
                    .and_then(|result| {
                        render_lifecycle(&result, "skill remove", output.json || json)
                    });
            }
            let approved = if yes {
                true
            } else {
                confirm_plan(&plan, "Remove this Installation?")?
            };
            let result = lifecycle.remove(&selector, false, force, approved)?;
            render_lifecycle(&result, "skill remove", output.json || json)
        }
        _ => unreachable!("discovery commands are dispatched by the shared discovery service"),
    }
}

fn render_evaluation(
    result: serde_json::Value,
    command: &str,
    json: bool,
) -> Result<(), Diagnostic> {
    if json {
        let envelope = Envelope::new(command, result);
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope)
                .map_err(|error| local_error(format!("cannot render Evaluation: {error}")))?
        );
    } else {
        println!("{command} (schema version 1)");
        render_fields(&result, 0);
    }
    Ok(())
}

fn render_local_artifact(
    result: serde_json::Value,
    command: &str,
    json: bool,
) -> Result<(), Diagnostic> {
    if json {
        let envelope = Envelope::new(command, result);
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope)
                .map_err(|error| local_error(format!("cannot render artifact: {error}")))?
        );
    } else {
        println!("{command} (schema version 1)");
        render_fields(&result, 0);
    }
    Ok(())
}

fn prompt_tracking_onboarding(
    service: &AgentService<CopilotAdapter, FsStateRepository>,
) -> Result<(), Diagnostic> {
    let onboarding = service.tracking_onboarding().map_err(local_error)?;
    if onboarding.completed || onboarding.candidates.is_empty() {
        return Ok(());
    }
    eprintln!(
        "TASC found {} untracked installation(s) in this Profile.",
        onboarding.candidates.len()
    );
    eprint!("Track all, select installations, or not now? [a/s/n] ");
    std::io::Write::flush(&mut std::io::stderr())
        .map_err(|error| local_error(format!("cannot write tracking prompt: {error}")))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| local_error(format!("cannot read tracking choice: {error}")))?;
    let choice = answer.trim().to_ascii_lowercase();
    let (selected_ids, all, defer) = match choice.as_str() {
        "a" | "all" => (
            onboarding
                .candidates
                .iter()
                .map(|item| item.action_id.clone())
                .collect::<Vec<_>>(),
            true,
            false,
        ),
        "s" | "select" | "selected" => {
            for (index, item) in onboarding.candidates.iter().enumerate() {
                eprintln!(
                    "  {}. {} [{}] {}",
                    index + 1,
                    item.identity.name,
                    item.action_id,
                    item.target
                        .as_ref()
                        .map(|target| target.assistant.as_str())
                        .unwrap_or("unknown")
                );
            }
            eprint!("Enter comma-separated installation numbers or IDs: ");
            std::io::Write::flush(&mut std::io::stderr())
                .map_err(|error| local_error(format!("cannot write selection prompt: {error}")))?;
            answer.clear();
            std::io::stdin()
                .read_line(&mut answer)
                .map_err(|error| local_error(format!("cannot read tracking selection: {error}")))?;
            let mut selected = Vec::new();
            for token in
                answer.split(|character: char| character == ',' || character.is_whitespace())
            {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                if let Ok(index) = token.parse::<usize>() {
                    if let Some(item) = index
                        .checked_sub(1)
                        .and_then(|index| onboarding.candidates.get(index))
                    {
                        selected.push(item.action_id.clone());
                    } else {
                        return Err(Diagnostic::new(
                            "invalid_tracking_selection",
                            "usage",
                            "tasc",
                            "Selected installation number is out of range.",
                            "Select one or more numbers shown in the onboarding list.",
                        ));
                    }
                } else if onboarding
                    .candidates
                    .iter()
                    .any(|item| item.action_id == token)
                {
                    selected.push(token.to_string());
                } else {
                    return Err(Diagnostic::new(
                        "invalid_tracking_selection",
                        "usage",
                        "tasc",
                        "Selection does not match an installation in this cohort.",
                        "Select one or more numbers or stable IDs shown in the onboarding list.",
                    ));
                }
            }
            if selected.is_empty() {
                return Err(Diagnostic::new(
                    "empty_tracking_selection",
                    "usage",
                    "tasc",
                    "No installations were selected.",
                    "Select one or more installations or choose not now.",
                ));
            }
            (selected, false, false)
        }
        "" | "n" | "no" | "not now" => (Vec::new(), false, true),
        _ => {
            return Err(Diagnostic::new(
                "invalid_tracking_choice",
                "usage",
                "tasc",
                "Choose all, selected, or not now.",
                "Enter a, s, or n.",
            ))
        }
    };
    let installations = service
        .complete_tracking_onboarding(&onboarding.installation_ids, &selected_ids, all, defer)
        .map_err(local_error)?;
    if defer {
        eprintln!("Tracking deferred for this installation cohort.");
    } else {
        eprintln!("Tracked {} installation(s).", installations.len());
    }
    Ok(())
}

fn confirm_plan(plan: &LifecycleResult, prompt: &str) -> Result<bool, Diagnostic> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    eprintln!("Plan: {}", safe_text(&plan.plan));
    eprint!("{prompt} [y/N] ");
    std::io::Write::flush(&mut std::io::stderr())
        .map_err(|error| local_error(format!("cannot write confirmation prompt: {error}")))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| local_error(format!("cannot read confirmation: {error}")))?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(true)
    } else {
        Err(Diagnostic::new(
            "cancelled",
            "cancelled",
            "tasc",
            "Operation cancelled.",
            "Retry when ready.",
        ))
    }
}

fn authoring_question(field: &str) -> &'static str {
    match field {
        "name" => "What concise lowercase skill name should identify this capability?",
        "purpose" => "What user goal should this skill accomplish?",
        "description" => "How should an assistant recognize what this skill does?",
        "triggers" => "When should this skill activate, and what requests should trigger it?",
        _ => "What should this skill explicitly not do?",
    }
}

fn prompt_value(value: Option<String>, prompt: &str) -> Result<String, Diagnostic> {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        return Ok(value);
    }
    if !std::io::stdin().is_terminal() {
        return Err(Diagnostic::new(
            "unresolved_product_question",
            "usage",
            "skill authoring",
            &format!("An answer is needed: {prompt}"),
            "Provide the corresponding authoring option, or run interactively to answer the question.",
        ));
    }
    eprint!("{prompt}: ");
    std::io::Write::flush(&mut std::io::stderr())
        .map_err(|error| local_error(format!("cannot write authoring prompt: {error}")))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| local_error(format!("cannot read authoring answer: {error}")))?;
    if answer.trim().is_empty() {
        return Err(Diagnostic::new(
            "unresolved_product_question",
            "usage",
            "skill authoring",
            &format!("An answer is needed: {prompt}"),
            "Answer the product question before creating the Skill Draft.",
        ));
    }
    Ok(answer.trim().to_string())
}

fn confirm_action(plan: &str, prompt: &str) -> Result<bool, Diagnostic> {
    if !std::io::stdin().is_terminal() {
        return Err(Diagnostic::new(
            "confirmation_required",
            "usage",
            "skill authoring",
            "This Workspace mutation requires explicit approval.",
            "Review the plan and retry with --yes, or run interactively to approve it.",
        ));
    }
    eprintln!("Plan: {}", safe_text(plan));
    eprint!("{prompt} [y/N] ");
    std::io::Write::flush(&mut std::io::stderr())
        .map_err(|error| local_error(format!("cannot write confirmation prompt: {error}")))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| local_error(format!("cannot read confirmation: {error}")))?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(true)
    } else {
        Err(Diagnostic::new(
            "cancelled",
            "cancelled",
            "tasc",
            "Operation cancelled.",
            "Retry when ready.",
        ))
    }
}

fn in_git_repository() -> bool {
    std::env::current_dir()
        .ok()
        .is_some_and(|directory| directory.ancestors().any(|path| path.join(".git").exists()))
}

fn render_lifecycle(result: &LifecycleResult, command: &str, json: bool) -> Result<(), Diagnostic> {
    if json {
        let envelope = Envelope::new(command, result);
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope)
                .map_err(|error| local_error(format!("cannot render lifecycle result: {error}")))?
        );
    } else {
        println!("{command} (schema version 1)");
        render_fields(
            &serde_json::to_value(result)
                .map_err(|error| local_error(format!("cannot render lifecycle result: {error}")))?,
            0,
        );
    }
    Ok(())
}

fn render_inventory(inventory: &Inventory, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(inventory)
                .map_err(|error| format!("cannot render inventory: {error}"))?
        );
    } else {
        println!("Inventory schema version: {}", inventory.schema_version);
        if let Some(profile) = &inventory.profile {
            println!("Profile: {} ({:?})", profile.id, profile.scope);
        }
        println!(
            "Inventory rows: {} of {} (offset {}, limit {})",
            inventory.items.len(),
            inventory.total,
            inventory.offset,
            inventory
                .limit
                .map_or_else(|| "all".to_string(), |limit| limit.to_string())
        );
        for (index, item) in inventory.items.iter().enumerate() {
            println!(
                "- {} [{}] {}",
                item.identity.name,
                if item.kind == "watchlist" {
                    "watched"
                } else if item.tracked {
                    "tracked"
                } else {
                    "untracked"
                },
                item.identity.id
            );
            println!("  row: {}", index + 1);
            println!("  action ID: {}", item.action_id);
            println!("  kind: {}", item.kind);
            println!("  tracked: {}", item.tracked);
            println!(
                "  lifecycle state: {}",
                item.lifecycle_state.map_or_else(
                    || "none".to_string(),
                    |state| format!("{state:?}").to_lowercase()
                )
            );
            println!("  tracking state: {}", item.tracking_state);
            println!("  watch state: {}", item.watch_state);
            println!("  scope: {}", item.scope);
            println!("  source: {}", item.identity.source);
            println!("  relative path: {}", item.identity.relative_path);
            if item.installation_id.is_empty() {
                println!("  installation: none");
            } else {
                println!("  installation: {}", item.installation_id);
            }
            if let Some(target) = &item.target {
                println!("  assistant: {}", target.assistant);
                println!("  target: {}", target.id);
                println!("  target path: {}", target.path);
            } else {
                println!("  assistant: none");
                println!("  target: none");
                println!("  target path: none");
            }
            println!("  path: {}", item.path);
            if let Some(status) = item.status {
                println!("  status: {}", format!("{status:?}").to_lowercase());
            } else {
                println!("  status: none");
            }
            if let Some(revision) = &item.source_revision {
                println!("  source revision: {revision}");
            } else {
                println!("  source revision: none");
            }
            println!(
                "  provenance: {}",
                match item.provenance.kind {
                    tasc_core::ProvenanceKind::GithubRepository => "github_repository",
                    tasc_core::ProvenanceKind::MarketplacePlugin => "marketplace_plugin",
                    tasc_core::ProvenanceKind::LocalWorkspace => "local_workspace",
                    tasc_core::ProvenanceKind::BundledFirstParty => "bundled_first_party",
                    tasc_core::ProvenanceKind::Unknown => "unknown",
                }
            );
            println!(
                "  provenance confidence: {}",
                match item.provenance.confidence {
                    tasc_core::ProvenanceConfidence::Verified => "verified",
                    tasc_core::ProvenanceConfidence::Probable => "probable",
                    tasc_core::ProvenanceConfidence::Inferred => "inferred",
                    tasc_core::ProvenanceConfidence::Unavailable => "unavailable",
                }
            );
            if let Some(evidence) = &item.provenance.raw_evidence {
                println!("  provenance evidence: {evidence}");
            } else {
                println!("  provenance evidence: none");
            }
            println!("  provenance explanation: {}", item.provenance.explanation);
            println!("  description excerpt: {}", item.description_excerpt);
            println!("  description: {}", item.description);
        }
    }
    Ok(())
}

fn render_status(status: &StateStatus, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(status)
                .map_err(|error| format!("cannot render Local State status: {error}"))?
        );
    } else {
        println!("Status schema version: {}", status.schema_version);
        println!("Local State version: {}", status.state_version);
        println!(
            "Profile: {} ({})",
            status.profile.id,
            format!("{:?}", status.profile.scope).to_lowercase()
        );
        println!("Identities: {}", status.identity_count);
        println!("Adapter targets: {}", status.target_count);
        println!("Installations: {}", status.installation_count);
        println!("Discovered skills: {}", status.discovered_count);
        println!("Untracked installations: {}", status.untracked_count);
    }
    Ok(())
}
