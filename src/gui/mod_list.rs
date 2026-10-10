use super::*;
use crate::state::edits::{Edit, ListTarget};

const ROW_PADDING: egui::Margin = egui::Margin::symmetric(0, 1);

impl App {
    pub(super) fn ui_profile(&mut self, ui: &mut Ui, profile: &str) {
        if self.editing_busy() || self.delete_confirmation.is_some() {
            ui.disable();
        }
        let sorting_config = self.get_sorting_config();
        let mixed_order = self
            .state
            .mod_data
            .profiles
            .get(profile)
            .is_some_and(|p| p.has_mixed_order());
        if mixed_order {
            ui.horizontal_wrapped(|ui| {
                ui.label("Arrange this profile to keep standalone mods before groups.");
                if ui.button("Arrange groups…").clicked() {
                    self.request_edit(
                        crate::state::edits::Edit::ArrangeProfile(profile.into()),
                        false,
                    );
                }
            });
        }
        let can_drag = ui.is_enabled() && !mixed_order;
        let display_order = self.visible_order(profile, sorting_config.as_ref());
        let sorting_key = sorting_config
            .as_ref()
            .map(|config| (config.sort_category, config.is_ascending));
        let snapshot = (ui.input(|i| i.pointer.primary_down())
            && !egui::DragAndDrop::has_any_payload(ui.ctx()))
        .then(|| drag_drop::DragSnapshot {
            before: (**self.state.mod_data).clone(),
            order: display_order.clone(),
            sorting: sorting_key,
        });
        let profile_name = profile.to_owned();
        let group_names: Vec<_> = self.state.mod_data.groups.keys().cloned().collect();
        let group_fills: HashMap<_, _> = self
            .state
            .mod_data
            .groups
            .iter()
            .map(|(name, group)| {
                (
                    name.clone(),
                    group_colors::palette(group.color, ui.visuals().dark_mode).1,
                )
            })
            .collect();
        let group_views = &mut self.group_views;

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
            rows: Vec<drag_drop::DropRow>,
            header: Option<(egui::Rect, String, usize)>,
        }
        let mut ctx = Ctx {
            needs_save: false,
            scroll_to_match: self.scroll_to_match,
            btn_remove: None,
            add_deps: None,
            edit: None,
            open_group: None,
            rows: Vec::new(),
            header: None,
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
                    .add(toggle_switch::stable_toggle_switch(&mut mc.enabled))
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
                    let dropdown = egui::ComboBox::from_id_salt("version")
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
                    named_combobox::close_on_outside_press(ui, &dropdown.response);

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
                |ctx: &mut Ctx,
                 ui: &mut Ui,
                 mc: &mut ModOrGroup,
                 row_index: usize,
                 group_views: &mut group_view::GroupViews| {
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
                            ui.push_id("mod", |ui| {
                                ui_mod(ctx, ui, None, row_index, mc);
                            });
                        }
                        ModOrGroup::Group {
                            group_name,
                            enabled,
                        } => {
                            if ui
                                .add(toggle_switch::stable_toggle_switch(enabled))
                                .on_hover_text_at_pointer("Enabled?")
                                .changed()
                            {
                                ctx.needs_save = true;
                            }
                            let header = egui::CollapsingHeader::new(group_name.as_str())
                                .open(Some({
                                    let state = group_views.get(&profile_name, group_name);
                                    let hovering = can_drag
                                        && ui.is_enabled()
                                        && drag_drop::can_hover_group(ui.ctx(), &profile_name)
                                        && ui.input(|i| i.pointer.interact_pos()).is_some_and(
                                            |pos| {
                                                state.rect.is_some_and(|rect| rect.contains(pos))
                                                    && ui.clip_rect().contains(pos)
                                                    && ui.ctx().layer_id_at(pos)
                                                        == Some(ui.layer_id())
                                            },
                                        );
                                    if state.hover(hovering, ui.input(|i| i.time)) {
                                        ui.ctx().request_repaint_after(Duration::from_millis(50));
                                    }
                                    state.open
                                }))
                                .icon(|ui, openness, response| {
                                    Icon::ChevronRight
                                        .image(ui, ui.visuals().text_color())
                                        .rotate(
                                            if openness > 0.5 {
                                                std::f32::consts::FRAC_PI_2
                                            } else {
                                                0.0
                                            },
                                            egui::Vec2::splat(0.5),
                                        )
                                        .paint_at(
                                            ui,
                                            egui::Rect::from_center_size(
                                                response.rect.center(),
                                                egui::Vec2::splat(icons::SIZE),
                                            ),
                                        );
                                })
                                .show(ui, |ui| {
                                    let body_background = ui.painter().add(egui::Shape::Noop);
                                    let body_top = ui.cursor().top();
                                    let group = groups.get_mut(group_name).unwrap();
                                    let keys = row_keys(
                                        group.mods.iter().map(|mc| (false, mc.spec.url.clone())),
                                    );
                                    for (visual_index, &index) in
                                        display_order.groups[group_name].iter().enumerate()
                                    {
                                        ui.push_id(
                                            ("group", group_name.as_str(), &keys[index]),
                                            |ui| {
                                                let stripe = ui.painter().add(egui::Shape::Noop);
                                                let row = egui::Frame::NONE
                                                    .inner_margin(ROW_PADDING)
                                                    .show(ui, |ui| {
                                                        ui.horizontal(|ui| {
                                                            drag_drop::handle(
                                                                ui,
                                                                ListTarget {
                                                                    profile: profile_name.clone(),
                                                                    group: Some(group_name.clone()),
                                                                    index,
                                                                },
                                                                snapshot.as_ref(),
                                                                false,
                                                                &group.mods[index].spec.url,
                                                                can_drag,
                                                            );
                                                            ui_mod(
                                                                ctx,
                                                                ui,
                                                                Some(group_name),
                                                                index,
                                                                &mut group.mods[index],
                                                            );
                                                        })
                                                    });
                                                let mut rect = row.response.rect;
                                                rect.max.x = ui.max_rect().right();
                                                if visual_index % 2 == 1 {
                                                    ui.painter().set(
                                                        stripe,
                                                        egui::Shape::rect_filled(
                                                            rect.expand2(egui::vec2(
                                                                0.0,
                                                                ui.spacing().item_spacing.y / 2.0,
                                                            )),
                                                            0,
                                                            ui.visuals().faint_bg_color,
                                                        ),
                                                    );
                                                }
                                                ctx.rows.push(drag_drop::DropRow {
                                                    rect,
                                                    target: ListTarget {
                                                        profile: profile_name.clone(),
                                                        group: Some(group_name.clone()),
                                                        index: visual_index,
                                                    },
                                                    group: None,
                                                    parent_index: Some(row_index),
                                                });
                                            },
                                        );
                                    }
                                    let body_rect = egui::Rect::from_min_max(
                                        egui::pos2(ui.max_rect().left(), body_top),
                                        egui::pos2(ui.max_rect().right(), ui.min_rect().bottom()),
                                    );
                                    ui.painter().set(
                                        body_background,
                                        egui::Shape::rect_filled(
                                            body_rect,
                                            0,
                                            ui.visuals().panel_fill,
                                        ),
                                    );
                                });
                            let header_rect = header.header_response.rect;
                            let tail = egui::Rect::from_min_max(
                                header_rect.right_top(),
                                egui::pos2(
                                    ui.max_rect().right().max(header_rect.right()),
                                    header_rect.bottom(),
                                ),
                            );
                            let count = groups[group_name].mods.len();
                            let count_label =
                                format!("({count} {})", if count == 1 { "mod" } else { "mods" });
                            ui.painter()
                                .with_clip_rect(tail.intersect(ui.clip_rect()))
                                .text(
                                    tail.left_center() + egui::vec2(4.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    &count_label,
                                    egui::TextStyle::Button.resolve(ui.style()),
                                    ui.visuals().text_color(),
                                );
                            let tail_response = ui.interact(
                                tail,
                                header.header_response.id.with("tail"),
                                egui::Sense::click(),
                            );
                            let header_response = header
                                .header_response
                                .union(tail_response)
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            header_response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::CollapsingHeader,
                                    ui.is_enabled(),
                                    format!("{group_name} {count_label}"),
                                )
                            });
                            if header_response.clicked() {
                                group_views.get(&profile_name, group_name).toggle();
                                ui.ctx().request_repaint();
                            }
                            ctx.header = Some((
                                header_response.rect,
                                group_name.clone(),
                                groups[group_name].mods.len(),
                            ));
                            header_response.context_menu(|ui| {
                                if let Some(color) =
                                    group_colors::picker(ui, groups[group_name].color)
                                {
                                    ctx.edit = Some(Edit::SetGroupColor {
                                        name: group_name.clone(),
                                        color,
                                    });
                                }
                                ui.separator();
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

            let keys = row_keys(profile.mods.iter().map(|entry| match entry {
                ModOrGroup::Individual(mc) => (false, mc.spec.url.clone()),
                ModOrGroup::Group { group_name, .. } => (true, group_name.clone()),
            }));
            for (visual_index, &store_index) in display_order.profile.iter().enumerate() {
                let group_name = match &profile.mods[store_index] {
                    ModOrGroup::Group { group_name, .. } => Some(group_name.clone()),
                    _ => None,
                };
                let mut frame = egui::Frame::NONE.inner_margin(ROW_PADDING);
                if visual_index % 2 == 1 {
                    frame.fill = ui.visuals().faint_bg_color;
                }
                if let Some(name) = &group_name {
                    frame.fill = group_fills[name];
                    frame.corner_radius = 3.into();
                }
                ctx.header = None;
                let row = ui.push_id((profile_name.as_str(), &keys[store_index]), |ui| {
                    frame.show(ui, |ui| {
                        if group_name.is_some() {
                            ui.set_min_width(ui.available_width());
                        }
                        ui.horizontal(|ui| {
                            let (is_group, label) = match &profile.mods[store_index] {
                                ModOrGroup::Individual(mc) => (false, mc.spec.url.as_str()),
                                ModOrGroup::Group { group_name, .. } => (true, group_name.as_str()),
                            };
                            drag_drop::handle(
                                ui,
                                ListTarget {
                                    profile: profile_name.clone(),
                                    group: None,
                                    index: store_index,
                                },
                                snapshot.as_ref(),
                                is_group,
                                label,
                                can_drag,
                            );
                            ui_item(
                                &mut ctx,
                                ui,
                                &mut profile.mods[store_index],
                                store_index,
                                group_views,
                            );
                        });
                    });
                });
                let mut rect = row.response.rect;
                rect.max.x = ui.max_rect().right();
                if let Some(name) = group_name {
                    group_views.get(&profile_name, &name).rect = Some(rect);
                    ui.painter().line_segment(
                        [rect.left_top(), rect.left_bottom()],
                        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );
                }
                let group = ctx.header.take().map(|(header, name, len)| {
                    rect.min.y = header.top();
                    rect.max.y = header.bottom();
                    (name, len)
                });
                ctx.rows.push(drag_drop::DropRow {
                    rect,
                    target: ListTarget {
                        profile: profile_name.clone(),
                        group: None,
                        index: visual_index,
                    },
                    group,
                    parent_index: None,
                });
            }
            drag_drop::finish(
                ui,
                &ctx.rows,
                &profile_name,
                profile.mods.len(),
                can_drag,
                &display_order,
                sorting_key,
            )
        };

        let mut dropped = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(profile) = profiles.get_mut(profile) {
                dropped = ui_profile(ui, profile);
            } else {
                ui.label("no such profile");
            }
        });

        if let Some(edit) = dropped
            && self.finish_edit(edit)
            && sorting_config.is_some()
        {
            self.update_sorting_config(None, false);
        }

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

fn row_keys(entries: impl Iterator<Item = (bool, String)>) -> Vec<(bool, String, usize)> {
    let mut occurrences = HashMap::new();
    entries
        .map(|(group, name)| {
            let count = occurrences.entry((group, name.clone())).or_insert(0);
            let key = (group, name, *count);
            *count += 1;
            key
        })
        .collect()
}
