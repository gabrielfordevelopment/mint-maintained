use egui::emath::TSTransform;
use egui::layers::{PaintList, ShapeIdx};

use super::{Icon, ModData, SortBy, icons};
use crate::state::edits::{ListTarget, PreparedEdit, VisibleOrder};

/// Stroke width of the space opened at the drop position.
pub(super) const GAP_STROKE: f32 = 1.5;

#[derive(Clone)]
pub(super) struct DragSnapshot {
    pub before: ModData,
    pub order: VisibleOrder,
    pub sorting: Option<(SortBy, bool)>,
}

pub(super) struct DraggedEntry {
    snapshot: DragSnapshot,
    source: ListTarget,
    is_group: bool,
    label: String,
    /// Identifies this drag, so preview animations always start from the unchanged layout.
    session: egui::Id,
    /// Distance from the top of the drag handle to the point where it was grabbed.
    grab: f32,
}

pub(super) struct DropRow {
    pub rect: egui::Rect,
    pub target: ListTarget,
    pub group: Option<(String, usize)>,
    pub parent_index: Option<usize>,
    pub paint: RowPaint,
}

/// Shapes painted for a row. The drop preview moves them without changing the layout, so drop
/// targets keep using the rows' real positions.
pub(super) struct RowPaint {
    /// Stored position of the entry, compared with the dragged source.
    pub entry: ListTarget,
    pub key: egui::Id,
    /// The whole row, including the members of an expanded group.
    pub rect: egui::Rect,
    pub start: ShapeIdx,
    pub end: ShapeIdx,
    /// Group rows: the group's name and the backgrounds that grow or shrink with its members.
    pub container: Option<(String, Vec<ShapeIdx>)>,
}

/// Where a release would drop the dragged entry.
struct Drop {
    destination: ListTarget,
    rect: egui::Rect,
    after: bool,
    /// Index of the group row that receives the entry at its end.
    into_group: Option<usize>,
}

struct Preview {
    /// Current vertical offset of each row, empty when the dragged row is not in this list.
    offsets: Vec<f32>,
    /// The space opened at the drop position.
    gap: Option<egui::Rect>,
}

pub(super) fn can_hover_group(ctx: &egui::Context, profile: &str) -> bool {
    egui::DragAndDrop::payload::<DraggedEntry>(ctx)
        .is_some_and(|payload| !payload.is_group && payload.source.profile == profile)
}

/// The point that picks drop targets: where the dragged row is shown, mapped to the list's
/// layout. It trails the pointer by one frame and is the pointer before the first preview.
pub(super) fn drop_point(ctx: &egui::Context) -> Option<egui::Pos2> {
    let pointer = ctx.input(|i| i.pointer.interact_pos())?;
    let probe = egui::DragAndDrop::payload::<DraggedEntry>(ctx)
        .and_then(|payload| ctx.data(|data| data.get_temp(payload.session.with("probe"))));
    Some(probe.unwrap_or(pointer))
}

pub(super) fn handle(
    ui: &mut egui::Ui,
    source: ListTarget,
    snapshot: Option<&DragSnapshot>,
    is_group: bool,
    label: &str,
    enabled: bool,
) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(26.0, 18.0),
        if enabled {
            egui::Sense::drag()
        } else {
            egui::Sense::hover()
        },
    );
    Icon::Drag.image(ui, ui.visuals().text_color()).paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(icons::SIZE)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled() && enabled,
            format!("Drag {label}"),
        )
    });
    if enabled
        && ui.is_enabled()
        && response.drag_started()
        && let Some(snapshot) = snapshot
    {
        let press = ui
            .input(|i| i.pointer.press_origin())
            .unwrap_or(rect.center());
        egui::DragAndDrop::set_payload(
            ui.ctx(),
            DraggedEntry {
                snapshot: snapshot.clone(),
                source: source.clone(),
                is_group,
                label: std::path::Path::new(label)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(label)
                    .into(),
                session: egui::Id::new(("mod-drag", ui.input(|i| i.time).to_bits())),
                grab: press.y - rect.top(),
            },
        );
    }
    // The preview lifts the dragged row by how far the pointer moved its handle.
    if response.dragged()
        && let Some(payload) = egui::DragAndDrop::payload::<DraggedEntry>(ui.ctx())
        && payload.source == source
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
    {
        ui.data_mut(|data| {
            data.insert_temp(
                payload.session.with("lift"),
                pos.y - payload.grab - rect.top(),
            )
        });
    }
    if response.hovered() {
        if enabled {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        response.on_hover_text(if enabled {
            "Drag to reorder or move between groups; a successful move switches to Manual"
        } else {
            "Arrange mixed profiles before dragging; wait for any running operation to finish"
        });
    }
}

