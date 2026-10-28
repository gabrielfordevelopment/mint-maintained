use super::*;
use crate::state::edits::{Edit, ListTarget};
use std::collections::HashSet;

const ROW_PADDING: egui::Margin = egui::Margin::symmetric(0, 1);
/// Options of the Compact/Normal switch, shared by the list toolbar and Settings.
pub(super) const VIEW_SEGMENTS: [icons::Segment<'static, ModListView>; 2] = [
    (
        ModListView::Compact,
        Icon::ViewList,
        "Compact",
        "One dense line per mod",
    ),
    (
        ModListView::Normal,
        Icon::ViewAgenda,
        "Normal",
        "Taller rows with thumbnails, author and summary",
    ),
];
/// Height of a mod row in the normal view. Compact rows keep the control height.
pub(super) const NORMAL_ROW_HEIGHT: f32 = 40.0;
const PRIORITY_WIDTH: f32 = 30.0;
/// Gap between row tags.
pub(super) const TAG_GAP: f32 = 4.0;
/// Width kept for a mod name before row tags are dropped.
const MIN_NAME_WIDTH: f32 = 160.0;
const MIN_SUBTITLE_WIDTH: f32 = 120.0;

/// Lays out one mod row. Normal rows get a fixed height with vertically centered controls.
fn row_layout<R>(ui: &mut Ui, height: Option<f32>, add: impl FnOnce(&mut Ui) -> R) -> R {
    match height {
        None => ui.horizontal(add).inner,
        Some(height) => {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                Layout::left_to_right(Align::Center),
                |ui| {
                    // Fill the row like compact rows do, so stripes and drop targets span it.
                    ui.set_min_size(egui::vec2(ui.available_width(), height));
                    add(ui)
                },
            )
            .inner
        }
    }
}

/// Draws a cached thumbnail or a deterministic placeholder. Clicking it opens the details.
fn thumbnail(
    ui: &mut Ui,
    size: egui::Vec2,
    texture: Option<&egui::TextureHandle>,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let radius = 3.0;
        let dark = ui.visuals().dark_mode;
        match texture {
            Some(texture) => {
                let mut image = egui::Image::new(texture)
                    .corner_radius(radius)
                    .fit_to_exact_size(size);
                if !enabled {
                    image = image.tint(Color32::from_gray(if dark { 120 } else { 170 }));
                }
                image.paint_at(ui, rect);
            }
            None => {
                let hash = label.bytes().fold(0u32, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(byte as u32)
                });
                let hue = (hash % 360) as f32 / 360.0;
                let fill = egui::ecolor::HsvaGamma {
                    h: hue,
                    s: 0.35,
                    v: if dark { 0.35 } else { 0.9 },
                    a: 1.0,
                };
                ui.painter().rect_filled(rect, radius, fill);
                let initial = label
                    .chars()
                    .find(|c| c.is_alphanumeric())
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default();
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    initial,
                    egui::FontId::proportional((size.y * 0.6).clamp(10.0, 20.0)),
                    ui.visuals().text_color(),
                );
            }
        }
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Show details")
}

pub(super) const PRIORITY_HINT: &str = "Load Priority\nIn case of asset conflict, mods with higher priority take precedent.\nCan have duplicate values.";

/// Id of a priority editor whose value changed and whether it is still being dragged or typed in.
pub(super) fn priority_editing(response: &egui::Response) -> (egui::Id, bool) {
    (response.id, response.dragged() || response.has_focus())
}

pub(super) fn priority_value(priority: &mut i32) -> egui::DragValue<'_> {
    egui::DragValue::new(priority)
        .custom_formatter(|n, _| {
            if n == 0. {
                "-".to_string()
            } else {
                format!("{n}")
            }
        })
        .speed(0.05)
        .range(RangeInclusive::new(-999, 999))
}

impl App {
    /// Applies a changed priority. While its editor is still dragged or typed in, saving waits
    /// until it is released, so one edit saves once and is one undo step instead of one per
    /// intermediate value.
    pub(super) fn priority_changed(&mut self, id: egui::Id, editing: bool) {
        if editing {
            self.priority_edit = Some(id);
        } else {
            self.priority_edit = None;
            self.save_mod_data("Edit mod settings");
        }
    }

    /// Saves a pending priority edit once its editor is no longer dragged or focused, even if the
    /// editor is no longer shown.
    pub(super) fn finish_priority_edit(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.priority_edit
            && !ctx.is_being_dragged(id)
            && !ctx.memory(|m| m.has_focus(id))
        {
            self.flush_priority_edit();
        }
    }

    /// Saves a pending priority edit now, for example before undoing.
    pub(super) fn flush_priority_edit(&mut self) {
        if self.priority_edit.take().is_some() {
            self.save_mod_data("Edit mod settings");
        }
    }
}

impl RowStriping {
    pub(super) const ALL: [RowStriping; 4] = [
        RowStriping::Off,
        RowStriping::Subtle,
        RowStriping::Medium,
        RowStriping::Strong,
    ];

    pub(super) fn as_str(self) -> &'static str {
        match self {
            RowStriping::Off => "Off",
            RowStriping::Subtle => "Subtle",
            RowStriping::Medium => "Medium",
            RowStriping::Strong => "Strong",
        }
    }

    /// Background of every other row, mixed from the panel and text colors of the theme.
    pub(super) fn fill(self, visuals: &egui::Visuals) -> Option<Color32> {
        // Dark panels need a larger share of the text color for a similar visible contrast.
        let amount = match (self, visuals.dark_mode) {
            (RowStriping::Off, _) => return None,
            (RowStriping::Subtle, _) => return Some(visuals.faint_bg_color),
            (RowStriping::Medium, false) => 0.07,
            (RowStriping::Medium, true) => 0.10,
            (RowStriping::Strong, false) => 0.13,
            (RowStriping::Strong, true) => 0.18,
        };
        let base = visuals.panel_fill;
        let ink = visuals.text_color();
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
        Some(Color32::from_rgb(
            mix(base.r(), ink.r()),
            mix(base.g(), ink.g()),
            mix(base.b(), ink.b()),
        ))
    }
}

