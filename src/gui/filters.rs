//! Session-only filter chips for the mod list. Filtering changes only what is displayed.

use super::*;
use crate::state::edits::VisibleOrder;

/// Chips within a category combine with OR. Categories combine with AND.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct ModFilters {
    pub enabled: bool,
    pub disabled: bool,
    pub update_available: bool,
    pub missing_metadata: bool,
    pub modio: bool,
    pub web: bool,
    pub local: bool,
}

/// Facts about one displayed mod that the chips test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RowFacts {
    /// Includes a disabled parent group.
    pub enabled: bool,
    pub update_available: bool,
    pub missing_metadata: bool,
    pub provider: Option<&'static str>,
}

impl ModFilters {
    pub(super) fn is_active(&self) -> bool {
        *self != Self::default()
    }

    pub(super) fn matches(&self, facts: &RowFacts) -> bool {
        let state = !(self.enabled || self.disabled)
            || (self.enabled && facts.enabled)
            || (self.disabled && !facts.enabled);
        let status = !(self.update_available || self.missing_metadata)
            || (self.update_available && facts.update_available)
            || (self.missing_metadata && facts.missing_metadata);
        let provider = !(self.modio || self.web || self.local)
            || match facts.provider {
                Some("modio") => self.modio,
                Some("http") => self.web,
                Some("file") => self.local,
                _ => false,
            };
        state && status && provider
    }
}

/// Matching and total member counts of a displayed group.
pub(super) type GroupCounts = HashMap<String, (usize, usize)>;

impl App {
    pub(super) fn row_facts(&self, mc: &ModConfig, parent_enabled: bool) -> RowFacts {
        let info = self.state.store.get_mod_info(&mc.spec);
        let update_available = info.as_ref().is_some_and(|info| {
            info.provider == "modio"
                && self
                    .state
                    .store
                    .get_mod_info(&info.spec)
                    .is_some_and(|latest| latest.resolution.url != info.resolution.url)
        });
        RowFacts {
            enabled: parent_enabled && mc.enabled,
            update_available,
            missing_metadata: info.is_none(),
            provider: info
                .map(|info| info.provider)
                .or_else(|| ModStore::provider_id(&mc.spec.url)),
        }
    }

    /// Whether a mod matches the search text: its name (its URL without metadata) or one of its
    /// tags, ignoring case. An empty search matches everything.
    pub(super) fn search_matches(&self, mc: &ModConfig, search: &str) -> bool {
        if search.is_empty() {
            return true;
        }
        let contains = |text: &str| find_string::contains_case_insensitive(text, search);
        match self.state.store.get_mod_info(&mc.spec) {
            Some(info) => {
                contains(&info.name)
                    || info.modio_tags.as_ref().is_some_and(|tags| {
                        mod_list::modio_tags(tags)
                            .iter()
                            .any(|tag| contains(tag.label))
                    })
            }
            None => contains(&mc.spec.url),
        }
    }

    /// Whether the search would show any row of the profile.
    pub(super) fn search_has_matches(&self, profile: &str, search: &str) -> bool {
        let data = &self.state.mod_data;
        data.profiles.get(profile).is_some_and(|p| {
            p.mods.iter().any(|entry| match entry {
                ModOrGroup::Individual(mc) => self.search_matches(mc, search),
                ModOrGroup::Group { group_name, .. } => {
                    find_string::contains_case_insensitive(group_name, search)
                        || data.groups.get(group_name).is_some_and(|group| {
                            group.mods.iter().any(|mc| self.search_matches(mc, search))
                        })
                }
            })
        })
    }

    /// Removes rows that do not match the chips or the search from `order`. Groups stay when any
    /// member matches, and a group whose name matches the search keeps all members that pass
    /// the chips.
    pub(super) fn filter_order(
        &self,
        profile: &str,
        order: &VisibleOrder,
        filters: &ModFilters,
        search: &str,
    ) -> (VisibleOrder, GroupCounts) {
        let mut filtered = order.clone();
        let mut counts = GroupCounts::new();
        if !filters.is_active() && search.is_empty() {
            return (filtered, counts);
        }
        let Some(entries) = self.state.mod_data.profiles.get(profile) else {
            return (filtered, counts);
        };
        filtered
            .profile
            .retain(|&index| match &entries.mods[index] {
                ModOrGroup::Individual(mc) => {
                    filters.matches(&self.row_facts(mc, true)) && self.search_matches(mc, search)
                }
                ModOrGroup::Group {
                    group_name,
                    enabled,
                } => {
                    let Some(group) = self.state.mod_data.groups.get(group_name) else {
                        return false;
                    };
                    let named = find_string::contains_case_insensitive(group_name, search);
                    let members = filtered.groups.entry(group_name.clone()).or_default();
                    members.retain(|&member| {
                        let mc = &group.mods[member];
                        filters.matches(&self.row_facts(mc, *enabled))
                            && (named || self.search_matches(mc, search))
                    });
                    counts.insert(group_name.clone(), (members.len(), group.mods.len()));
                    !members.is_empty()
                }
            });
        (filtered, counts)
    }

