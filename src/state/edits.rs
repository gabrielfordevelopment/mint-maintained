use super::{GroupColor, ModConfig, ModData_v0_1_0 as ModData, ModGroup, ModOrGroup, StateError};

#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTarget {
    pub profile: String,
    pub group: Option<String>,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub enum Edit {
    ArrangeProfile(String),
    DeleteEntry(ListTarget),
    DeleteProfile(String),
    CreateGroup {
        profile: String,
        name: String,
    },
    AttachGroup {
        profile: String,
        name: String,
    },
    RenameGroup {
        name: String,
        replacement: String,
    },
    DeleteGroup(String),
    SetGroupColor {
        name: String,
        color: GroupColor,
    },
    MoveMod {
        source: ListTarget,
        destination: Option<String>,
    },
    MoveEntry {
        source: ListTarget,
        destination: ListTarget,
    },
    Ungroup {
        profile: String,
        index: usize,
    },
}

impl Edit {
    pub fn ordered_profile(&self) -> Option<&str> {
        match self {
            Self::CreateGroup { profile, .. }
            | Self::AttachGroup { profile, .. }
            | Self::Ungroup { profile, .. } => Some(profile),
            Self::MoveMod { source, .. } | Self::MoveEntry { source, .. } => Some(&source.profile),
            _ => None,
        }
    }
}

pub struct PreparedEdit {
    before: ModData,
    edit: Edit,
    visible_order: Option<VisibleOrder>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleOrder {
    pub profile: Vec<usize>,
    pub groups: std::collections::BTreeMap<String, Vec<usize>>,
}

impl VisibleOrder {
    fn apply(&self, data: &mut ModData, profile: &str) -> Result<(), StateError> {
        fn reorder<T: Clone>(items: &mut Vec<T>, order: &[usize]) -> Result<(), StateError> {
            let mut seen = vec![false; items.len()];
            if order.len() != items.len()
                || order.iter().any(|&i| {
                    seen.get_mut(i)
                        .is_none_or(|seen| std::mem::replace(seen, true))
                })
            {
                return Err(invalid("The displayed order changed. Try dragging again."));
            }
            *items = order.iter().map(|&i| items[i].clone()).collect();
            Ok(())
        }
        reorder(data.profile_entries(profile)?, &self.profile)?;
        for (name, order) in &self.groups {
            if !data.group_users(name).iter().any(|user| user == profile) {
                return Err(invalid("The displayed group is no longer in this profile."));
            }
            let group = data
                .groups
                .get_mut(name)
                .ok_or_else(|| invalid("The group no longer exists."))?;
            reorder(&mut group.mods, order)?;
        }
        Ok(())
    }
}

impl PreparedEdit {
    pub fn action(&self) -> &Edit {
        &self.edit
    }

    pub fn new(data: &ModData, edit: Edit) -> Self {
        Self {
            before: data.clone(),
            edit,
            visible_order: None,
        }
    }

    pub fn move_in_visible_order(
        data: &ModData,
        source: ListTarget,
        destination: ListTarget,
        order: VisibleOrder,
    ) -> Self {
        Self {
            before: data.clone(),
            edit: Edit::MoveEntry {
                source,
                destination,
            },
            visible_order: Some(order),
        }
    }

    pub fn apply(self, data: &mut ModData) -> Result<bool, StateError> {
        if *data != self.before {
            return Err(invalid(
                "The list changed during this operation. Nothing was changed; select the item again.",
            ));
        }
        if let Some(order) = self.visible_order {
            let Edit::MoveEntry {
                mut source,
                destination,
            } = self.edit
            else {
                return Err(invalid("Only drag moves can apply a displayed order."));
            };
            let indices = match &source.group {
                Some(name) => order
                    .groups
                    .get(name)
                    .ok_or_else(|| invalid("The source group is no longer displayed."))?,
                None => &order.profile,
            };
            source.index = indices
                .iter()
                .position(|&i| i == source.index)
                .ok_or_else(|| invalid("The source entry is no longer displayed."))?;
            let mut candidate = data.clone();
            order.apply(&mut candidate, &source.profile)?;
            let displayed = candidate.clone();
            candidate.apply_edit(Edit::MoveEntry {
                source,
                destination,
            })?;
            if candidate != displayed {
                *data = candidate;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            data.apply_edit(self.edit)?;
            Ok(*data != self.before)
        }
    }
}

fn invalid(message: &str) -> StateError {
    StateError::InvalidEdit {
        message: message.into(),
    }
}

impl ModData {
    pub fn group_users(&self, name: &str) -> Vec<String> {
        self.profiles.iter().filter(|(_, profile)| {
            profile.mods.iter().any(|entry| matches!(entry, ModOrGroup::Group { group_name, .. } if group_name == name))
        }).map(|(name, _)| name.clone()).collect()
    }