/// A mod.io tag: its label, status color (none for neutral tags) and explanation.
pub(super) struct Tag {
    pub label: &'static str,
    pub fill: Option<Color32>,
    pub hint: Option<&'static str>,
}

/// Tags in order of importance: approval, required status, then categories.
pub(super) fn modio_tags(tags: &mint_lib::mod_info::ModioTags) -> Vec<Tag> {
    let tag = |label, fill, hint| Tag { label, fill, hint };
    let mut list = vec![match tags.approval_status {
        ApprovalStatus::Verified => tag(
            "Verified",
            Some(Color32::LIGHT_GREEN),
            Some("Does not contain any gameplay affecting features or changes"),
        ),
        ApprovalStatus::Approved => tag(
            "Approved",
            Some(Color32::LIGHT_BLUE),
            Some("Contains gameplay affecting features or changes"),
        ),
        ApprovalStatus::Sandbox => tag(
            "Sandbox",
            Some(Color32::LIGHT_YELLOW),
            Some("Contains significant, possibly progression breaking, changes to gameplay"),
        ),
    }];
    list.push(match tags.required_status {
        RequiredStatus::RequiredByAll => tag(
            "RequiredByAll",
            Some(Color32::LIGHT_RED),
            Some("All lobby members must use this mod for it to work correctly!"),
        ),
        RequiredStatus::Optional => tag(
            "Optional",
            None,
            Some("Clients are not required to install this mod to function"),
        ),
    });
    for (on, label) in [
        (tags.qol, "QoL"),
        (tags.gameplay, "Gameplay"),
        (tags.audio, "Audio"),
        (tags.visual, "Visual"),
        (tags.framework, "Framework"),
    ] {
        if on {
            list.push(tag(label, None, None));
        }
    }
    list
}

/// A tag as a disabled small button, as before. Neutral tags get an outline instead of the
/// default fill so adjacent tags stay distinct on striped rows.
pub(super) fn tag_chip(
    ui: &mut Ui,
    text: impl Into<egui::WidgetText>,
    fill: Option<Color32>,
) -> egui::Response {
    let button = egui::Button::new(text).small();
    let button = match fill {
        Some(fill) => button.fill(fill).stroke(egui::Stroke::NONE),
        None => button.fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(
            1.0_f32,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        )),
    };
    ui.add_enabled(false, button)
}

pub(super) fn tag_text(visuals: &egui::Visuals, fill: Option<Color32>) -> Color32 {
    match fill {
        Some(_) => Color32::BLACK,
        None => neutral_tag_text(visuals),
    }
}

/// Text color of tags without a status color. Disabled widgets fade halfway to the background,
/// so it starts from the strong text color, like the black text on colored tags, to stay
/// readable on every row stripe.
pub(super) fn neutral_tag_text(visuals: &egui::Visuals) -> Color32 {
    visuals.strong_text_color()
}

/// A row with half of the spacing above and below it. Adjacent slots meet, so stripes, the
/// hover color, the selection outline and row clicks leave no gap between rows.
fn row_slot(ui: &Ui, rect: egui::Rect) -> egui::Rect {
    rect.expand2(egui::vec2(0.0, ui.spacing().item_spacing.y / 2.0))
}

/// Outline of the selected row's slot. It is painted after every row, so the next row's
/// background never covers its lower edge.
fn selection_outline(ui: &Ui, slot: egui::Rect) {
    ui.painter().rect_stroke(
        slot,
        SELECTED_RADIUS,
        ui.visuals().selection.stroke,
        egui::StrokeKind::Inside,
    );
}

const SELECTED_RADIUS: u8 = 2;

/// Registers a mod row's click target before the row's controls. egui keeps a widget's place
/// in the hit-test order when it is updated later in the frame, so the row is updated to its
/// slot once drawn and stays below its controls: they keep their clicks, while clicks on free
/// space select the row and right clicks open its menu.
fn reserve_row(ui: &mut Ui, id: egui::Id) {
    ui.interact(
        egui::Rect::from_min_size(ui.cursor().min, egui::Vec2::ZERO),
        id,
        egui::Sense::click(),
    );
}

/// Describes a mod row's click target for screen readers and remembers its slot while it has
/// keyboard focus. The target is focusable like other click targets: Tab reaches it and Enter or
/// Space shows the mod's details.
fn row_focus(
    ui: &Ui,
    response: &egui::Response,
    focused: &mut Option<egui::Rect>,
    slot: egui::Rect,
    mod_row: Option<&(ModSpecification, bool, String)>,
) {
    if let Some((_, _, name)) = mod_row {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                ui.is_enabled(),
                format!("{name} details"),
            )
        });
    }
    if response.has_focus() {
        *focused = Some(slot);
        // Up and Down step through the mods instead of moving focus to a nearby control.
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
}

/// Index in `rows` (shown mods in display order) to select after this frame's Up and Down
/// presses. Without a focused control, or with focus on a mod row, the keys step from the
/// selected mod, stopping at the ends. A selected mod that is not shown steps to the first or
/// last one. Other focused controls keep the keys for egui's focus navigation.
fn arrow_step(
    ui: &mut Ui,
    rows: &[(Option<String>, usize, egui::Id, egui::Rect)],
    selected: Option<&details::DetailsTarget>,
) -> Option<usize> {
    let focused = ui.memory(|memory| memory.focused());
    if focused.is_some_and(|focused| !rows.iter().any(|row| row.2 == focused)) || rows.is_empty() {
        return None;
    }
    let step = ui.input_mut(|input| {
        input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) as isize
            - input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) as isize
    });
    if step == 0 {
        return None;
    }
    let last = rows.len() as isize - 1;
    let current = selected.and_then(|selected| {
        rows.iter().position(|(group, index, ..)| {
            group.as_deref() == selected.target.group.as_deref() && *index == selected.target.index
        })
    });
    Some(match current {
        Some(current) => (current as isize + step).clamp(0, last) as usize,
        None if step > 0 => 0,
        None => last as usize,
    })
}

