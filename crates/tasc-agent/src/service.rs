use tasc_core::{default_insights, TascState};

pub struct AgentService {
    state: TascState,
}

impl AgentService {
    pub fn new(state: TascState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &TascState {
        &self.state
    }

    pub fn insights(&self) -> Vec<String> {
        default_insights()
            .into_iter()
            .map(|insight| format!("{}: {}", insight.title, insight.summary))
            .collect()
    }
}
