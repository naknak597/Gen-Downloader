use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tauri_plugin_shell::process::CommandChild;

/// Thread-safe application state for tracking running download processes and cancellations
#[derive(Default, Clone)]
pub struct DownloadManager {
    pub active_tasks: Arc<Mutex<HashMap<String, CommandChild>>>,
    pub cancelled_tasks: Arc<Mutex<HashSet<String>>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a spawned child process handle keyed by task_id
    pub fn insert_task(&self, task_id: String, child: CommandChild) -> Result<(), String> {
        let mut tasks = self
            .active_tasks
            .lock()
            .map_err(|e| format!("Active tasks mutex poisoned: {e}"))?;
        tasks.insert(task_id, child);
        Ok(())
    }

    /// Removes and returns the child process handle for the given task_id
    pub fn remove_task(&self, task_id: &str) -> Result<Option<CommandChild>, String> {
        let mut tasks = self
            .active_tasks
            .lock()
            .map_err(|e| format!("Active tasks mutex poisoned: {e}"))?;
        Ok(tasks.remove(task_id))
    }

    /// Marks a task as cancelled in the cancelled_tasks set
    pub fn mark_cancelled(&self, task_id: &str) -> Result<(), String> {
        let mut cancelled = self
            .cancelled_tasks
            .lock()
            .map_err(|e| format!("Cancelled tasks mutex poisoned: {e}"))?;
        cancelled.insert(task_id.to_string());
        Ok(())
    }

    /// Checks if a task has been flagged as cancelled
    pub fn is_cancelled(&self, task_id: &str) -> bool {
        self.cancelled_tasks
            .lock()
            .map(|set| set.contains(task_id))
            .unwrap_or(false)
    }

    /// Removes a task from the cancelled set and returns whether it was present
    pub fn clear_cancelled(&self, task_id: &str) -> bool {
        self.cancelled_tasks
            .lock()
            .map(|mut set| set.remove(task_id))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_manager_cancellation_tracking() {
        let dm = DownloadManager::default();
        assert!(!dm.is_cancelled("task_test"));

        // Mark task as cancelled
        assert!(dm.mark_cancelled("task_test").is_ok());
        assert!(dm.is_cancelled("task_test"));

        // Clean up task cancellation
        assert!(dm.clear_cancelled("task_test"));
        assert!(!dm.is_cancelled("task_test"));
    }
}
