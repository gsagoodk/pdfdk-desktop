// Job records shown in the app's Activity view

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub input_file: String,
    pub output_file: Option<String>,
    pub status: JobStatus,
    pub error: Option<String>,
    pub created_at: u64,
    pub completed_at: Option<u64>,
    /// 0.4.6: the ToolConfig key the job ran with ("widget:<id>" or a folder key) so Retry
    /// reruns with that holder's CURRENT options instead of the tool card's.
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Processing,
    Completed,
    Failed,
}

fn now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Job {
    pub fn new(tool_id: &str, tool_name: &str, input_file: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tool_id: tool_id.to_string(),
            tool_name: tool_name.to_string(),
            input_file: input_file.to_string(),
            output_file: None,
            status: JobStatus::Queued,
            error: None,
            created_at: now(),
            completed_at: None,
            key: None,
        }
    }

    pub fn with_key(mut self, key: &str) -> Self {
        self.key = Some(key.to_string());
        self
    }

    pub fn set_processing(&mut self) {
        self.status = JobStatus::Processing;
    }

    pub fn set_completed(&mut self, output_file: &str) {
        self.status = JobStatus::Completed;
        self.output_file = Some(output_file.to_string());
        self.completed_at = Some(now());
    }

    pub fn set_failed(&mut self, error: &str) {
        self.status = JobStatus::Failed;
        self.error = Some(error.to_string());
        self.completed_at = Some(now());
    }
}

/// A copy of one job, for retrying it.
pub fn job_get(id: &str) -> Option<Job> {
    crate::job_find(id)
}