pub(super) fn finish(
    ui: &mut egui::Ui,
    rows: &[DropRow],
    profile: &str,
    end_index: usize,
    enabled: bool,
    order: &VisibleOrder,
    sorting: Option<(SortBy, bool)>,
) -> Option<PreparedEdit> {
    let payload = egui::DragAndDrop::payload::<DraggedEntry>(ui.ctx())?;
    if !enabled || !ui.is_enabled() || payload.source.profile != profile {
        return None;
    }
    if payload.snapshot.order != *order || payload.snapshot.sorting != sorting {
        return None;
    }
    let pos = ui.input(|i| i.pointer.interact_pos())?;
    let inside = ui.clip_rect().contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id());
    let probe = egui::pos2(pos.x, probe_y(ui, rows, &payload).unwrap_or(pos.y));
    ui.data_mut(|data| data.insert_temp(payload.session.with("probe"), probe));
    let drop = inside
        .then(|| locate(ui, rows, &payload, profile, end_index, probe))
        .flatten();
    let preview = preview(ui, rows, &payload, drop.as_ref());
    let Drop {
        destination,
        rect,
        after,
        into_group,
    } = drop?;
    let color = ui.visuals().selection.stroke.color;
    if let Some(row) = into_group {
        let rect = rect.translate(egui::vec2(
            0.0,
            preview.offsets.get(row).copied().unwrap_or_default(),
        ));
        ui.painter()
            .rect_filled(rect, 3, ui.visuals().selection.bg_fill.gamma_multiply(0.35));
        ui.painter().rect_stroke(
            rect,
            3,
            egui::Stroke::new(2.0_f32, color),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(gap) = preview.gap {
        // The opened space shows where the entry lands.
        ui.painter()
            .rect_filled(gap, 3, ui.visuals().selection.bg_fill.gamma_multiply(0.2));
        ui.painter().rect_stroke(
            gap,
            3,
            egui::Stroke::new(GAP_STROKE, color),
            egui::StrokeKind::Inside,
        );
    } else if preview.offsets.is_empty() && into_group.is_none() {
        let y = if after { rect.bottom() } else { rect.top() };
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(2.0_f32, color),
        );
    }
    let hint = match &destination.group {
        Some(name) => format!("{} → {name} (shared group)", payload.label),
        None if payload.is_group => format!("{} → groups", payload.label),
        None => format!("{} → standalone mods", payload.label),
    };
    egui::Area::new(egui::Id::new("drag-hint"))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(pos + egui::vec2(16.0, 18.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(hint);
            });
        });
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if ui.input(|i| i.pointer.primary_released()) {
        egui::DragAndDrop::clear_payload(ui.ctx());
        return Some(PreparedEdit::move_in_visible_order(
            &payload.snapshot.before,
            payload.source.clone(),
            destination,
            payload.snapshot.order.clone(),
        ));
    }
    if pos.y > ui.clip_rect().bottom() - 24.0 {
        ui.scroll_with_delta(egui::vec2(0.0, -6.0));
        ui.ctx().request_repaint();
    }
    if pos.y < ui.clip_rect().top() + 24.0 {
        ui.scroll_with_delta(egui::vec2(0.0, 6.0));
        ui.ctx().request_repaint();
    }
    None
}

