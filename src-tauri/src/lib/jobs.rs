use std::collections::HashMap;
use std::sync::Mutex;

/// Реестр версий фоновых операций по ключу (kind). Начало новой операции
pub struct JobRegistry {
    versions: Mutex<HashMap<String, u64>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            versions: Mutex::new(HashMap::new()),
        }
    }

    pub fn begin(&self, kind: &str) -> u64 {
        let mut map = self.versions.lock().expect("job registry poisoned");
        let next = map.get(kind).copied().unwrap_or(0) + 1;
        map.insert(kind.to_string(), next);
        next
    }

    pub fn cancel(&self, kind: &str) {
        self.begin(kind);
    }

    pub fn is_current(&self, kind: &str, id: u64) -> bool {
        let map = self.versions.lock().expect("job registry poisoned");
        map.get(kind).copied() == Some(id)
    }
}

#[tauri::command]
pub fn begin_job(kind: String, jobs: tauri::State<JobRegistry>) -> u64 {
    jobs.begin(&kind)
}

#[tauri::command]
pub fn cancel_job(kind: String, jobs: tauri::State<JobRegistry>) {
    jobs.cancel(&kind);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancels_previous_version() {
        let jobs = JobRegistry::new();
        let first = jobs.begin("conversion");
        assert!(jobs.is_current("conversion", first));
        let second = jobs.begin("conversion");
        assert!(!jobs.is_current("conversion", first));
        assert!(jobs.is_current("conversion", second));
    }
}