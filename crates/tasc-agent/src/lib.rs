pub mod discovery;
pub mod gh;
pub mod service;
pub mod watch;
pub use service::{AgentService, CopilotAdapter, FsStateRepository};
pub use watch::{FsWatchCheckRepository, WatchService};