    pub(super) fn ui_filter_chips(&mut self, ui: &mut Ui, profile: &str) {
        let mut facts = Vec::new();
        let data = &self.state.mod_data;
        for entry in data
            .profiles
            .get(profile)
            .map(|p| &p.mods[..])
            .unwrap_or_default()
        {
            match entry {
                ModOrGroup::Individual(mc) => facts.push(self.row_facts(mc, true)),
                ModOrGroup::Group {
                    group_name,
                    enabled,
                } => {
                    for mc in data
                        .groups
                        .get(group_name)
                        .map(|g| &g.mods[..])
                        .unwrap_or_default()
                    {
                        facts.push(self.row_facts(mc, *enabled));
                    }
                }
            }
        }
        let count = |test: &dyn Fn(&RowFacts) -> bool| facts.iter().filter(|f| test(f)).count();
        let counts = [
            count(&|f| f.enabled),
            count(&|f| !f.enabled),
            count(&|f| f.update_available),
            count(&|f| f.missing_metadata),
            count(&|f| f.provider == Some("modio")),
            count(&|f| f.provider == Some("http")),
            count(&|f| f.provider == Some("file")),
        ];
        let filters = &mut self.mod_filters;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label("Filter:");
            let chips: [(&mut bool, &str, &str); 7] = [
                (
                    &mut filters.enabled,
                    "Enabled",
                    "Mods that will be installed",
                ),
                (
                    &mut filters.disabled,
                    "Disabled",
                    "Mods turned off directly or by their group",
                ),
                (
                    &mut filters.update_available,
                    "Update available",
                    "Pinned mod.io mods with a newer cached version",
                ),
                (
                    &mut filters.missing_metadata,
                    "Missing metadata",
                    "Mods whose details are not cached yet",
                ),
                (&mut filters.modio, "mod.io", "Mods from mod.io"),
                (&mut filters.web, "Web", "Mods downloaded from a web URL"),
                (&mut filters.local, "Local", "Local mod files"),
            ];
            for (index, (value, label, hint)) in chips.into_iter().enumerate() {
                if index == 2 || index == 4 {
                    ui.separator();
                }
                if chip(ui, *value, &format!("{label} ({})", counts[index]))
                    .on_hover_text(hint)
                    .clicked()
                {
                    *value = !*value;
                }
            }
            if filters.is_active() && ui.button("Clear").clicked() {
                *filters = ModFilters::default();
            }
        });
    }
}

/// A rounded toggle button used for filter chips and the view switch.
pub(super) fn chip(ui: &mut Ui, selected: bool, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .selected(selected)
            .corner_radius(9.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(enabled: bool, provider: Option<&'static str>) -> RowFacts {
        RowFacts {
            enabled,
            update_available: false,
            missing_metadata: provider.is_none(),
            provider,
        }
    }

    #[test]
    fn chips_or_within_category_and_and_across_categories() {
        let none = ModFilters::default();
        assert!(!none.is_active());
        assert!(none.matches(&facts(false, None)));

        let state = ModFilters {
            enabled: true,
            disabled: true,
            ..Default::default()
        };
        assert!(state.matches(&facts(true, None)) && state.matches(&facts(false, None)));

        let enabled_local = ModFilters {
            enabled: true,
            local: true,
            ..Default::default()
        };
        assert!(enabled_local.matches(&facts(true, Some("file"))));
        assert!(!enabled_local.matches(&facts(false, Some("file"))));
        assert!(!enabled_local.matches(&facts(true, Some("modio"))));
        assert!(!enabled_local.matches(&facts(true, None)));

        let status = ModFilters {
            update_available: true,
            missing_metadata: true,
            ..Default::default()
        };
        assert!(status.matches(&facts(true, None)));
        assert!(!status.matches(&facts(true, Some("http"))));
        let mut update = facts(true, Some("modio"));
        update.update_available = true;
        assert!(status.matches(&update));
    }
}
