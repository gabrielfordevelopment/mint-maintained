use super::{Icon, ModData, SortBy, icons};
use crate::state::edits::{ListTarget, PreparedEdit, VisibleOrder};

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
}

pub(super) struct DropRow {
    pub rect: egui::Rect,
    pub target: ListTarget,
    pub group: Option<(String, usize)>,
    pub parent_index: Option<usize>,
}

pub(super) fn can_hover_group(ctx: &egui::Context, profile: &str) -> bool {
    egui::DragAndDrop::payload::<DraggedEntry>(ctx)
        .is_some_and(|payload| !payload.is_group && payload.source.profile == profile)
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
        egui::DragAndDrop::set_payload(
            ui.ctx(),
            DraggedEntry {
                snapshot: snapshot.clone(),
                source,
                is_group,
                label: std::path::Path::new(label)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(label)
                    .into(),
            },
        );
    }
    if response.hovered() {
        if enabled {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        response.on_hover_text(if enabled {
            "Drag to reorder or move between groups; a successful move switches to Manual"
        } else {
            "Dragging is unavailable while an operation is running"
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
    if !ui.clip_rect().contains(pos) || ui.ctx().layer_id_at(pos) != Some(ui.layer_id()) {
        return None;
    }
    let mut target = None;
    let mut highlight = None;
    let mut into_group = false;
    // Child rows are registered before their containing group row.
    for row in rows {
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
            into_group = true;
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
                egui::vec2(ui.available_width(), 20.0),
            ),
            false,
        ));
    }
    let destination = target?;
    let (rect, after) = highlight?;
    let color = ui.visuals().selection.stroke.color;
    if into_group {
        ui.painter()
            .rect_filled(rect, 3, ui.visuals().selection.bg_fill.gamma_multiply(0.35));
        ui.painter().rect_stroke(
            rect,
            3,
            egui::Stroke::new(2.0_f32, color),
            egui::StrokeKind::Inside,
        );
    } else {
        let y = if after { rect.bottom() } else { rect.top() };
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(2.0_f32, color),
        );
    }
    let hint = match &destination.group {
        Some(name) => format!("{} → {name} (shared group)", payload.label),
        None => format!("{} → profile", payload.label),
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