/// Thin separator above a row, centered in the spacing between rows.
fn row_line(ui: &Ui, rect: egui::Rect) -> (egui::layers::ShapeIdx, f32) {
    let y = rect.top() - ui.spacing().item_spacing.y / 2.0;
    let line = ui.painter().add(egui::Shape::hline(
        rect.x_range(),
        y,
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    ));
    (line, y)
}

/// Background of the mod row under the pointer: a light tint of the theme's selection color,
/// distinct from the gray row stripes.
pub(super) fn row_hover_fill(visuals: &egui::Visuals) -> Color32 {
    selection_tint(visuals, if visuals.dark_mode { 0.35 } else { 0.3 })
}

/// Background of the selected mod row: the hover tint, a little stronger so both stay distinct.
pub(super) fn row_selected_fill(visuals: &egui::Visuals) -> Color32 {
    selection_tint(visuals, if visuals.dark_mode { 0.55 } else { 0.5 })
}

fn selection_tint(visuals: &egui::Visuals, amount: f32) -> Color32 {
    let base = visuals.panel_fill;
    let tint = visuals.selection.bg_fill;
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color32::from_rgb(
        mix(base.r(), tint.r()),
        mix(base.g(), tint.g()),
        mix(base.b(), tint.b()),
    )
}

/// Heights of mod rows when they were last drawn, keyed by their paint key, and the layout they
/// were measured with.
#[derive(Default)]
pub(super) struct RowHeights {
    layout: Option<egui::Id>,
    heights: HashMap<egui::Id, f32>,
    /// Rows drawn in the last frame.
    drawn: HashSet<egui::Id>,
    /// Rows drawn when a pointer button was pressed, kept drawn until it is released.
    held: HashSet<egui::Id>,
    /// Rows with an open context menu, kept drawn so the menu stays open.
    menus: HashSet<egui::Id>,
}

/// Skips building mod rows far outside the visible part of the list, so a frame costs about the
/// same with ten mods or a thousand. A skipped row only takes up the height it had when it was
/// last drawn, so the scroll range, drop targets and arrow-key navigation stay as if it were
/// drawn. Rows without a known height, the dragged row and every row while scrolling to a search
/// match are always drawn.
///
/// egui resolves a click on release against the widget pressed earlier. The list can scroll in
/// between, so rows drawn at the press stay drawn until release: a click, toggle or drag that
/// started on a row reaches it even if it moved far away. A row's context menu is shown only
/// while the row is built, so rows with an open menu stay drawn as well.
struct RowCulling {
    previous: HashMap<egui::Id, f32>,
    held: HashSet<egui::Id>,
    menus: HashSet<egui::Id>,
    next: RowHeights,
    /// Vertical range in which rows are drawn, or `None` to draw every row.
    window: Option<egui::Rangef>,
    dragged: Option<ListTarget>,
}

impl RowCulling {
    fn new(ui: &Ui, heights: RowHeights, layout: egui::Id, draw_all: bool) -> Self {
        let (pressed, holding) = ui.input(|input| {
            let pointer = &input.pointer;
            (
                pointer.any_pressed(),
                pointer.any_down() || pointer.any_released(),
            )
        });
        let held = if pressed {
            heights.drawn
        } else if holding {
            heights.held
        } else {
            HashSet::new()
        };
        let previous = if heights.layout == Some(layout) {
            heights.heights
        } else {
            HashMap::new()
        };
        let dragged = drag_drop::dragged_source(ui.ctx());
        let clip = ui.clip_rect();
        // Rows a little outside the view are drawn as well. While dragging, rows slide by up to
        // the dragged row's height, so a tall group must not uncover rows that were skipped.
        let mut margin = (clip.height() / 2.0).max(64.0);
        if dragged.is_some() {
            margin += previous.values().copied().fold(0.0, f32::max);
        }
        Self {
            previous,
            next: RowHeights {
                layout: Some(layout),
                heights: HashMap::new(),
                drawn: HashSet::new(),
                held: held.clone(),
                menus: HashSet::new(),
            },
            held,
            menus: heights.menus,
            window: (!draw_all).then(|| clip.y_range().expand(margin)),
            dragged,
        }
    }

    /// The height to reserve instead of drawing the row at the cursor, or `None` to draw it.
    fn skip(&mut self, ui: &Ui, key: egui::Id, entry: &ListTarget) -> Option<f32> {
        let window = self.window?;
        let height = *self.previous.get(&key)?;
        if self.dragged.as_ref() == Some(entry)
            || self.held.contains(&key)
            || self.menus.contains(&key)
        {
            return None;
        }
        let top = ui.cursor().top();
        if top + height >= window.min && top <= window.max {
            return None;
        }
        self.next.heights.insert(key, height);
        Some(height)
    }

    fn measured(&mut self, key: egui::Id, height: f32) {
        self.next.heights.insert(key, height);
        self.next.drawn.insert(key);
    }

    fn menu_open(&mut self, key: egui::Id) {
        self.next.menus.insert(key);
    }
}

/// Space of a skipped row: its slot is still a click target while it has keyboard focus, so
/// focus survives scrolling it out of view and arrow keys can step to it.
fn skipped_row(ui: &mut Ui, height: f32, row_id: egui::Id) -> (egui::Rect, egui::Rect) {
    let (_, mut rect) = ui.allocate_space(egui::vec2(ui.available_width(), height));
    rect.max.x = ui.max_rect().right();
    let slot = row_slot(ui, rect);
    if ui.memory(|memory| memory.has_focus(row_id)) {
        ui.interact(slot, row_id, egui::Sense::click());
    }
    (rect, slot)
}

/// Last component of a local path or URL, used as a readable label without metadata.
fn short_label(url: &str) -> &str {
    url.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(url)
}

