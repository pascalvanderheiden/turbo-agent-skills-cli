use clap::{Args, Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use tasc_agent::{
    consistency::{ConsistencyService, FsConsistencyArtifactRepository},
    discovery::DiscoveryService,
    evaluation::{EvaluationService, FsRunArtifactRepository},
    gh::{Cancellation, GhSkill},
    insights::{FsInsightArtifactRepository, FsInsightLifecycleRepository, InsightService},
    lifecycle::{LifecycleResult, LifecycleService},
    service::FsStateRepository,
    AgentService, CopilotAdapter, FsWatchCheckRepository, WatchService,
};
use tasc_core::{
    discovery::{Diagnostic, Envelope},
    insights::InsightStatus,
    Inventory, SkillIdentity, StateStatus,
};

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
    /// Record and inspect source metadata without installing skills.
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Search public GitHub skills.
    #[command(visible_alias = "discover")]
    Search(SearchArgs),
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
    /// Search public GitHub skills with deterministic ranking.
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
    List(JsonOutput),
    /// Explicitly record a discovered installation without changing its files.
    Adopt { id: String },
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
            Commands::Skill {
                command: SkillCommands::Preview { output, .. },
            } => output.json,
            Commands::Skill {
                command:
                    SkillCommands::Install { output, .. }
                    | SkillCommands::Update { output, .. }
                    | SkillCommands::Remove { output, .. },
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
            Commands::Insight { command } => match command {
                InsightCommands::Generate(output)
                | InsightCommands::Show { output, .. }
                | InsightCommands::Status { output, .. }
                | InsightCommands::Suppress { output, .. }
                | InsightCommands::Unsuppress { output, .. }
                | InsightCommands::Propose { output, .. } => output.json,
                InsightCommands::List { output, .. } => output.json,
            },
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
        Commands::Search(_)
        | Commands::Skill {
            command: SkillCommands::Search(_),
        } => "skill search",
        Commands::Skill {
            command: SkillCommands::Preview { .. },
        } => "skill preview",
        Commands::Skill {
            command: SkillCommands::Install { .. },
        } => "skill install",
        Commands::Skill {
            command: SkillCommands::Update { .. },
        } => "skill update",
        Commands::Skill {
            command: SkillCommands::Remove { .. },
        } => "skill remove",
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
    let state_path = FsStateRepository::default_path().map_err(local_error)?;
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
        Commands::Skill {
            command: SkillCommands::List(output),
        } => render_inventory(
            &service.inventory().map_err(local_error)?,
            output.json || json,
        )
        .map_err(local_error),
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
        println!("Discovered skills: {}", inventory.items.len());
        for item in &inventory.items {
            println!(
                "- {} [{}] {}",
                item.identity.name,
                if item.tracked { "tracked" } else { "untracked" },
                item.identity.id
            );
            println!("  source: {}", item.identity.source);
            println!("  relative path: {}", item.identity.relative_path);
            println!("  installation: {}", item.installation_id);
            println!("  assistant: {}", item.target.assistant);
            println!("  target: {}", item.target.id);
            println!("  target path: {}", item.target.path);
            println!("  path: {}", item.path);
            if let Some(status) = item.status {
                println!("  status: {status:?}");
            }
            if let Some(revision) = &item.source_revision {
                println!("  source revision: {revision}");
            }
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
        println!("Identities: {}", status.identity_count);
        println!("Adapter targets: {}", status.target_count);
        println!("Installations: {}", status.installation_count);
        println!("Discovered skills: {}", status.discovered_count);
        println!("Untracked installations: {}", status.untracked_count);
    }
    Ok(())
}
