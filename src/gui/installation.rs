use crate::{
    integrate::status::{InstalledMod, InstalledState, read_installation},
    state::State,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Eq)]
struct Request {
    game: PathBuf,
    mods: Vec<(String, Option<InstalledMod>)>,
}

#[derive(Default)]
pub(super) struct InstallationStatus {
    requested: Option<Request>,
    pending: Option<mpsc::Receiver<(String, HashMap<String, String>)>>,
    last_checked: Option<Instant>,
    pub summary: String,
    pub rows: HashMap<String, String>,
}

fn compare(
    installed: InstalledState,
    desired: &[(String, Option<InstalledMod>)],
) -> (String, HashMap<String, String>) {
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
            );
        }
        InstalledState::Unknown(reason) => {
            return (format!("Installation status unknown. {reason}"), rows);
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
    let summary = if unresolved {
        "Some mod versions are unknown. Check for updates before installing."
    } else if known != desired_mods {
        "Profile changes are not installed. Click Install mods to apply them."
    } else {
        "Selected profile matches the installed mods."
    };
    (summary.into(), rows)
}

impl InstallationStatus {
    pub fn poll(&mut self, state: &State, ctx: &egui::Context, busy: bool) {
        let Some(game) = &state.config.drg_pak_path else {
            self.summary.clear();
            self.rows.clear();
            self.requested = None;
            self.pending = None;
            return;
        };
        let specs = match state
            .mod_data
            .enabled_mods_ordered(&state.mod_data.active_profile)
        {
            Ok(specs) => specs,
            Err(error) => {
                self.summary = error.to_string();
                self.rows.clear();
                self.requested = None;
                self.pending = None;
                return;
            }
        };
        let request = Request {
            game: game.clone(),
            mods: specs
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
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok((summary, rows)) => {
                    if self.requested.as_ref() == Some(&request) {
                        self.summary = summary;
                        self.rows = rows;
                    }
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.summary = "Installation status could not be checked.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let changed = self.requested.as_ref() != Some(&request);
        if changed {
            self.rows.clear();
            self.summary = "Checking installed mod versions...".into();
        }
        if !busy
            && self.pending.is_none()
            && (changed
                || self
                    .last_checked
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(2)))
        {
            self.requested = Some(request.clone());
            self.last_checked = Some(Instant::now());
            let (tx, rx) = mpsc::channel();
            self.pending = Some(rx);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut desired = request.mods;
                for (_, entry) in &mut desired {
                    if let Some(m) = entry
                        && m.read_local_stamp().is_err()
                    {
                        *entry = None;
                    }
                }
                let _ = tx.send(compare(read_installation(&request.game), &desired));
                ctx.request_repaint();
            });
        }
        ctx.request_repaint_after(Duration::from_millis(500));
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
        assert!(
            compare(installed, &[("missing".into(), None)])
                .0
                .contains("unknown")
        );
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
        let result = compare(InstalledState::Known(vec![older]), &desired);
        assert!(result.0.contains("not installed"));
        assert!(result.1["latest"].contains("not installed"));
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
        assert!(
            compare(InstalledState::NotInstalled, &desired)
                .0
                .contains("No mods installed")
        );
    }
}
