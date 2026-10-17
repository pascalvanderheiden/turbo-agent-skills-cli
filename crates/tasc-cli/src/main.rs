use clap::{Args, Parser, Subcommand, ValueEnum};
use tasc_agent::{
    discovery::DiscoveryService,
    gh::{Cancellation, GhSkill},
    service::FsStateRepository,
    AgentService, CopilotAdapter,
};
use tasc_core::{
    discovery::{Diagnostic, Envelope},
    Inventory, StateStatus,
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
    /// List discovered skills and whether they are tracked in Local State.
    List(JsonOutput),
    /// Explicitly record a discovered installation without changing its files.
    Adopt { id: String },
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
        Commands::Search(_)
        | Commands::Skill {
            command: SkillCommands::Search(_),
        } => "skill search",
        Commands::Skill {
            command: SkillCommands::Preview { .. },
        } => "skill preview",
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
                command: SkillCommands::Search(_) | SkillCommands::Preview { .. }
            }
            | Commands::Source {
                command: SourceCommands::Capabilities(_)
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
        command => {
            return run_local(command, state_path, cli.format == OutputFormat::Json)
                .map_err(local_error);
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

fn run_local(command: Commands, state_path: std::path::PathBuf, json: bool) -> Result<(), String> {
    let service = AgentService::new(CopilotAdapter::local(), FsStateRepository::new(state_path));
    match command {
        Commands::Skill {
            command: SkillCommands::List(output),
        } => render_inventory(&service.inventory()?, output.json || json),
        Commands::Skill {
            command: SkillCommands::Adopt { id },
        } => {
            let installation = service.adopt(&id)?;
            println!(
                "Adopted {} at {} ({})",
                installation.skill_id, installation.path, installation.target_id
            );
            Ok(())
        }
        Commands::State {
            command: StateCommands::Status(output),
        } => render_status(&service.status()?, output.json || json),
        _ => unreachable!("discovery commands are dispatched by the shared discovery service"),
    }
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