fn locate(
    ui: &egui::Ui,
    rows: &[DropRow],
    payload: &DraggedEntry,
    profile: &str,
    end_index: usize,
    pos: egui::Pos2,
) -> Option<Drop> {
    let mut target = None;
    let mut highlight = None;
    let mut into_group = None;
    // Child rows are registered before their containing group row.
    for (row_index, row) in rows.iter().enumerate() {
        if pos.y < row.rect.top() - 1.5 || pos.y > row.rect.bottom() + 1.5 {
            continue;
        }
        let mut destination = row.target.clone();
        let mut after = pos.y >= row.rect.center().y;
        if let Some(parent) = row.parent_index {
            if pos.x < row.rect.left() {
                destination.group = None;
                destination.index = parent + 1;
            } else {
                destination.index += usize::from(after);
            }
        } else if let Some((name, length)) = &row.group
            && !payload.is_group
            && pos.x >= row.rect.left() + 60.0
            && pos.y > row.rect.top() + 3.0
            && pos.y < row.rect.bottom() - 3.0
        {
            destination.group = Some(name.clone());
            destination.index = *length;
            into_group = Some(row_index);
        } else {
            destination.index += usize::from(after);
        }
        if payload.is_group && destination.group.is_some() {
            return None;
        }
        let mut rect = row.rect;
        if destination.group.is_none() {
            rect.min.x = ui.max_rect().left();
            if let Some(parent) = row.parent_index {
                let bottom = rows
                    .iter()
                    .filter(|r| r.parent_index == Some(parent))
                    .map(|r| r.rect.bottom())
                    .fold(rect.bottom(), f32::max);
                rect.min.y = bottom;
                rect.max.y = bottom;
                after = true;
            }
        }
        highlight = Some((rect, after));
        target = Some(destination);
        break;
    }
    if target.is_none() && rows.iter().all(|row| pos.y >= row.rect.bottom()) {
        target = Some(ListTarget {
            profile: profile.into(),
            group: None,
            index: end_index,
        });
        highlight = Some((
            egui::Rect::from_min_size(
                egui::pos2(
                    ui.max_rect().left(),
                    rows.iter()
                        .map(|r| r.rect.bottom())
                        .fold(ui.max_rect().top(), f32::max),
                ),
                egui::vec2(ui.available_width(), 0.0),
            ),
            true,
        ));
    }
    let mut destination = target?;
    let (mut rect, mut after) = highlight?;
    if destination.group.is_none() {
        let profile = payload.snapshot.before.profiles.get(profile)?;
        let index = profile.insertion_index(destination.index, payload.is_group);
        if index != destination.index {
            destination.index = index;
            if let Some(first_mod) = rows.iter().find(|row| {
                row.parent_index.is_none() && row.target.index == profile.standalone_start()
            }) {
                rect = first_mod.rect;
                after = false;
            } else {
                let bottom = rows
                    .iter()
                    .map(|row| row.rect.bottom())
                    .fold(ui.max_rect().top(), f32::max);
                rect.min.y = bottom;
                rect.max.y = bottom;
                after = true;
            }
            rect.min.x = ui.max_rect().left();
        }
    }
    Some(Drop {
        destination,
        rect,
        after,
        into_group,
    })
}

/// Where the lifted row's top edge falls in the list's unchanged layout.
///
/// The preview shows the list without the lifted row, so rows below it appear `height` higher
/// than they are laid out. Mapping the lifted row back to the layout lets drop targets follow
/// what is on screen. The top edge decides: over the upper half of a row, the lifted row takes
/// that row's place, so it always covers at least half of the space that opens.
fn probe_y(ui: &egui::Ui, rows: &[DropRow], payload: &DraggedEntry) -> Option<f32> {
    let source = rows.iter().find(|row| row.paint.entry == payload.source)?;
    let lift = ui.data(|data| data.get_temp::<f32>(payload.session.with("lift")))?;
    let top = source.paint.rect.top();
    let spacing = ui.spacing().item_spacing.y;
    let height = source.paint.rect.height() + spacing;
    // Rows split at their centers. Half the spacing moves that split to the middle of the slot,
    // spacing included.
    Some(lifted_to_layout(top + lift, top, height) - spacing / 2.0)
}

/// Maps a position in the list shown without the lifted row (whose slot started at `top` and
/// was `height` tall including spacing) back to the unchanged layout.
pub(super) fn lifted_to_layout(y: f32, top: f32, height: f32) -> f32 {
    if y < top { y } else { y + height }
}

/// Vertical offset of a row centered at `y` once the dragged row (centered at `source`,
/// `height` tall including spacing) is lifted out and room is made below `insert`.
pub(super) fn preview_shift(y: f32, source: f32, insert: Option<f32>, height: f32) -> f32 {
    let mut shift = 0.0;
    if y > source {
        shift -= height;
    }
    if insert.is_some_and(|insert| y > insert) {
        shift += height;
    }
    shift
}

fn translate(list: &mut PaintList, start: ShapeIdx, end: ShapeIdx, dy: f32, clip: egui::Rect) {
    if dy == 0.0 {
        return;
    }
    let offset = egui::vec2(0.0, dy);
    for index in start.0..end.0 {
        list.mutate_shape(ShapeIdx(index), |shape| {
            shape.clip_rect = shape.clip_rect.translate(offset).intersect(clip);
            shape.shape.transform(TSTransform::from_translation(offset));
        });
    }
}

