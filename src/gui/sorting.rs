use super::*;

impl App {
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