    pub fn apply_edit(&mut self, edit: Edit) -> Result<(), StateError> {
        if let Some(profile) = edit.ordered_profile()
            && self
                .profiles
                .get(profile)
                .is_some_and(|p| p.has_mixed_order())
        {
            return Err(invalid(
                "Arrange this profile's groups before moving entries or changing groups.",
            ));
        }
        let mut next = self.clone();
        next.validate()?;
        next.edit_inner(edit)?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    fn profile_entries(&mut self, profile: &str) -> Result<&mut Vec<ModOrGroup>, StateError> {
        self.profiles
            .get_mut(profile)
            .map(|p| &mut p.mods)
            .ok_or_else(|| invalid("The profile no longer exists."))
    }

    fn remove_entry(&mut self, target: &ListTarget) -> Result<ModOrGroup, StateError> {
        let entries = self.profile_entries(&target.profile)?;
        if let Some(name) = &target.group {
            if !entries
                .iter()
                .any(|e| matches!(e, ModOrGroup::Group { group_name, .. } if group_name == name))
            {
                return Err(invalid("This group is no longer attached to the profile."));
            }
            let mods = &mut self
                .groups
                .get_mut(name)
                .ok_or_else(|| invalid("The group no longer exists."))?
                .mods;
            if target.index >= mods.len() {
                return Err(invalid("The mod no longer exists."));
            }
            Ok(ModOrGroup::Individual(mods.remove(target.index)))
        } else {
            if target.index >= entries.len() {
                return Err(invalid("The list item no longer exists."));
            }
            Ok(entries.remove(target.index))
        }
    }

    fn attach_group(&mut self, profile: &str, name: &str) -> Result<(), StateError> {
        if !self.groups.contains_key(name) {
            return Err(invalid("The group no longer exists."));
        }
        let entries = self.profile_entries(profile)?;
        if !entries
            .iter()
            .any(|e| matches!(e, ModOrGroup::Group { group_name, .. } if group_name == name))
        {
            entries.push(ModOrGroup::Group {
                group_name: name.into(),
                enabled: true,
            });
        }
        Ok(())
    }

    fn available_group_name(&self, name: &str) -> Result<(), StateError> {
        if name.trim().is_empty() || self.groups.contains_key(name) {
            Err(invalid("Choose a non-empty, unique group name."))
        } else {
            Ok(())
        }
    }

    fn edit_inner(&mut self, edit: Edit) -> Result<(), StateError> {
        match edit {
            Edit::ArrangeProfile(profile) => {
                self.profile_entries(&profile)?
                    .sort_by_key(|entry| matches!(entry, ModOrGroup::Group { .. }));
            }
            Edit::SetGroupColor { name, color } => {
                self.groups
                    .get_mut(&name)
                    .ok_or_else(|| invalid("The group no longer exists."))?
                    .color = color;
            }
            Edit::MoveEntry {
                source,
                mut destination,
            } => {
                if source.profile != destination.profile {
                    return Err(invalid("Move entries within the same profile."));
                }
                let group_enabled = source.group.as_ref().map(|name| {
                    self.profiles.get(&source.profile).is_some_and(|profile| profile.mods.iter().any(|entry| matches!(entry, ModOrGroup::Group { group_name, enabled: true } if group_name == name)))
                }).unwrap_or(true);
                if source.group == destination.group && source.index < destination.index {
                    destination.index = destination.index.checked_sub(1).unwrap();
                }
                let mut entry = self.remove_entry(&source)?;
                if let Some(name) = &destination.group {
                    let ModOrGroup::Individual(config) = entry else {
                        return Err(invalid("Groups cannot be nested."));
                    };
                    self.attach_group(&destination.profile, name)?;
                    let mods = &mut self.groups.get_mut(name).unwrap().mods;
                    if destination.index > mods.len() {
                        return Err(invalid("The destination changed."));
                    }
                    mods.insert(destination.index, config);
                } else {
                    if let ModOrGroup::Individual(config) = &mut entry {
                        config.enabled &= group_enabled;
                    }
                    let profile = self
                        .profiles
                        .get_mut(&destination.profile)
                        .ok_or_else(|| invalid("The profile no longer exists."))?;
                    let index = profile.insertion_index(
                        destination.index,
                        matches!(entry, ModOrGroup::Group { .. }),
                    );
                    let entries = &mut profile.mods;
                    if destination.index > entries.len() {
                        return Err(invalid("The destination changed."));
                    }
                    entries.insert(index, entry);
                }
            }
            Edit::DeleteEntry(target) => {
                self.remove_entry(&target)?;
            }
            Edit::DeleteProfile(name) => {
                if self.profiles.len() <= 1 {
                    return Err(invalid("The last profile cannot be deleted."));
                }
                if self.profiles.remove(&name).is_none() {
                    return Err(invalid("The profile no longer exists."));
                }
                if self.active_profile == name {
                    self.active_profile = self.profiles.keys().next().unwrap().clone();
                }
            }
            Edit::CreateGroup { profile, name } => {
                let name = name.trim().to_owned();
                self.available_group_name(&name)?;
                self.groups.insert(name.clone(), ModGroup::default());
                self.attach_group(&profile, &name)?;
            }
            Edit::AttachGroup { profile, name } => self.attach_group(&profile, &name)?,
            Edit::RenameGroup { name, replacement } => {
                let replacement = replacement.trim().to_owned();
                if replacement == name {
                    return Ok(());
                }
                self.available_group_name(&replacement)?;
                let group = self
                    .groups
                    .remove(&name)
                    .ok_or_else(|| invalid("The group no longer exists."))?;
                self.groups.insert(replacement.clone(), group);
                for profile in self.profiles.values_mut() {
                    for entry in &mut profile.mods {
                        if let ModOrGroup::Group { group_name, .. } = entry
                            && *group_name == name
                        {
                            *group_name = replacement.clone();
                        }
                    }
                }
            }
            Edit::DeleteGroup(name) => {
                if self.groups.remove(&name).is_none() {
                    return Err(invalid("The group no longer exists."));
                }
                for profile in self.profiles.values_mut() {
                    profile.mods.retain(|e| !matches!(e, ModOrGroup::Group { group_name, .. } if *group_name == name));
                }
            }
            Edit::MoveMod {
                source,
                destination,
            } => {
                if source.group == destination {
                    return Ok(());
                }
                let index = if let Some(name) = &destination {
                    self.groups
                        .get(name)
                        .ok_or_else(|| invalid("The group no longer exists."))?
                        .mods
                        .len()
                } else {
                    self.profiles
                        .get(&source.profile)
                        .ok_or_else(|| invalid("The profile no longer exists."))?
                        .group_start()
                };
                let destination = ListTarget {
                    profile: source.profile.clone(),
                    group: destination,
                    index,
                };
                self.edit_inner(Edit::MoveEntry {
                    source,
                    destination,
                })?;
            }
            Edit::Ungroup { profile, index } => {
                let target = ListTarget {
                    profile: profile.clone(),
                    group: None,
                    index,
                };
                let ModOrGroup::Group {
                    group_name,
                    enabled,
                } = self.remove_entry(&target)?
                else {
                    return Err(invalid("Select a group to ungroup."));
                };
                let mods: Vec<ModConfig> = self.groups[&group_name]
                    .mods
                    .iter()
                    .cloned()
                    .map(|mut mc| {
                        mc.enabled &= enabled;
                        mc
                    })
                    .collect();
                let profile = self.profiles.get_mut(&profile).unwrap();
                let index = profile.group_start();
                profile
                    .mods
                    .splice(index..index, mods.into_iter().map(ModOrGroup::Individual));
            }
        }
        Ok(())
    }
}
