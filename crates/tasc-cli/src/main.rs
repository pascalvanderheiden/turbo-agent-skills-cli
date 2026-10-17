use clap::{Parser, Subcommand};
use tasc_agent::service::AgentService;
use tasc_core::TascState;

#[derive(Parser)]
#[command(name = "tasc", version, about = "TASC: manage, evaluate, and improve agent skills")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the currently tracked skill sources and installed skills.
    Status,
    /// Add a new skill source to the local watchlist.
    AddSource { name: String, url: String },
    /// Show a concise list of insights for your current skill environment.
    Insights,
    /// Trigger a local evaluation pass against the tracked skills.
    Evaluate,
}

fn main() {
    let cli = Cli::parse();
    let service = AgentService::new(TascState::default());

    match cli.command {
        Commands::Status => {
            let state = service.state();
            println!("Sources: {}", state.sources.len());
            println!("Skills: {}", state.skills.len());
            for source in &state.sources {
                println!("- {} [{}] {}", source.name, source.kind, source.url);
            }
        }
        Commands::AddSource { name, url } => {
            let source = tasc_core::create_skill_source(&name, &url, tasc_core::SkillSourceKind::GitHubRepository);
            println!("Added source '{}' ({})", source.name, source.url);
        }
        Commands::Insights => {
            for insight in tasc_core::default_insights() {
                println!("- {} [{}] {}", insight.title, insight.priority, insight.summary);
            }
        }
        Commands::Evaluate => {
            println!("Evaluation pass is ready to run with the shared TASC core.");
            println!("- tracked sources: {}", service.state().sources.len());
            println!("- tracked skills: {}", service.state().skills.len());
        }
    }
}
