use super::*;
use crate::state::edits::{Edit, ListTarget};

impl App {
    pub(super) fn ui_profile(&mut self, ui: &mut Ui, profile: &str) {
        if self.editing_busy() || self.delete_confirmation.is_some() {
            ui.disable();
        }
        let sorting_config = self.get_sorting_config();
        let profile_name = profile.to_owned();
        let group_names: Vec<_> = self.state.mod_data.groups.keys().cloned().collect();

        let ModData {
            profiles, groups, ..
        } = self.state.mod_data.deref_mut().deref_mut();

        struct Ctx {
            needs_save: bool,
            scroll_to_match: bool,
            btn_remove: Option<(Option<String>, usize)>,
            add_deps: Option<Vec<ModSpecification>>,
            edit: Option<Edit>,
            open_group: Option<String>,
        }
        let mut ctx = Ctx {
            needs_save: false,
            scroll_to_match: self.scroll_to_match,
            btn_remove: None,
            add_deps: None,
            edit: None,
            open_group: None,
        };

        let mut ui_profile = |ui: &mut Ui, profile: &mut ModProfile| {
            let enabled_specs = profile
                .mods
                .iter()
                .enumerate()
                .flat_map(|(i, m)| -> Box<dyn Iterator<Item = _>> {
                    match m {
                        ModOrGroup::Individual(mc) => Box::new(
                            mc.enabled
                                .then_some(((None, i), mc.spec.clone()))
                                .into_iter(),
                        ),
                        ModOrGroup::Group {
                            group_name,
                            enabled,
                        } => Box::new(
                            enabled
                                .then(|| groups.get(group_name))
                                .flatten()
                                .into_iter()
                                .flat_map(move |g| {
                                    g.mods.iter().enumerate().filter_map(move |(index, mc)| {
                                        mc.enabled.then_some((
                                            (Some(group_name.clone()), index),
                                            mc.spec.clone(),
                                        ))
                                    })
                                }),
                        ),
                    }
                })
                .collect::<Vec<_>>();

            let ui_mod_tags = |ctx: &mut Ctx, ui: &mut Ui, info: &ModInfo| {
                if let Some(ModioTags {
                    qol,
                    gameplay,
                    audio,
                    visual,
                    framework,
                    required_status,
                    approval_status,
                    versions: _,
                }) = info.modio_tags.as_ref()
                {
                    let mut mk_searchable_modio_tag =
                        |tag_str: &str,
                         ui: &mut Ui,
                         color: Option<egui::Color32>,
                         hover_str: Option<&str>| {
                            let search = searchable_text(tag_str, &self.search_string, {
                                TextFormat {
                                    color: if color.is_some() {
                                        Color32::BLACK
                                    } else {
                                        Color32::GRAY
                                    },

                                    ..Default::default()
                                }
                            });

                            let button = if let Some(color) = color {
                                egui::Button::new(search.job)
                                    .small()
                                    .fill(color)
                                    .stroke(egui::Stroke::NONE)
                            } else {
                                egui::Button::new(search.job)
                                    .small()
                                    .stroke(egui::Stroke::NONE)
                            };

                            let res = if let Some(hover_str) = hover_str {
                                ui.add_enabled(false, button)
                                    .on_disabled_hover_text(hover_str)
                            } else {
                                ui.add_enabled(false, button)
                            };

                            if search.is_match && self.scroll_to_match {
                                res.scroll_to_me(None);
                                ctx.scroll_to_match = false;
                            }
                        };

                    match approval_status {
                        ApprovalStatus::Verified => {
                            mk_searchable_modio_tag(
                                "Verified",
                                ui,
                                Some(egui::Color32::LIGHT_GREEN),
                                Some("Does not contain any gameplay affecting features or changes"),
                            );
                        }
                        ApprovalStatus::Approved => {
                            mk_searchable_modio_tag(
                                "Approved",
                                ui,
                                Some(egui::Color32::LIGHT_BLUE),
                                Some("Contains gameplay affecting features or changes"),
                            );
                        }
                        ApprovalStatus::Sandbox => {
                            mk_searchable_modio_tag(
                                "Sandbox",
                                ui,
                                Some(egui::Color32::LIGHT_YELLOW),
                                Some(
                                    "Contains significant, possibly progression breaking, changes to gameplay",
                                ),
                            );
                        }
                    }

                    match required_status {
                        RequiredStatus::RequiredByAll => {
                            mk_searchable_modio_tag(
                                "RequiredByAll",
                                ui,
                                Some(egui::Color32::LIGHT_RED),
                                Some(
                                    "All lobby members must use this mod for it to work correctly!",
                                ),
                            );
                        }
                        RequiredStatus::Optional => {
                            mk_searchable_modio_tag(
                                "Optional",
                                ui,
                                None,
                                Some("Clients are not required to install this mod to function"),
                            );
                        }
                    }

                    if *qol {
                        mk_searchable_modio_tag("QoL", ui, None, None);
                    }
                    if *gameplay {
                        mk_searchable_modio_tag("Gameplay", ui, None, None);
                    }
                    if *audio {
                        mk_searchable_modio_tag("Audio", ui, None, None);
                    }
                    if *visual {
                        mk_searchable_modio_tag("Visual", ui, None, None);
                    }
                    if *framework {
                        mk_searchable_modio_tag("Framework", ui, None, None);
                    }
                }
            };

            let mut ui_mod = |ctx: &mut Ctx,
                              ui: &mut Ui,
                              group: Option<&str>,
                              row_index: usize,
                              mc: &mut ModConfig| {
                if group.is_some() && icons::button(ui, Icon::Delete, "Delete mod").clicked() {
                    ctx.btn_remove = Some((group.map(str::to_owned), row_index));
                }
                if !mc.enabled {
                    let vis = ui.visuals_mut();
                    vis.override_text_color = Some(vis.text_color());
                    vis.hyperlink_color = vis.text_color();
                }

                if ui
                    .add(toggle_switch(&mut mc.enabled))
                    .on_hover_text_at_pointer("Enabled?")
                    .changed()
                {
                    ctx.needs_save = true;
                }

                /*
                if ui
                    .add(egui::Checkbox::without_text(&mut mc.required))
                    .changed()
                {
                    needs_save = true;
                }
                */

                let info = self.state.store.get_mod_info(&mc.spec);

                if let Some(ref info) = info
                    && let Some(modio_id) = info.modio_id
                    && self.problematic_mod_id.is_some_and(|id| id == modio_id)
                {
                    icons::show(ui, Icon::Error).on_hover_text("This mod failed to install");
                }

                if mc.enabled
                    && let Some(req) = &self.integrate_rid
                {
                    match req.state.get(&mc.spec) {
                        Some(SpecFetchProgress::Progress { progress, size }) => {
                            ui.add(
                                egui::ProgressBar::new(*progress as f32 / *size as f32)
                                    .show_percentage()
                                    .desired_width(100.0),
                            );
                        }
                        Some(SpecFetchProgress::Complete) => {
                            ui.add(egui::ProgressBar::new(1.0).desired_width(100.0));
                        }
                        None => {
                            ui.spinner();
                        }
                    }
                }

                if let Some(info) = &info {
                    egui::ComboBox::from_id_salt(row_index)
                        .selected_text(
                            self.state
                                .store
                                .get_version_name(&mc.spec)
                                .unwrap_or_default(),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut mc.spec.url,
                                info.spec.url.to_string(),
                                self.state
                                    .store
                                    .get_version_name(&info.spec)
                                    .unwrap_or_default(),
                            );
                            for version in info.versions.iter().rev() {
                                ui.selectable_value(
                                    &mut mc.spec.url,
                                    version.url.to_string(),
                                    self.state
                                        .store
                                        .get_version_name(version)
                                        .unwrap_or_default(),
                                );
                            }
                        });

                    ui.scope(|ui| {
                        ui.style_mut().spacing.interact_size.x = 30.;
                        let dark = ui.visuals().dark_mode;
                        match mc.priority.cmp(&0) {
                            std::cmp::Ordering::Less => {
                                ui.visuals_mut().override_text_color = Some(if dark {
                                    Color32::LIGHT_RED
                                } else {
                                    Color32::DARK_RED
                                });
                            }
                            std::cmp::Ordering::Greater => {
                                ui.visuals_mut().override_text_color = Some(if dark {
                                    Color32::LIGHT_GREEN
                                } else {
                                    Color32::DARK_GREEN
                                });
                            }
                            _ => {}
                        }
                        ui.add(
                            egui::DragValue::new(&mut mc.priority)
                                .custom_formatter(|n, _| {
                                    if n == 0. {
                                        "-".to_string()
                                    } else {
                                        format!("{n}")
                                    }
                                })
                                .speed(0.05)
                                .range(RangeInclusive::new(-999, 999)),
                        )
                        .on_hover_text_at_pointer(
                            "Load Priority\nIn case of asset conflict, mods with higher priority take precedent.\nCan have duplicate values.",
                        );
                    });

                    if icons::button(ui, Icon::Copy, "Copy URL").clicked() {
                        ui.ctx().copy_text(mc.spec.url.to_string());
                    }

                    if mc.enabled {
                        let is_duplicate = enabled_specs.iter().any(|((name, index), spec)| {
                            (group, row_index) != (name.as_deref(), *index)
                                && info.spec.satisfies_dependency(spec)
                        });
                        if is_duplicate
                            && icons::button(ui, Icon::Warning, "Remove duplicate").clicked()
                        {
                            ctx.btn_remove = Some((group.map(str::to_owned), row_index));
                        }

                        let missing_deps = info
                            .suggested_dependencies
                            .iter()
                            .filter(|d| {
                                !enabled_specs.iter().any(|(_, s)| s.satisfies_dependency(d))
                            })
                            .collect::<Vec<_>>();

                        if !missing_deps.is_empty() {
                            let mut msg = "Add missing dependencies:".to_string();
                            for dep in &missing_deps {
                                msg.push('\n');
                                msg.push_str(&dep.url);
                            }
                            if icons::button(ui, Icon::Warning, &msg).clicked() {
                                ctx.add_deps = Some(missing_deps.into_iter().cloned().collect());
                            }
                        }
                    }

                    match info.provider {
                        "modio" => {
                            let texture: &egui::TextureHandle =
                                self.modio_texture_handle.get_or_insert_with(|| {
                                    let image = image::load_from_memory(MODIO_LOGO_PNG).unwrap();
                                    let size = [image.width() as _, image.height() as _];
                                    let image_buffer = image.to_rgba8();
                                    let pixels = image_buffer.as_flat_samples();
                                    let image = egui::ColorImage::from_rgba_unmultiplied(
                                        size,
                                        pixels.as_slice(),
                                    );

                                    ui.ctx()
                                        .load_texture("modio-logo", image, Default::default())
                                });
                            let mut img =
                                egui::Image::new(texture).fit_to_exact_size([16.0, 16.0].into());
                            if !mc.enabled {
                                img = img.tint(Color32::LIGHT_RED);
                            }
                            ui.add(img);
                        }
                        "http" => {
                            icons::show(ui, Icon::Web).on_hover_text("Web download");
                        }
                        "file" => {
                            icons::show(ui, Icon::Folder).on_hover_text("Local file");
                        }
                        _ => unimplemented!("unimplemented provider kind"),
                    }

                    let search = searchable_text(&info.name, &self.search_string, {
                        TextFormat {
                            color: ui.visuals().hyperlink_color,
                            ..Default::default()
                        }
                    });

                    let res = ui.hyperlink_to(search.job, &mc.spec.url);
                    res.context_menu(|ui| {
                        if let Some(action) = editing::mod_context_menu(
                            ui,
                            &group_names,
                            ListTarget {
                                profile: profile_name.clone(),
                                group: group.map(str::to_owned),
                                index: row_index,
                            },
                        ) {
                            ctx.edit = Some(action);
                        }
                    });
                    if search.is_match && self.scroll_to_match {
                        res.scroll_to_me(None);
                        ctx.scroll_to_match = false;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui_mod_tags(ctx, ui, info);
                    });
                } else {
                    if icons::button(ui, Icon::Copy, "Copy URL").clicked() {
                        ui.ctx().copy_text(mc.spec.url.to_string());
                    }

                    let search = searchable_text(&mc.spec.url, &self.search_string, {
                        TextFormat {
                            color: ui.visuals().hyperlink_color,
                            ..Default::default()
                        }
                    });

                    let res = ui.hyperlink_to(search.job, &mc.spec.url);
                    res.context_menu(|ui| {
                        if let Some(action) = editing::mod_context_menu(
                            ui,
                            &group_names,
                            ListTarget {
                                profile: profile_name.clone(),
                                group: group.map(str::to_owned),
                                index: row_index,
                            },
                        ) {
                            ctx.edit = Some(action);
                        }
                    });
                    if search.is_match && self.scroll_to_match {
                        res.scroll_to_me(None);
                        ctx.scroll_to_match = false;
                    }
                }
            };

            let mut ui_item =
                |ctx: &mut Ctx, ui: &mut Ui, mc: &mut ModOrGroup, row_index: usize| {
                    ui.scope(|ui| {
                        if icons::button(
                            ui,
                            Icon::Delete,
                            if matches!(mc, ModOrGroup::Group { .. }) {
                                "Remove group from profile"
                            } else {
                                "Delete mod"
                            },
                        )
                        .clicked()
                        {
                            ctx.btn_remove = Some((None, row_index));
                        };
                    });

                    match mc {
                        ModOrGroup::Individual(mc) => {
                            ui.push_id(("mod", row_index), |ui| {
                                ui_mod(ctx, ui, None, row_index, mc);
                            });
                        }
                        ModOrGroup::Group {
                            group_name,
                            enabled,
                        } => {
                            if ui
                                .add(toggle_switch(enabled))
                                .on_hover_text_at_pointer("Enabled?")
                                .changed()
                            {
                                ctx.needs_save = true;
                            }
                            ui.collapsing(group_name.as_str(), |ui| {
                                let group = groups.get_mut(group_name).unwrap();
                                let order = sorted_mod_indices(
                                    group.mods.iter().map(Some),
                                    sorting_config.as_ref(),
                                    |spec| self.state.store.get_mod_info(spec),
                                );
                                for index in order {
                                    ui.push_id(("group", group_name.as_str(), index), |ui| {
                                        ui.horizontal(|ui| {
                                            ui_mod(
                                                ctx,
                                                ui,
                                                Some(group_name),
                                                index,
                                                &mut group.mods[index],
                                            );
                                        });
                                    });
                                }
                            })
                            .header_response
                            .context_menu(|ui| {
                                ui.label("Shared group: changes apply to all profiles using it.");
                                if ui.button("Rename group…").clicked() {
                                    ctx.open_group = Some(group_name.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Ungroup").clicked() {
                                    ctx.edit = Some(Edit::Ungroup {
                                        profile: profile_name.clone(),
                                        index: row_index,
                                    });
                                    ui.close_menu();
                                }
                            });
                        }
                    }
                };

            if sorting_config.is_some() {
                let order = sorted_mod_indices(
                    profile.mods.iter().map(|item| match item {
                        ModOrGroup::Individual(mc) => Some(mc),
                        ModOrGroup::Group { .. } => None,
                    }),
                    sorting_config.as_ref(),
                    |spec| self.state.store.get_mod_info(spec),
                );
                for (visual_index, store_index) in order.into_iter().enumerate() {
                    let mut frame = egui::Frame::NONE;
                    if visual_index % 2 == 1 {
                        frame.fill = ui.visuals().faint_bg_color
                    }
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui_item(&mut ctx, ui, &mut profile.mods[store_index], store_index);
                        });
                    });
                }
            } else {
                let res = egui_dnd::dnd(ui, ui.id())
                    .with_mouse_config(egui_dnd::DragDropConfig::mouse())
                    .show(
                        profile.mods.iter_mut().enumerate(),
                        |ui, (_index, item), handle, state| {
                            let mut frame = egui::Frame::NONE;
                            if state.dragged {
                                frame.fill = ui.visuals().extreme_bg_color
                            } else if state.index % 2 == 1 {
                                frame.fill = ui.visuals().faint_bg_color
                            }
                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    handle.ui(ui, |ui| {
                                        ui.add_sized(
                                            [26.0, 18.0],
                                            Icon::Drag.image(ui, ui.visuals().text_color()),
                                        );
                                    });

                                    ui_item(&mut ctx, ui, item, state.index);
                                });
                            });
                        },
                    );

                if res.final_update().is_some() {
                    res.update_vec(&mut profile.mods);
                    ctx.needs_save = true;
                }
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(profile) = profiles.get_mut(profile) {
                ui_profile(ui, profile);
            } else {
                ui.label("no such profile");
            }
        });

        if let Some(add_deps) = ctx.add_deps {
            message::ResolveMods::send(self, ui.ctx(), add_deps, true);
            self.problematic_mod_id = None;
        }

        self.scroll_to_match = ctx.scroll_to_match;

        if ctx.needs_save {
            self.report_save(self.state.mod_data.save());
        }
        if let Some(name) = ctx.open_group {
            self.groups_window = Some(groups::GroupsWindow::selected(name));
        }
        if let Some((group, index)) = ctx.btn_remove {
            ctx.edit = Some(Edit::DeleteEntry(ListTarget {
                profile: profile_name,
                group,
                index,
            }));
        }
        if let Some(edit) = ctx.edit {
            self.request_edit(edit, editing::skip_delete_confirmation(ui.ctx()));
        }
    }
}
