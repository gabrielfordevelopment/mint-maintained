use super::{ModConfig, ModData_v0_1_0 as ModData, ModGroup, ModOrGroup, StateError};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct ListTarget {
    pub profile: String,
    pub group: Option<String>,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub enum Edit {
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
    MoveMod {
        source: ListTarget,
        destination: Option<String>,
    },
    Ungroup {
        profile: String,
        index: usize,
    },
}

pub struct PreparedEdit {
    before: ModData,
    edit: Edit,
}

impl PreparedEdit {
    pub fn new(data: &ModData, edit: Edit) -> Self {
        Self {
            before: data.clone(),
            edit,
        }
    }

    pub fn apply(self, data: &mut ModData) -> Result<(), StateError> {
        if *data != self.before {
            return Err(invalid(
                "The list changed while confirmation was open. Nothing was deleted; select the item again.",
            ));
        }
        data.apply_edit(self.edit)
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
                let group_enabled = source.group.as_ref().map(|name| {
                    self.profiles.get(&source.profile).is_some_and(|profile| profile.mods.iter().any(|e| matches!(e, ModOrGroup::Group { group_name, enabled: true } if group_name == name)))
                }).unwrap_or(true);
                let ModOrGroup::Individual(mut config) = self.remove_entry(&source)? else {
                    return Err(invalid("Only individual mods can be moved into a group."));
                };
                if let Some(name) = destination {
                    self.attach_group(&source.profile, &name)?;
                    self.groups.get_mut(&name).unwrap().mods.push(config);
                } else {
                    config.enabled &= group_enabled;
                    let entries = self.profile_entries(&source.profile)?;
                    let group_index = entries.iter().position(|e| matches!(e, ModOrGroup::Group { group_name, .. } if Some(group_name) == source.group.as_ref())).ok_or_else(|| invalid("The source group is missing."))?;
                    entries.insert(group_index + 1, ModOrGroup::Individual(config));
                }
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
                self.profile_entries(&profile)?
                    .splice(index..index, mods.into_iter().map(ModOrGroup::Individual));
            }
        }
        Ok(())
    }
}
