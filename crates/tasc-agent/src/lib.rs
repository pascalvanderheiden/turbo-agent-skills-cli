pub mod discovery;
pub mod evaluation;
pub mod gh;
pub mod lifecycle;
pub mod service;
pub mod watch;
pub use evaluation::{EvaluationService, FsRunArtifactRepository, RunArtifactRepository};
pub use service::{AgentService, CopilotAdapter, FsStateRepository};
pub use watch::{FsWatchCheckRepository, WatchService};
