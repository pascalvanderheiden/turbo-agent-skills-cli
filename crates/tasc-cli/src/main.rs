use clap::{Args, Parser, Subcommand};
use tasc_agent::{service::FsStateRepository, AgentService, CopilotAdapter};
use tasc_core::{Inventory, StateStatus};

#[derive(Parser)]
#[command(name = "tasc", version, about = "TASC: manage local agent skills")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    /// List discovered skills and whether they are tracked in Local State.
    List(JsonOutput),
    /// Explicitly record a discovered installation without changing its files.
    Adopt { id: String },
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

fn main() {
    if let Err(error) = run() {
        eprintln!("tasc: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let state_path = FsStateRepository::default_path()?;
    let service = AgentService::new(CopilotAdapter::local(), FsStateRepository::new(state_path));
    match cli.command {
        Commands::Skill {
            command: SkillCommands::List(output),
        } => render_inventory(&service.inventory()?, output.json),
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
        } => render_status(&service.status()?, output.json),
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