/// Lifts the dragged row so it follows the pointer and slides the other rows to where they
/// will be after the drop. Only painted shapes move, never the layout or drop targets.
fn preview(
    ui: &egui::Ui,
    rows: &[DropRow],
    payload: &DraggedEntry,
    drop: Option<&Drop>,
) -> Preview {
    let Some(source) = rows.iter().find(|row| row.paint.entry == payload.source) else {
        return Preview {
            offsets: Vec::new(),
            gap: None,
        };
    };
    let spacing = ui.spacing().item_spacing.y;
    let height = source.paint.rect.height() + spacing;
    let source_y = source.rect.center().y;
    let destination_group = drop.and_then(|drop| drop.destination.group.as_deref());
    // Rows below this boundary move down. Appending to a group opens space after its last
    // visible member, while a collapsed group is only highlighted.
    let insert = drop.and_then(|drop| match drop.into_group {
        Some(_) => rows
            .iter()
            .filter(|row| row.target.group.as_deref() == destination_group)
            .map(|row| row.rect.bottom() + spacing / 2.0)
            .reduce(f32::max),
        None if drop.after => Some(drop.rect.bottom() + spacing / 2.0),
        None => Some(drop.rect.top() - spacing / 2.0),
    });
    let ctx = ui.ctx();
    let time = ui.style().animation_time;
    let clip = ui.clip_rect();
    let lifted = |row: &DropRow| {
        row.paint.start.0 >= source.paint.start.0 && row.paint.end.0 <= source.paint.end.0
    };
    let offsets: Vec<f32> = rows
        .iter()
        .map(|row| {
            if lifted(row) {
                return 0.0;
            }
            let target = preview_shift(row.rect.center().y, source_y, insert, height);
            ctx.animate_value_with_time(payload.session.with(row.paint.key), target, time)
        })
        .collect();
    let grows: Vec<f32> = rows
        .iter()
        .map(|row| {
            let Some((name, _)) = &row.paint.container else {
                return 0.0;
            };
            let mut grow = 0.0;
            if destination_group == Some(name.as_str()) && insert.is_some() {
                grow += height;
            }
            if source.paint.entry.group.as_ref() == Some(name) {
                grow -= height;
            }
            ctx.animate_value_with_time(payload.session.with((row.paint.key, "grow")), grow, time)
        })
        .collect();
    let lift = ui
        .data(|data| data.get_temp::<f32>(payload.session.with("lift")))
        .unwrap_or_default();
    let ghost = source.paint.rect.translate(egui::vec2(0.0, lift));
    let ghost_layer = egui::LayerId::new(egui::Order::Foreground, payload.session.with("ghost"));
    let visuals = ui.visuals();

    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(ui.layer_id());
        let mut lifted_shapes = Vec::new();
        for index in source.paint.start.0..source.paint.end.0 {
            list.mutate_shape(ShapeIdx(index), |shape| {
                let taken = std::mem::replace(&mut shape.shape, egui::Shape::Noop);
                lifted_shapes.push((shape.clip_rect, taken));
            });
        }
        for (index, row) in rows.iter().enumerate() {
            // Group members are painted inside their group row, which already moves them.
            let parent = row
                .parent_index
                .and_then(|parent| {
                    rows.iter()
                        .position(|r| r.parent_index.is_none() && r.paint.entry.index == parent)
                })
                .map_or(0.0, |parent| offsets[parent]);
            translate(
                list,
                row.paint.start,
                row.paint.end,
                offsets[index] - parent,
                clip,
            );
            if let Some((_, shapes)) = &row.paint.container
                && grows[index] != 0.0
            {
                for &shape in shapes {
                    list.mutate_shape(shape, |shape| match &mut shape.shape {
                        egui::Shape::Rect(rect) => rect.rect.max.y += grows[index],
                        egui::Shape::LineSegment { points, .. } => points[1].y += grows[index],
                        _ => {}
                    });
                }
            }
        }

        let ghost_list = graphics.entry(ghost_layer);
        ghost_list.add(clip, visuals.popup_shadow.as_shape(ghost, 3).into());
        ghost_list.add(clip, egui::Shape::rect_filled(ghost, 3, visuals.panel_fill));
        let offset = egui::vec2(0.0, lift);
        for (clip_rect, mut shape) in lifted_shapes {
            shape.transform(TSTransform::from_translation(offset));
            ghost_list.add(clip_rect.translate(offset).intersect(clip), shape);
        }
        ghost_list.add(
            clip,
            egui::Shape::rect_stroke(
                ghost,
                3,
                egui::Stroke::new(1.0_f32, visuals.selection.stroke.color),
                egui::StrokeKind::Outside,
            ),
        );
    });

    let gap = insert.map(|insert| {
        let top = insert + preview_shift(insert, source_y, None, height) + spacing / 2.0;
        let top = ctx.animate_value_with_time(payload.session.with("gap"), top, time);
        let x = match destination_group {
            Some(name) => rows
                .iter()
                .find(|row| row.target.group.as_deref() == Some(name))
                .map_or(source.paint.rect.x_range(), |row| row.rect.x_range()),
            None => ui.max_rect().x_range(),
        };
        egui::Rect::from_x_y_ranges(x, top..=top + source.paint.rect.height())
    });
    Preview { offsets, gap }
}
