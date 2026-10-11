use super::{ModSpecification, ModStore, ProviderError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Debug, Clone)]
pub struct RefreshFailure {
    pub spec: ModSpecification,
    pub name: String,
    pub reason: &'static str,
    pub details: String,
}

#[derive(Debug, Default, Clone)]
pub struct RefreshReport {
    pub checked: usize,
    pub total: usize,
    pub failures: Vec<RefreshFailure>,
    pub remaining: Vec<ModSpecification>,
    pub save_error: Option<String>,
    pub cancelled: bool,
    pub new_versions: Vec<String>,
}

impl RefreshReport {
    pub fn summary(&self) -> String {
        if self.cancelled {
            format!(
                "Update check cancelled. Checked {} of {} mods.",
                self.checked, self.total
            )
        } else if self.save_error.is_some() {
            "Update check finished, but the cache could not be saved.".into()
        } else if self.failures.is_empty() && !self.new_versions.is_empty() {
            format!(
                "Found new versions for {} mods. Install mods applies those set to latest.",
                self.new_versions.len()
            )
        } else if self.failures.is_empty() {
            format!("Checked {} mods. No files were installed.", self.checked)
        } else {
            format!(
                "{} of {} mods could not be checked. Existing cached data was kept.",
                self.failures.len(),
                self.total
            )
        }
    }

    pub fn details(&self) -> String {
        let mut lines = vec![self.summary()];
        if !self.new_versions.is_empty() {
            lines.push(format!(
                "New versions found:\n{}",
                self.new_versions.join("\n")
            ));
        }
        for failure in &self.failures {
            lines.push(format!(
                "{}: {}\n{}",
                failure.name, failure.reason, failure.details
            ));
        }
        if let Some(error) = &self.save_error {
            lines.push(error.clone());
        }
        lines.join("\n\n")
    }

    pub fn retry_specs(&self) -> Vec<ModSpecification> {
        self.failures
            .iter()
            .map(|f| f.spec.clone())
            .chain(self.remaining.iter().cloned())
            .collect()
    }
}

fn failure_reason(error: &ProviderError) -> &'static str {
    if matches!(error, ProviderError::NoProvider { .. }) {
        return "Configure the mod provider in Settings before checking for updates.";
    }
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = source {
        if error
            .downcast_ref::<modio::Error>()
            .is_some_and(|e| e.is_ratelimited())
        {
            return "mod.io temporarily limited requests. Try again after the wait.";
        }
        let status = error
            .downcast_ref::<modio::Error>()
            .and_then(|e| e.status())
            .or_else(|| {
                error
                    .downcast_ref::<reqwest::Error>()
                    .and_then(|e| e.status())
            });
        match status.map(|s| s.as_u16()) {
            Some(429) => return "mod.io temporarily limited requests. Try again after the wait.",
            Some(401 | 403) => return "Access denied. Check the provider token or mod permissions.",
            Some(404) => return "This mod could not be found.",
            Some(500..=599) => return "The mod service is temporarily unavailable.",
            _ => {}
        }
        source = error.source();
    }
    "Could not check this mod. See technical details."
}

impl ModStore {
    pub fn cached_mod_specs(&self) -> Vec<ModSpecification> {
        self.providers
            .read()
            .unwrap()
            .values()
            .flat_map(|p| p.cached_specs(self.cache.clone()))
            .collect()
    }

    pub fn save_cache(&self) -> Result<(), ProviderError> {
        self.cache
            .read()
            .unwrap()
            .save()
            .map_err(|error| ProviderError::PartialUpdate {
                failures: format!("Updated metadata could not be saved: {error}"),
            })
    }

    pub fn refresh_specs(&self, specs: Vec<ModSpecification>) -> Vec<ModSpecification> {
        let mut seen = std::collections::HashSet::new();
        specs
            .into_iter()
            .map(|s| self.get_mod_info(&s).map(|m| m.spec).unwrap_or(s))
            .filter(|s| seen.insert(s.clone()))
            .collect()
    }

    pub async fn refresh_mods(
        &self,
        specs: Vec<ModSpecification>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicUsize>,
    ) -> RefreshReport {
        let specs = self.refresh_specs(specs);
        let mut report = RefreshReport {
            total: specs.len(),
            ..Default::default()
        };
        for (index, spec) in specs.iter().enumerate() {
            let previous = self.get_mod_info(spec).map(|info| info.resolution);
            let task = async {
                self.get_provider(&spec.url)?
                    .refresh_mod(spec, self.cache.clone())
                    .await
            };
            let result = tokio::select! {
                biased;
                _ = async { while !cancelled.load(Ordering::Acquire) { tokio::time::sleep(std::time::Duration::from_millis(25)).await; } } => {
                    report.cancelled = true;
                    report.remaining = specs[index..].to_vec();
                    break;
                }
                result = task => result,
            };
            report.checked += 1;
            progress.store(report.checked, Ordering::Release);
            if result.is_ok()
                && let Some(info) = self.get_mod_info(spec)
                && previous.is_some_and(|old| old != info.resolution)
            {
                report.new_versions.push(info.name);
            }
            if let Err(error) = result {
                report.failures.push(RefreshFailure {
                    name: self
                        .get_mod_info(spec)
                        .map(|m| m.name)
                        .unwrap_or_else(|| spec.url.clone()),
                    spec: spec.clone(),
                    reason: failure_reason(&error),
                    details: format!("{}\n{error}", spec.url),
                });
            }
        }
        report.save_error = self.save_cache().err().map(|e| e.to_string());
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_checks_preserve_remaining_work_and_cache_save_failures_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModStore::new(dir.path(), &Default::default()).unwrap();
        let specs = vec![ModSpecification::new("invalid://example".into())];
        let report = store
            .refresh_mods(
                specs.clone(),
                Arc::new(AtomicBool::new(true)),
                Default::default(),
            )
            .await;
        assert!(report.cancelled);
        assert_eq!(report.checked, 0);
        assert_eq!(report.retry_specs(), specs);
        let cache_path = dir.path().join("cache.json");
        let original = std::fs::read(&cache_path).unwrap();
        std::fs::remove_file(&cache_path).unwrap();
        std::fs::create_dir(&cache_path).unwrap();
        let failed = store
            .refresh_mods(vec![], Default::default(), Default::default())
            .await;
        assert!(failed.save_error.is_some());
        std::fs::remove_dir(&cache_path).unwrap();
        std::fs::write(&cache_path, &original).unwrap();
        assert!(
            store
                .refresh_mods(failed.retry_specs(), Default::default(), Default::default())
                .await
                .save_error
                .is_none()
        );
        assert_eq!(std::fs::read(cache_path).unwrap(), original);
    }
}
