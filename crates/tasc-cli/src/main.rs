use clap::{Args, Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use tasc_agent::{
    discovery::DiscoveryService,
    gh::{Cancellation, GhSkill},
    lifecycle::{LifecycleResult, LifecycleService},
    service::FsStateRepository,
    AgentService, CopilotAdapter, FsWatchCheckRepository, WatchService,
};
use tasc_core::{
    discovery::{Diagnostic, Envelope},
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
    let repository = FsStateRepository::new(state_path);
    let service = AgentService::new(CopilotAdapter::local(), repository.clone());
    match command {
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
