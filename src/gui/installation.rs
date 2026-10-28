use crate::{
    integrate::status::{InstallationCache, InstalledMod, InstalledState},
    state::State,
};
use mint_lib::mod_info::ModSpecification;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// How often the background check looks for changed local files and installations.
const CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// Rebuilds the request at most this rarely while nothing reported a change, as a safety net for
/// metadata changes that arrive without a message.
const REBUILD_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq, Eq)]
struct Request {
    game: PathBuf,
    mods: Vec<(String, Option<InstalledMod>)>,
}

/// Background thread that compares the installation with the latest request. It wakes the UI only
/// when the result changes, so an idle window does not repaint for it.
struct Worker {
    requests: mpsc::Sender<(Request, bool)>,
    results: mpsc::Receiver<(Request, Comparison)>,
    busy: Arc<AtomicBool>,
}

impl Worker {
    fn spawn(ctx: &egui::Context) -> Option<Self> {
        let (requests, request_rx) = mpsc::channel::<(Request, bool)>();
        let (result_tx, results) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        let paused = busy.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("installation-status".into())
            .spawn(move || {
                let mut cache = InstallationCache::default();
                let mut current: Option<Request> = None;
                let mut sent: Option<Comparison> = None;
                loop {
                    match request_rx.recv_timeout(CHECK_INTERVAL) {
                        Ok((mut request, mut force)) => {
                            while let Ok((newer, newer_force)) = request_rx.try_recv() {
                                request = newer;
                                force |= newer_force;
                            }
                            if force {
                                cache.clear();
                            }
                            if current.as_ref() != Some(&request) {
                                sent = None;
                            }
                            current = Some(request);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                    // Installing or editing may be writing the files being compared.
                    if paused.load(Ordering::Acquire) {
                        continue;
                    }
                    let Some(request) = &current else {
                        continue;
                    };
                    let comparison = check(&mut cache, request);
                    if sent.as_ref() != Some(&comparison) {
                        sent = Some(comparison.clone());
                        if result_tx.send((request.clone(), comparison)).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                }
            })
            .ok()?;
        Some(Self {
            requests,
            results,
            busy,
        })
    }
}

fn check(cache: &mut InstallationCache, request: &Request) -> Comparison {
    let mut desired = request.mods.clone();
    for (_, entry) in &mut desired {
        if let Some(m) = entry
            && m.read_local_stamp().is_err()
        {
            *entry = None;
        }
    }
    compare(cache.read(&request.game), &desired)
}

#[derive(Default)]
pub(super) struct InstallationStatus {
    worker: Option<Worker>,
    /// Game path and enabled mods the request was built from.
    inputs: Option<(PathBuf, Vec<ModSpecification>)>,
    built: Option<Instant>,
    /// Mod metadata or the installation may have changed since the request was built.
    stale: bool,
    requested: Option<Request>,
    pub summary: String,
    pub rows: HashMap<String, String>,
    /// The active profile is known to differ from the installed mods.
    pub needs_install: bool,
}

/// Summary, per-mod hints, and whether installing would change the game.
type Comparison = (String, HashMap<String, String>, bool);

fn compare(installed: InstalledState, desired: &[(String, Option<InstalledMod>)]) -> Comparison {
    let mut rows = HashMap::new();
    let known = match installed {
        InstalledState::NotInstalled => {
            for (spec, _) in desired {
                rows.insert(
                    spec.clone(),
                    "Not installed. Click Install mods to apply this profile.".into(),
                );
            }
            return (
                "No mods installed. Click Install mods to apply this profile.".into(),
                rows,
                !desired.is_empty(),
            );
        }
        InstalledState::Unknown(reason) => {
            return (
                format!("Installation status unknown. {reason}"),
                rows,
                false,
            );
        }
        InstalledState::Known(mods) => mods,
    };
    let mut unresolved = false;
    for (spec, entry) in desired {
        let Some(entry) = entry else {
            unresolved = true;
            continue;
        };
        match known
            .iter()
            .find(|m| *m == entry)
            .or_else(|| known.iter().find(|m| m.identity == entry.identity))
        {
            None => {
                rows.insert(
                    spec.clone(),
                    "Not installed. Click Install mods to add this mod.".into(),
                );
            }
            Some(old)
                if old.resolution != entry.resolution || old.local_stamp != entry.local_stamp =>
            {
                rows.insert(
                    spec.clone(),
                    "Selected version is not installed. Click Install mods to apply it.".into(),
                );
            }
            _ => {}
        }
    }
    let desired_mods: Vec<_> = desired.iter().filter_map(|(_, m)| m.clone()).collect();
    let differs = known != desired_mods;
    let summary = if unresolved {
        "Some mod versions are unknown. Check for updates before installing."
    } else if differs {
        "Profile changes are not installed. Click Install mods to apply them."
    } else {
        "Selected profile matches the installed mods."
    };
    // Unknown versions cannot be compared reliably, so they do not claim a pending change.
    (summary.into(), rows, differs && !unresolved)
}

impl InstallationStatus {
    /// Background work finished, so metadata or the installed mods may have changed.
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    fn reset(&mut self) {
        self.worker = None;
        self.inputs = None;
        self.built = None;
        self.requested = None;
        self.rows.clear();
        self.needs_install = false;
    }

    pub fn poll(&mut self, state: &State, ctx: &egui::Context, busy: bool) {
        let Some(game) = &state.config.drg_pak_path else {
            self.reset();
            self.summary.clear();
            return;
        };
        let specs = match state
            .mod_data
            .enabled_mods_ordered(&state.mod_data.active_profile)
        {
            Ok(specs) => specs,
            Err(error) => {
                self.reset();
                self.summary = error.to_string();
                return;
            }
        };
        if let Some(worker) = &self.worker {
            worker.busy.store(busy, Ordering::Release);
        }
        // Looking up every mod's metadata is the expensive part, so it only happens when an
        // input changed instead of on every frame.
        let inputs = (game.clone(), specs);
        if self.stale
            || self.inputs.as_ref() != Some(&inputs)
            || self.built.is_none_or(|t| t.elapsed() >= REBUILD_INTERVAL)
        {
            let force = std::mem::take(&mut self.stale);
            let request = Request {
                game: game.clone(),
                mods: inputs
                    .1
                    .iter()
                    .map(|s| {
                        (
                            s.url.clone(),
                            state
                                .store
                                .get_mod_info(s)
                                .as_ref()
                                .map(InstalledMod::from_info),
                        )
                    })
                    .collect(),
            };
            self.inputs = Some(inputs);
            self.built = Some(Instant::now());
            let changed = self.requested.as_ref() != Some(&request);
            if changed {
                self.rows.clear();
                self.summary = "Checking installed mod versions...".into();
                self.requested = Some(request.clone());
            }
            if changed || force {
                if self.worker.is_none() {
                    self.worker = Worker::spawn(ctx);
                }
                if let Some(worker) = &self.worker {
                    worker.busy.store(busy, Ordering::Release);
                    if worker.requests.send((request, force)).is_err() {
                        self.worker = None;
                    }
                }
            }
        }
        let Some(worker) = &self.worker else {
            self.summary = "Installation status could not be checked.".into();
            self.needs_install = false;
            self.requested = None;
            return;
        };
        loop {
            match worker.results.try_recv() {
                Ok((request, (summary, rows, needs_install))) => {
                    if self.requested.as_ref() == Some(&request) {
                        self.summary = summary;
                        self.rows = rows;
                        self.needs_install = needs_install;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.summary = "Installation status could not be checked.".into();
                    self.needs_install = false;
                    // The next rebuild starts a new check.
                    self.requested = None;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordering_and_changed_local_files_need_installation_but_unknown_versions_do_not_claim_success()
     {
        let first = InstalledMod {
            identity: "first".into(),
            resolution: "first".into(),
            local_stamp: Some((10, 100)),
        };
        let second = InstalledMod {
            identity: "second".into(),
            resolution: "second".into(),
            local_stamp: Some((20, 200)),
        };
        let installed = InstalledState::Known(vec![first.clone(), second.clone()]);
        let reversed = vec![
            ("second".into(), Some(second.clone())),
            ("first".into(), Some(first.clone())),
        ];
        let changed = compare(installed.clone(), &reversed);
        assert!(changed.0.contains("not installed"));
        assert!(changed.1.is_empty());
        assert!(changed.2, "reordering needs installation");
        let mut modified = first;
        modified.local_stamp = Some((11, 101));
        let changed = compare(
            installed.clone(),
            &[
                ("first".into(), Some(modified)),
                ("second".into(), Some(second)),
            ],
        );
        assert!(changed.1["first"].contains("not installed"));
        assert!(changed.2);
        let unknown = compare(installed, &[("missing".into(), None)]);
        assert!(unknown.0.contains("unknown"));
        assert!(!unknown.2, "unknown versions do not claim a pending change");
    }

    #[test]
    fn installed_versions_order_removed_mods_and_unknown_states_are_distinct() {
        let entry = InstalledMod {
            identity: "mod#1".into(),
            resolution: "mod#1/10".into(),
            local_stamp: None,
        };
        let desired = vec![("latest".into(), Some(entry.clone()))];
        assert!(
            compare(InstalledState::Known(vec![entry.clone()]), &desired)
                .0
                .contains("matches")
        );
        let mut older = entry.clone();
        older.resolution = "mod#1/9".into();
        let both = compare(
            InstalledState::Known(vec![older.clone(), entry.clone()]),
            &[
                ("pinned".into(), Some(older.clone())),
                ("latest".into(), Some(entry.clone())),
            ],
        );
        assert!(both.0.contains("matches"));
        assert!(both.1.is_empty());
        assert!(!both.2);
        let result = compare(InstalledState::Known(vec![older]), &desired);
        assert!(result.0.contains("not installed"));
        assert!(result.1["latest"].contains("not installed"));
        assert!(result.2);
        assert!(
            compare(InstalledState::Known(vec![entry]), &[])
                .0
                .contains("not installed")
        );
        assert!(
            compare(
                InstalledState::Unknown("Legacy installation".into()),
                &desired
            )
            .0
            .contains("unknown")
        );
        let nothing = compare(InstalledState::NotInstalled, &desired);
        assert!(nothing.0.contains("No mods installed"));
        assert!(nothing.2);
        assert!(!compare(InstalledState::NotInstalled, &[]).2);
        assert!(
            !compare(
                InstalledState::Unknown("Legacy installation".into()),
                &desired
            )
            .2
        );
    }
}
