use super::*;

impl App {
    pub(super) fn visible_order(
        &self,
        profile: &str,
        config: Option<&SortingConfig>,
    ) -> crate::state::edits::VisibleOrder {
        let entries = self
            .state
            .mod_data
            .profiles
            .get(profile)
            .map(|p| p.mods.as_slice())
            .unwrap_or_default();
        let profile = sorted_mod_indices(
            entries.iter().map(|entry| match entry {
                ModOrGroup::Individual(config) => Some(config),
                ModOrGroup::Group { .. } => None,
            }),
            config,
            |spec| self.state.store.get_mod_info(spec),
        );
        let groups = entries
            .iter()
            .filter_map(|entry| match entry {
                ModOrGroup::Group { group_name, .. } => {
                    self.state.mod_data.groups.get(group_name).map(|group| {
                        (
                            group_name.clone(),
                            sorted_mod_indices(group.mods.iter().map(Some), config, |spec| {
                                self.state.store.get_mod_info(spec)
                            }),
                        )
                    })
                }
                _ => None,
            })
            .collect();
        crate::state::edits::VisibleOrder { profile, groups }
    }

    pub(super) fn ui_sort_controls(&mut self, ui: &mut Ui) {
        let (mut category, mut descending) = self
            .get_sorting_config()
            .map(|config| (Some(config.sort_category), config.is_ascending))
            .unwrap_or_default();
        let previous = (category, descending);
        ui.label("Sort by:");
        icons::combo_box("mod-sort-category")
            .selected_text(category.as_ref().map_or("Manual", SortBy::as_str))
            .width(110.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut category, None, "Manual");
                for option in SortBy::iter() {
                    ui.selectable_value(&mut category, Some(option), option.as_str());
                }
            });
        let label = if descending {
            "Descending"
        } else {
            "Ascending"
        };
        let arrow = Icon::ChevronRight
            .image(ui, ui.visuals().text_color())
            .rotate(
                if descending {
                    std::f32::consts::FRAC_PI_2
                } else {
                    -std::f32::consts::FRAC_PI_2
                },
                egui::Vec2::splat(0.5),
            );
        let direction = ui
            .add_enabled_ui(category.is_some(), |ui| {
                ui.add_sized(
                    [108.0, ui.spacing().interact_size.y],
                    egui::Button::image_and_text(arrow, label),
                )
            })
            .inner;
        if direction.clicked() {
            descending = !descending;
        }
        direction.on_disabled_hover_text("Manual order uses drag and drop.");
        if (category, descending) != previous {
            // The legacy is_ascending field reverses the base comparator; keep its saved meaning.
            self.update_sorting_config(category, descending);
        }
    }

    pub(super) fn get_sorting_config(&self) -> Option<SortingConfig> {
        self.state.config.sorting_config.clone()
    }

    pub(super) fn update_sorting_config(
        &mut self,
        sort_category: Option<SortBy>,
        is_ascending: bool,
    ) {
        self.state.config.sorting_config = sort_category.map(|sort_category| SortingConfig {
            sort_category,
            is_ascending,
        });
        self.report_save(self.state.config.save());
    }
}

pub(super) fn sorted_mod_indices<'a>(
    mods: impl IntoIterator<Item = Option<&'a ModConfig>>,
    config: Option<&SortingConfig>,
    get_info: impl Fn(&ModSpecification) -> Option<ModInfo>,
) -> Vec<usize> {
    let mut entries: Vec<_> = mods
        .into_iter()
        .enumerate()
        .map(|(index, mc)| {
            let info = config.and_then(|_| mc.and_then(|mc| get_info(&mc.spec)));
            (index, mc, info)
        })
        .collect();
    if let Some(config) = config {
        let compare = sort_mods(config);
        // Group entries separate independently sorted runs and retain their positions.
        for run in entries.split_mut(|(_, mc, _)| mc.is_none()) {
            run.sort_by(|(_, a, info_a), (_, b, info_b)| {
                compare((a.unwrap(), info_a.as_ref()), (b.unwrap(), info_b.as_ref()))
            });
        }
    }
    entries.into_iter().map(|(index, _, _)| index).collect()
}

type ModListEntry<'a> = (&'a ModConfig, Option<&'a ModInfo>);
fn sort_mods(config: &SortingConfig) -> impl Fn(ModListEntry, ModListEntry) -> Ordering {
    move |(mc_a, info_a), (mc_b, info_b)| {
        fn map_cmp<V, M, F>(a: &V, b: &V, map: F) -> Ordering
        where
            M: Ord,
            F: Fn(&V) -> M,
        {
            map(a).cmp(&map(b))
        }

        let name_order = map_cmp(&(mc_a, info_a), &(mc_b, info_b), |(mc, info)| {
            (info.map(|i| i.name.to_lowercase()), &mc.spec.url)
        });
        let provider_order = map_cmp(&info_a, &info_b, |info| info.map(|i| i.provider));
        let approval_order = map_cmp(&info_a, &info_b, |info| {
            info.and_then(|i| i.modio_tags.as_ref())
                .map(|t| t.approval_status)
        });
        let required_order = map_cmp(&info_a, &info_b, |info| {
            info.and_then(|i| i.modio_tags.as_ref())
                .map(|t| std::cmp::Reverse(t.required_status))
        });
        let mut order = match config.sort_category {
            SortBy::Enabled => mc_b.enabled.cmp(&mc_a.enabled),
            SortBy::Name => name_order,
            SortBy::Priority => mc_a.priority.cmp(&mc_b.priority),
            SortBy::Provider => provider_order,
            SortBy::RequiredStatus => required_order,
            SortBy::ApprovalCategory => approval_order,
        };

        if config.is_ascending {
            order = order.reverse();
        }
        if config.sort_category != SortBy::Name {
            order = order.then(name_order);
        }
        order
    }
}