impl App {
    pub(super) fn ui_profile(&mut self, ui: &mut Ui, profile: &str) {
        if self.editing_busy() || self.delete_confirmation.is_some() {
            ui.disable();
        }
        // Rows leave one pixel above and below their controls. Growing widgets on hover would
        // reach into the neighboring row's background and be cut off. Hover still changes color.
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.hovered.expansion = 0.0;
        widgets.active.expansion = 0.0;
        let sorting_config = self.get_sorting_config();
        let mixed_order = self
            .state
            .mod_data
            .profiles
            .get(profile)
            .is_some_and(|p| p.has_mixed_order());
        if mixed_order {
            ui.horizontal_wrapped(|ui| {
                ui.label("Arrange this profile to keep groups before standalone mods.");
                if ui.button("Arrange groups…").clicked() {
                    self.request_edit(
                        crate::state::edits::Edit::ArrangeProfile(profile.into()),
                        false,
                    );
                }
            });
        }
        let searching = !self.search_string.is_empty();
        let chips = self.mod_filters.is_active();
        let filtering = chips || searching;
        let can_drag = ui.is_enabled() && !mixed_order && !filtering;
        let display_order = self.visible_order(profile, sorting_config.as_ref());
        // Drag snapshots keep the complete order. Filtering only hides rows while dragging is off.
        let (shown_order, group_counts) = self.filter_order(
            profile,
            &display_order,
            &self.mod_filters,
            &self.search_string,
        );
        if filtering {
            let (what, clear, hint) = match (chips, searching) {
                (true, true) => (
                    "the search and filters",
                    "Clear search and filters",
                    "Clear the search and filters to reorder mods by dragging.",
                ),
                (true, false) => (
                    "the selected filters",
                    "Clear filters",
                    "Clear filters to reorder mods by dragging.",
                ),
                _ => (
                    "the search",
                    "Clear search",
                    "Clear the search to reorder mods by dragging.",
                ),
            };
            if shown_order.profile.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(format!("No mods match {what}."));
                    if ui.button(clear).clicked() {
                        self.mod_filters = Default::default();
                        self.search_string.clear();
                    }
                });
            } else {
                ui.weak(hint);
            }
        }
        let list = self.state.config.mod_list.clone();
        let row_height = (list.view == ModListView::Normal).then_some(NORMAL_ROW_HEIGHT);
        let thumbnail_size = match list.view {
            ModListView::Compact => egui::vec2(28.0, 16.0),
            ModListView::Normal => egui::vec2(64.0, 36.0),
        };
        let show_thumbnails = list.thumbnails();
        let stripe_fill = list.striping.fill(ui.visuals());
        let hover_fill = row_hover_fill(ui.visuals());
        let selected_fill = row_selected_fill(ui.visuals());
        let row_lines = list.row_lines;
        // Highlight the row under the pointer, but not while dragging or while the list is locked.
        let highlight_hover = ui.is_enabled() && !egui::DragAndDrop::has_any_payload(ui.ctx());
        let selected = self.details.clone();
        // With a mod selected, Up and Down step through the shown mods unless the keyboard
        // belongs to something else: a focused control other than a mod row (see `arrow_step`),
        // a dropdown, a confirmation or a drag.
        let arrow_nav = selected
            .as_ref()
            .is_some_and(|selected| selected.target.profile == profile)
            && ui.is_enabled()
            && !ui
                .ctx()
                .memory(|memory| memory.any_popup_open() || memory.top_modal_layer().is_some())
            && !egui::DragAndDrop::has_any_payload(ui.ctx());
        let is_selected = |group: Option<&str>, index: usize| {
            selected.as_ref().is_some_and(|s| {
                s.target.profile == profile
                    && s.target.group.as_deref() == group
                    && s.target.index == index
            })
        };
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
            copied_link: bool,
            check_update: Option<ModSpecification>,
            show_details: Option<(Option<String>, usize)>,
            open_url: Option<String>,
            rows: Vec<drag_drop::DropRow>,
            header: Option<(egui::Rect, String, usize)>,
            body: Option<egui::layers::ShapeIdx>,
            selected_slot: Option<egui::Rect>,
            lines: Vec<(egui::layers::ShapeIdx, f32)>,
            /// The mod drawn in the current row and whether it comes from mod.io, for its menu.
            row_menu: Option<(ModSpecification, bool, String)>,
            /// Slot of the row that has keyboard focus.
            focused_slot: Option<egui::Rect>,
            /// Shown mod rows in display order, for stepping with the arrow keys.
            nav: Vec<(Option<String>, usize, egui::Id, egui::Rect)>,
            hide_details: bool,
            /// A priority editor that changed its value and whether it is still being edited.
            priority_edit: Option<(egui::Id, bool)>,
            row_heights: RowHeights,
            culling: Option<RowCulling>,
            /// The current row's name has its context menu open.
            name_menu_open: bool,
        }
        let mut ctx = Ctx {
            needs_save: false,
            scroll_to_match: self.scroll_to_match,
            btn_remove: None,
            add_deps: None,
            edit: None,
            open_group: None,
            copied_link: false,
            check_update: None,
            show_details: None,
            open_url: None,
            rows: Vec::new(),
            header: None,
            body: None,
            selected_slot: None,
            focused_slot: None,
            nav: Vec::new(),
            lines: Vec::new(),
            row_menu: None,
            hide_details: false,
            priority_edit: None,
            row_heights: std::mem::take(&mut self.row_heights),
            culling: None,
            name_menu_open: false,
        };
        // Row heights depend on the view and text size, not on the window width: rows truncate
        // their text instead of wrapping.
        let row_layout_key = egui::Id::new((
            "row-heights",
            list.view == ModListView::Normal,
            list.compact_thumbnails,
            ui.ctx().pixels_per_point().to_bits(),
            ui.spacing().item_spacing.y.to_bits(),
            ui.spacing().interact_size.y.to_bits(),
        ));
        let draw_all_rows = self.scroll_to_match || !self.cull_rows;

        let mut ui_profile = |ui: &mut Ui, profile: &mut ModProfile| {
            ctx.culling = Some(RowCulling::new(
                ui,
                std::mem::take(&mut ctx.row_heights),
                row_layout_key,
                draw_all_rows,
            ));
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
            // Rows ask about duplicates and missing dependencies, so index once instead of
            // comparing every pair of mods on every frame.
            let enabled_specs = mint_lib::mod_info::DependencyIndex::new(enabled_specs);

            // Shows the most important tags that fit in `budget`. The rest stay in the details panel.
            let ui_mod_tags = |ctx: &mut Ctx, ui: &mut Ui, info: &ModInfo, budget: f32| {
                let Some(tags) = info.modio_tags.as_ref() else {
                    return;
                };
                let list = modio_tags(tags);
                let font = egui::TextStyle::Body.resolve(ui.style());
                let gap = TAG_GAP.max(ui.spacing().item_spacing.x);
                let padding = ui.spacing().button_padding.x * 2.0 + gap;
                let mut used = 0.0;
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for Tag { label, fill, hint } in list {
                        let width = ui.fonts(|f| {
                            f.layout_no_wrap(label.to_owned(), font.clone(), Color32::GRAY)
                                .size()
                                .x
                        }) + padding;
                        used += width;
                        if used > budget {
                            break;
                        }
                        let search = searchable_text(label, &self.search_string, {
                            TextFormat {
                                color: tag_text(ui.visuals(), fill),
                                ..Default::default()
                            }
                        });
                        let mut res = tag_chip(ui, search.job, fill);
                        if let Some(hint) = hint {
                            res = res.on_disabled_hover_text(hint);
                        }
                        if search.is_match && self.scroll_to_match {
                            res.scroll_to_me(None);
                            ctx.scroll_to_match = false;
                        }
                    }
                });
            };

            // Context menu of a mod, shared by its name and the rest of its row.
            let mod_menu = |ctx: &mut Ctx,
                            ui: &mut Ui,
                            spec: &ModSpecification,
                            modio: bool,
                            group: Option<&str>,
                            row_index: usize| {
                // The selected row offers to close the panel instead of reopening it.
                if is_selected(group, row_index) {
                    if ui.button("Hide details").clicked() {
                        ctx.hide_details = true;
                        ui.close_menu();
                    }
                } else if ui.button("Show details").clicked() {
                    ctx.show_details = Some((group.map(str::to_owned), row_index));
                    ui.close_menu();
                }
                if ui.button("Open in browser").clicked() {
                    ctx.open_url = Some(spec.url.clone());
                    ui.close_menu();
                }
                if modio && ui.button("Check for updates").clicked() {
                    ctx.check_update = Some(spec.clone());
                    ui.close_menu();
                }
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
                    let previous_url = mc.spec.url.clone();
                    let dropdown = icons::combo_box("version")
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
                    let selected = info.resolution.url.0.as_str();
                    let latest = self.state.store.get_mod_info(&info.spec);
                    let newer = latest
                        .as_ref()
                        .is_some_and(|m| m.resolution.url.0 != selected);
                    let pending = self.installation_status.rows.get(&mc.spec.url);
                    let hint = if newer {
                        Some(
                            "Latest differs from your pinned version. Choose latest, then click Install mods to use it.",
                        )
                    } else if pending.is_some()
                        && info.provider == "modio"
                        && !self.state.store.is_pinned(&mc.spec)
                    {
                        Some("Latest version is not installed. Click Install mods to apply it.")
                    } else {
                        pending.map(String::as_str)
                    };
                    if let Some(hint) = hint {
                        icons::show(ui, Icon::Update).on_hover_text(hint);
                    } else {
                        ui.allocate_space(egui::vec2(icons::SIZE, icons::SIZE));
                    }

                    if list.priority == PriorityDisplay::Always {
                        ui.scope(|ui| {
                            ui.style_mut().spacing.interact_size.x = PRIORITY_WIDTH;
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
                            let previous = mc.priority;
                            let response = ui
                                .add(priority_value(&mut mc.priority))
                                .on_hover_text_at_pointer(PRIORITY_HINT);
                            if mc.priority != previous {
                                ctx.priority_edit = Some(priority_editing(&response));
                            }
                        });
                    }

                    ctx.needs_save |= previous_url != mc.spec.url;
                    if icons::button(ui, Icon::Copy, "Copy URL").clicked() {
                        ui.ctx().copy_text(mc.spec.url.to_string());
                        ctx.copied_link = true;
                    }

                    if mc.enabled {
                        let is_duplicate = enabled_specs
                            .satisfies_other(&info.spec, |(name, index)| {
                                (group, row_index) == (name.as_deref(), *index)
                            });
                        if is_duplicate
                            && icons::button(ui, Icon::Warning, "Remove duplicate").clicked()
                        {
                            ctx.btn_remove = Some((group.map(str::to_owned), row_index));
                        }

                        let missing_deps = info
                            .suggested_dependencies
                            .iter()
                            .filter(|d| !enabled_specs.provides(d))
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
                } else if icons::button(ui, Icon::Copy, "Copy URL").clicked() {
                    ui.ctx().copy_text(mc.spec.url.to_string());
                    ctx.copied_link = true;
                }

                let details = (show_thumbnails || row_height.is_some())
                    .then(|| self.state.store.get_mod_details(&mc.spec))
                    .flatten();
                let label = info
                    .as_ref()
                    .map_or(mc.spec.url.as_str(), |i| i.name.as_str());
                if show_thumbnails {
                    let texture = details
                        .as_ref()
                        .and_then(|d| d.thumbnail_url.as_deref())
                        .filter(|_| ui.is_rect_visible(ui.max_rect()))
                        .and_then(|url| {
                            self.thumbnails
                                .get(ui.ctx(), url, thumbnails::ThumbnailSize::Small)
                        });
                    let placeholder = if info.is_some() {
                        label
                    } else {
                        short_label(&mc.spec.url)
                    };
                    if thumbnail(
                        ui,
                        thumbnail_size,
                        texture.as_ref(),
                        placeholder,
                        mc.enabled,
                    )
                    .clicked()
                    {
                        ctx.show_details = Some((group.map(str::to_owned), row_index));
                    }
                }

                let name = |ctx: &mut Ctx, ui: &mut Ui| {
                    let search = searchable_text(label, &self.search_string, {
                        TextFormat {
                            color: ui.visuals().hyperlink_color,
                            ..Default::default()
                        }
                    });
                    // A truncating label styled as a link, so long names never run into the tags.
                    let res = ui
                        .add(
                            egui::Label::new(search.job)
                                .sense(egui::Sense::click())
                                .truncate(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Open the mod page\nClick the row to show details");
                    if res.hovered() {
                        ui.painter().hline(
                            res.rect.x_range(),
                            res.rect.bottom(),
                            Stroke::new(1.0_f32, ui.visuals().hyperlink_color),
                        );
                    }
                    if res.clicked() || res.middle_clicked() {
                        ctx.open_url = Some(mc.spec.url.clone());
                    }
                    let modio = info.as_ref().map_or_else(
                        || mc.spec.url.starts_with("https://mod.io/"),
                        |info| info.provider == "modio",
                    );
                    ctx.row_menu = Some((mc.spec.clone(), modio, label.to_owned()));
                    res.context_menu(|ui| mod_menu(ctx, ui, &mc.spec, modio, group, row_index));
                    ctx.name_menu_open = res.context_menu_opened();
                    if search.is_match && self.scroll_to_match {
                        res.scroll_to_me(None);
                        ctx.scroll_to_match = false;
                    }
                };

                if row_height.is_some() {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        name(ctx, ui);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if list.show_tags
                                && let Some(info) = &info
                            {
                                let budget = ui.available_width() - MIN_SUBTITLE_WIDTH;
                                ui_mod_tags(ctx, ui, info, budget);
                            }
                            let subtitle = match (&info, &details) {
                                (None, _) => "Details are not cached yet.".to_owned(),
                                (Some(_), Some(details)) => [
                                    details
                                        .updated
                                        .and_then(details::format_date)
                                        .map(|date| format!("Updated {date}")),
                                    details.author.as_ref().map(|a| format!("by {a}")),
                                    details.summary.clone(),
                                ]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" · "),
                                (Some(info), None) => match info.provider {
                                    "file" => "Local file".to_owned(),
                                    "http" => "Web download".to_owned(),
                                    _ => String::new(),
                                },
                            };
                            if !subtitle.is_empty() {
                                ui.add(
                                    egui::Label::new(RichText::new(subtitle).small().weak())
                                        .truncate(),
                                );
                            }
                        });
                    });
                } else {
                    // Tags are placed from the right first. The name gets the remaining width.
                    // The area also stretches the row to its full width, with or without tags.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if list.show_tags
                            && let Some(info) = &info
                        {
                            let budget = ui.available_width() - MIN_NAME_WIDTH;
                            ui_mod_tags(ctx, ui, info, budget);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            name(ctx, ui);
                        });
                    });
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
                            let header = icons::collapsing_header(group_name.as_str())
                                .open(Some({
                                    let state = group_views.get(&profile_name, group_name);
                                    if !searching {
                                        state.search_closed = false;
                                    }
                                    let hovering = can_drag
                                        && ui.is_enabled()
                                        && drag_drop::can_hover_group(ui.ctx(), &profile_name)
                                        && ui.input(|i| i.pointer.interact_pos()).is_some_and(
                                            |pos| {
                                                ui.clip_rect().contains(pos)
                                                    && ui.ctx().layer_id_at(pos)
                                                        == Some(ui.layer_id())
                                            },
                                        )
                                        && drag_drop::drop_point(ui.ctx()).is_some_and(|point| {
                                            state.rect.is_some_and(|rect| rect.contains(point))
                                        });
                                    if state.hover(hovering, ui.input(|i| i.time)) {
                                        ui.ctx().request_repaint_after(Duration::from_millis(50));
                                    }
                                    // Searching shows matching members without changing
                                    // the saved expansion.
                                    if searching {
                                        !state.search_closed
                                    } else {
                                        state.open
                                    }
                                }))
                                .show(ui, |ui| {
                                    let body_background = ui.painter().add(egui::Shape::Noop);
                                    ctx.body = Some(body_background);
                                    let body_top = ui.cursor().top();
                                    let group = groups.get_mut(group_name).unwrap();
                                    let keys = row_keys(
                                        group.mods.iter().map(|mc| (false, mc.spec.url.clone())),
                                    );
                                    for (visual_index, &index) in
                                        shown_order.groups[group_name].iter().enumerate()
                                    {
                                        ui.push_id(
                                            ("group", group_name.as_str(), &keys[index]),
                                            |ui| {
                                                let row_id = ui.id().with("row");
                                                let entry = ListTarget {
                                                    profile: profile_name.clone(),
                                                    group: Some(group_name.clone()),
                                                    index,
                                                };
                                                let target = ListTarget {
                                                    index: visual_index,
                                                    ..entry.clone()
                                                };
                                                if let Some(height) =
                                                    ctx.culling.as_mut().and_then(|culling| {
                                                        culling.skip(ui, ui.id(), &entry)
                                                    })
                                                {
                                                    let start = ui.painter().add(egui::Shape::Noop);
                                                    let (rect, slot) =
                                                        skipped_row(ui, height, row_id);
                                                    ctx.nav.push((
                                                        Some(group_name.clone()),
                                                        index,
                                                        row_id,
                                                        slot,
                                                    ));
                                                    ctx.rows.push(drag_drop::DropRow {
                                                        rect,
                                                        target,
                                                        group: None,
                                                        parent_index: Some(row_index),
                                                        paint: drag_drop::RowPaint {
                                                            entry,
                                                            key: ui.id(),
                                                            rect,
                                                            start,
                                                            end: start,
                                                            container: None,
                                                        },
                                                    });
                                                    return;
                                                }
                                                let stripe = ui.painter().add(egui::Shape::Noop);
                                                reserve_row(ui, row_id);
                                                ctx.row_menu = None;
                                                let row = egui::Frame::NONE
                                                    .inner_margin(ROW_PADDING)
                                                    .show(ui, |ui| {
                                                        row_layout(ui, row_height, |ui| {
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
                                                if let Some(culling) = &mut ctx.culling {
                                                    culling.measured(
                                                        ui.id(),
                                                        row.response.rect.height(),
                                                    );
                                                }
                                                let mut rect = row.response.rect;
                                                rect.max.x = ui.max_rect().right();
                                                let slot = row_slot(ui, rect);
                                                let hovered = highlight_hover
                                                    && ui.rect_contains_pointer(slot);
                                                let selected =
                                                    is_selected(Some(group_name.as_str()), index);
                                                let fill = if selected {
                                                    Some(selected_fill)
                                                } else if hovered {
                                                    Some(hover_fill)
                                                } else {
                                                    stripe_fill.filter(|_| visual_index % 2 == 1)
                                                };
                                                if let Some(fill) = fill {
                                                    let radius =
                                                        if selected { SELECTED_RADIUS } else { 0 };
                                                    ui.painter().set(
                                                        stripe,
                                                        egui::Shape::rect_filled(
                                                            slot, radius, fill,
                                                        ),
                                                    );
                                                }
                                                if row_lines {
                                                    ctx.lines.push(row_line(ui, rect));
                                                }
                                                if selected {
                                                    ctx.selected_slot = Some(slot);
                                                }
                                                let response =
                                                    ui.interact(slot, row_id, egui::Sense::click());
                                                row_focus(
                                                    ui,
                                                    &response,
                                                    &mut ctx.focused_slot,
                                                    slot,
                                                    ctx.row_menu.as_ref(),
                                                );
                                                ctx.nav.push((
                                                    Some(group_name.clone()),
                                                    index,
                                                    response.id,
                                                    slot,
                                                ));
                                                if response.clicked() {
                                                    ctx.show_details =
                                                        Some((Some(group_name.clone()), index));
                                                }
                                                if let Some((spec, modio, _)) = ctx.row_menu.take()
                                                {
                                                    response.context_menu(|ui| {
                                                        mod_menu(
                                                            ctx,
                                                            ui,
                                                            &spec,
                                                            modio,
                                                            Some(group_name),
                                                            index,
                                                        )
                                                    });
                                                }
                                                if (std::mem::take(&mut ctx.name_menu_open)
                                                    || response.context_menu_opened())
                                                    && let Some(culling) = &mut ctx.culling
                                                {
                                                    culling.menu_open(ui.id());
                                                }
                                                let end = ui.painter().add(egui::Shape::Noop);
                                                ctx.rows.push(drag_drop::DropRow {
                                                    rect,
                                                    target,
                                                    group: None,
                                                    parent_index: Some(row_index),
                                                    paint: drag_drop::RowPaint {
                                                        entry,
                                                        key: ui.id(),
                                                        rect,
                                                        start: stripe,
                                                        end,
                                                        container: None,
                                                    },
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
                            let noun = if count == 1 { "mod" } else { "mods" };
                            let count_label = match group_counts.get(group_name.as_str()) {
                                Some((shown, _)) => format!("({shown}/{count} {noun})"),
                                None => format!("({count} {noun})"),
                            };
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
                                let state = group_views.get(&profile_name, group_name);
                                if searching {
                                    state.search_closed ^= true;
                                } else {
                                    state.toggle();
                                }
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
            for (visual_index, &store_index) in shown_order.profile.iter().enumerate() {
                let group_name = match &profile.mods[store_index] {
                    ModOrGroup::Group { group_name, .. } => Some(group_name.clone()),
                    _ => None,
                };
                let frame = egui::Frame::NONE.inner_margin(ROW_PADDING);
                // Rows paint their stripe, hover or group color here, behind the frame's content.
                let background = ui.painter().add(egui::Shape::Noop);
                let row_id = egui::Id::new((profile_name.as_str(), &keys[store_index], "row"));
                let paint_key = egui::Id::new((profile_name.as_str(), &keys[store_index]));
                let entry = ListTarget {
                    profile: profile_name.clone(),
                    group: None,
                    index: store_index,
                };
                let target = ListTarget {
                    index: visual_index,
                    ..entry.clone()
                };
                if group_name.is_none()
                    && let Some(height) = ctx
                        .culling
                        .as_mut()
                        .and_then(|culling| culling.skip(ui, paint_key, &entry))
                {
                    let (rect, slot) = ui
                        .push_id((profile_name.as_str(), &keys[store_index]), |ui| {
                            skipped_row(ui, height, row_id)
                        })
                        .inner;
                    ctx.nav.push((None, store_index, row_id, slot));
                    ctx.rows.push(drag_drop::DropRow {
                        rect,
                        target,
                        group: None,
                        parent_index: None,
                        paint: drag_drop::RowPaint {
                            entry,
                            key: paint_key,
                            rect,
                            start: background,
                            end: background,
                            container: None,
                        },
                    });
                    continue;
                }
                if group_name.is_none() {
                    reserve_row(ui, row_id);
                }
                ctx.row_menu = None;
                ctx.header = None;
                ctx.body = None;
                let row = ui.push_id((profile_name.as_str(), &keys[store_index]), |ui| {
                    frame.show(ui, |ui| {
                        if group_name.is_some() {
                            ui.set_min_width(ui.available_width());
                        }
                        let height = row_height.filter(|_| group_name.is_none());
                        row_layout(ui, height, |ui| {
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
                if let Some(culling) = &mut ctx.culling {
                    culling.measured(paint_key, row.response.rect.height());
                }
                let selected = group_name.is_none() && is_selected(None, store_index);
                let fill = match &group_name {
                    Some(name) => Some(group_fills[name]),
                    None if selected => Some(selected_fill),
                    None if highlight_hover
                        && ui.rect_contains_pointer(row_slot(ui, row.response.rect)) =>
                    {
                        Some(hover_fill)
                    }
                    None => stripe_fill.filter(|_| visual_index % 2 == 1),
                };
                if let Some(fill) = fill {
                    // Mod rows fill half the spacing on each side, like group members, so
                    // stripes meet without a gap. Group headers keep their separate bars.
                    let (rect, radius) = match group_name {
                        Some(_) => (row.response.rect, 3),
                        None if selected => (row_slot(ui, row.response.rect), SELECTED_RADIUS),
                        None => (row_slot(ui, row.response.rect), 0),
                    };
                    ui.painter()
                        .set(background, egui::Shape::rect_filled(rect, radius, fill));
                }
                let mut rect = row.response.rect;
                rect.max.x = ui.max_rect().right();
                let full_rect = rect;
                if row_lines && visual_index > 0 {
                    ctx.lines.push(row_line(ui, rect));
                }
                if selected {
                    ctx.selected_slot = Some(row_slot(ui, rect));
                }
                if group_name.is_none() {
                    let slot = row_slot(ui, rect);
                    let response = ui.interact(slot, row_id, egui::Sense::click());
                    row_focus(
                        ui,
                        &response,
                        &mut ctx.focused_slot,
                        slot,
                        ctx.row_menu.as_ref(),
                    );
                    ctx.nav.push((None, store_index, response.id, slot));
                    if response.clicked() {
                        ctx.show_details = Some((None, store_index));
                    }
                    if let Some((spec, modio, _)) = ctx.row_menu.take() {
                        response.context_menu(|ui| {
                            mod_menu(&mut ctx, ui, &spec, modio, None, store_index)
                        });
                    }
                    if (std::mem::take(&mut ctx.name_menu_open) || response.context_menu_opened())
                        && let Some(culling) = &mut ctx.culling
                    {
                        culling.menu_open(paint_key);
                    }
                }
                let container = group_name.as_ref().map(|name| {
                    group_views.get(&profile_name, name).rect = Some(rect);
                    let line = ui.painter().add(egui::Shape::line_segment(
                        [rect.left_top(), rect.left_bottom()],
                        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    ));
                    let shapes = [Some(background), ctx.body.take(), Some(line)];
                    (name.clone(), shapes.into_iter().flatten().collect())
                });
                let end = ui.painter().add(egui::Shape::Noop);
                let group = ctx.header.take().map(|(header, name, len)| {
                    rect.min.y = header.top();
                    rect.max.y = header.bottom();
                    (name, len)
                });
                ctx.rows.push(drag_drop::DropRow {
                    rect,
                    target,
                    group,
                    parent_index: None,
                    paint: drag_drop::RowPaint {
                        entry,
                        key: paint_key,
                        rect: full_rect,
                        start: background,
                        end,
                        container,
                    },
                });
            }
            if arrow_nav && let Some(next) = arrow_step(ui, &ctx.nav, selected.as_ref()) {
                let (group, index, id, slot) = ctx.nav[next].clone();
                // Keyboard focus on a row follows the selection.
                if ui
                    .memory(|memory| memory.focused())
                    .is_some_and(|focused| ctx.nav.iter().any(|row| row.2 == focused))
                {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                ui.scroll_to_rect(slot, None);
                ctx.show_details = Some((group, index));
            }
            // While dragging, the selected row's fill moves with it but the outline would not.
            // The keyboard-focused row gets the same outline.
            if !egui::DragAndDrop::has_any_payload(ui.ctx()) {
                for slot in [ctx.selected_slot.take(), ctx.focused_slot.take()]
                    .into_iter()
                    .flatten()
                {
                    selection_outline(ui, slot);
                    // The outline already separates the row, and a row line on the same edge would
                    // make that edge look thicker.
                    for &(line, y) in &ctx.lines {
                        if (y - slot.top()).abs() < 1.0 || (y - slot.bottom()).abs() < 1.0 {
                            ui.painter().set(line, egui::Shape::Noop);
                        }
                    }
                }
            }
            if let Some(culling) = ctx.culling.take() {
                ctx.row_heights = culling.next;
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
        // Keep the list's scroll bar clear of the details panel's resize handle.
        let panel_gap = if selected.is_some() { 12.0 } else { 0.0 };
        // The list fills the remaining space instead of shrinking to its rows. A list that
        // empties (for example a search without matches) would otherwise collapse to zero height
        // while its fading scroll bar can still be dragged, which egui cannot scroll.
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .max_width(ui.available_width() - panel_gap)
            .show(ui, |ui| {
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
        self.row_heights = std::mem::take(&mut ctx.row_heights);
        details::remember_focus(ui.ctx());

        if let Some((id, editing)) = ctx.priority_edit {
            self.priority_changed(id, editing);
        }
        if ctx.needs_save {
            self.save_mod_data("Edit mod settings");
        }
        self.finish_priority_edit(ui.ctx());
        if ctx.copied_link {
            self.notifications.success("Copied mod link.");
        }
        if ctx.hide_details {
            self.details = None;
        }
        if let Some((group, index)) = ctx.show_details
            && let Some(url) = self.entry_url(&profile_name, group.as_deref(), index)
        {
            self.details = Some(details::DetailsTarget {
                target: ListTarget {
                    profile: profile_name.clone(),
                    group,
                    index,
                },
                url,
            });
        }
        if let Some(url) = ctx.open_url {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        if let Some(spec) = ctx.check_update {
            message::UpdateCache::send_specs(self, ui.ctx(), vec![spec]);
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

impl App {
    /// Compact/Normal switch beside the sort controls. Saved immediately as a view setting.
    pub(super) fn ui_view_controls(&mut self, ui: &mut Ui) {
        let mut view = self.state.config.mod_list.view;
        ui.label("View:");
        if icons::segmented(ui, "mod-list-view", &mut view, &VIEW_SEGMENTS) {
            self.state.config.mod_list.view = view;
            self.report_save(self.state.config.save());
        }
    }
}
