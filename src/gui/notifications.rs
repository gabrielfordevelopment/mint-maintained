use std::collections::VecDeque;

use super::{App, LastActionStatus, diagnostics, icons::Icon};

const LIFETIME: f64 = 10.0;
const FADE: f64 = 0.18;

struct Notification {
    text: String,
    error: bool,
    started: Option<f64>,
    closing: Option<f64>,
    paused_at: Option<f64>,
    paused_total: f64,
    undo_id: Option<u64>,
}

impl Notification {
    fn age(&self, now: f64) -> f64 {
        self.paused_at.unwrap_or(now) - self.started.unwrap_or(now) - self.paused_total
    }

    fn hover(&mut self, now: f64, hovered: bool) {
        match (self.paused_at, hovered) {
            (None, true) => self.paused_at = Some(now),
            (Some(start), false) => {
                self.paused_total += now - start;
                self.paused_at = None;
            }
            _ => {}
        }
    }

    fn progress(&self, now: f64) -> f32 {
        (self.age(now) / LIFETIME).clamp(0.0, 1.0) as f32
    }

    fn opacity(&self, now: f64) -> f32 {
        let age = now - self.started.unwrap_or(now);
        let fade_out = self
            .closing
            .map(|time| (now - time) / FADE)
            .unwrap_or_else(|| {
                if self.error {
                    0.0
                } else {
                    (self.age(now) - LIFETIME) / FADE
                }
            });
        ((age / FADE).min(1.0) * (1.0 - fade_out.clamp(0.0, 1.0))) as f32
    }

    fn expired(&self, now: f64) -> bool {
        self.closing.is_some_and(|time| now >= time + FADE)
            || (!self.error && self.age(now) >= LIFETIME + FADE)
    }
}

#[derive(Default)]
pub(super) struct Notifications {
    queue: VecDeque<Notification>,
    details: Option<String>,
}

impl Notifications {
    pub(super) fn dismiss_edit(&mut self, id: u64) {
        self.queue
            .retain(|notification| notification.undo_id != Some(id));
    }

    pub(super) fn success(&mut self, text: impl Into<String>) {
        self.push(text.into(), false, None);
    }

    pub(super) fn success_with_undo(&mut self, text: String, undo_id: Option<u64>) {
        self.push(text, false, undo_id);
    }

    fn push(&mut self, text: String, error: bool, undo_id: Option<u64>) {
        if let Some(existing) = self
            .queue
            .iter_mut()
            .find(|n| n.text == text && n.error == error && n.undo_id == undo_id)
        {
            existing.started = None;
            existing.closing = None;
            existing.paused_at = None;
            existing.paused_total = 0.0;
            return;
        }
        let notification = Notification {
            text,
            error,
            started: None,
            closing: None,
            paused_at: None,
            paused_total: 0.0,
            undo_id,
        };
        if error {
            if let Some(current) = self.queue.front_mut() {
                current.started = None;
                current.paused_at = None;
                current.paused_total = 0.0;
            }
            self.queue.push_front(notification);
        } else {
            if self.queue.iter().filter(|n| !n.error).count() >= 4
                && let Some(index) = self.queue.iter().rposition(|n| !n.error)
            {
                self.queue.remove(index);
            }
            self.queue.push_back(notification);
        }
    }

    pub(super) fn show(&mut self, ctx: &egui::Context, available_undo: Option<u64>) -> Option<u64> {
        let now = ctx.input(|i| i.time);
        let id = egui::Id::new("notification");
        let hovered = ctx.pointer_hover_pos().is_some_and(|pos| {
            ctx.memory(|m| m.area_rect(id))
                .is_some_and(|r| r.contains(pos))
                && ctx.layer_id_at(pos) == Some(egui::LayerId::new(egui::Order::Foreground, id))
        });
        if let Some(notification) = self.queue.front_mut() {
            notification.hover(now, hovered);
        }
        let mut undo = None;
        if self.queue.front().is_some_and(|n| n.expired(now)) {
            self.queue.pop_front();
        }
        if let Some(notification) = self.queue.front_mut() {
            notification.started.get_or_insert(now);
            let opacity = notification.opacity(now);
            egui::Area::new(id)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -38.0])
                .movable(false)
                .show(ctx, |ui| {
                    ui.set_opacity(opacity);
                    let frame = egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin { left: 44, right: 50, top: 10, bottom: 10 })
                        .show(ui, |ui| {
                            ui.set_width(
                                (460.0_f32.min(ctx.screen_rect().width() - 24.0) - 94.0).max(80.0),
                            );
                            let text = if notification.error {
                                notification.text.lines().next().unwrap_or("Operation failed")
                            } else {
                                &notification.text
                            };
                            ui.add(egui::Label::new(text).wrap());
                            if notification.error && ui.button("Details").clicked() {
                                self.details = Some(notification.text.clone());
                            }
                            if let Some(action) = notification.undo_id
                                && ui.add_enabled(available_undo == Some(action), egui::Button::new("Undo"))
                                    .on_disabled_hover_text("Undo newer edits first, or wait for the current operation to finish.")
                                    .clicked() {
                                undo = Some(action);
                                notification.closing = Some(now);
                            }
                            if !notification.error {
                                ui.add_space(6.0);
                                ui.add(
                                    egui::ProgressBar::new(notification.progress(now))
                                        .desired_height(3.0),
                                );
                            }
                        });
                    let rect = frame.response.rect;
                    let color = if notification.error { ui.visuals().error_fg_color } else { ui.visuals().selection.stroke.color };
                    (if notification.error { Icon::Error } else { Icon::Check })
                        .image_sized(ui, color, 24.0).paint_at(ui,
                            egui::Rect::from_center_size(egui::pos2(rect.left()+24.0, rect.center().y), egui::Vec2::splat(24.0)));
                    let close_rect = egui::Rect::from_center_size(egui::pos2(rect.right()-25.0, rect.center().y), egui::Vec2::splat(32.0));
                    let close = ui.interact(close_rect, ui.id().with("close"), egui::Sense::click());
                    close.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Dismiss notification"));
                    if close.hovered() { ui.painter().rect_filled(close_rect, 3, ui.visuals().widgets.hovered.weak_bg_fill); }
                    if close.has_focus() {
                        ui.painter().rect_stroke(close_rect, 3, ui.visuals().selection.stroke, egui::StrokeKind::Inside);
                    }
                    Icon::Close.image_sized(ui, ui.style().interact(&close).text_color(), 24.0)
                        .paint_at(ui, egui::Rect::from_center_size(close_rect.center(), egui::Vec2::splat(24.0)));
                    if close.on_hover_text("Dismiss notification").clicked() { notification.closing = Some(now); }
                });
            if (!notification.error && !hovered)
                || notification.closing.is_some()
                || now < notification.started.unwrap() + FADE
            {
                ctx.request_repaint();
            }
        }
        if let Some(details) = &self.details {
            let mut open = true;
            egui::Window::new("Notification details")
                .open(&mut open)
                .default_size([520.0, 280.0])
                .show(ctx, |ui| diagnostics::details_ui(ui, details));
            if !open {
                self.details = None;
            }
        }
        undo
    }
}

impl App {
    pub(super) fn show_notifications(&mut self, ctx: &egui::Context) {
        if let Some(action) = &mut self.last_action
            && !action.notified
        {
            action.notified = true;
            let (text, error) = match &action.status {
                LastActionStatus::Success(text) => (text, false),
                LastActionStatus::Failure(text) => (text, true),
            };
            self.notifications.push(
                diagnostics::redact(
                    text,
                    self.state
                        .config
                        .provider_parameters
                        .values()
                        .flat_map(|p| p.values().cloned()),
                ),
                error,
                if error { None } else { action.undo_id },
            );
        }
        let available = self
            .history_available()
            .then(|| self.history.undo_id())
            .flatten();
        if let Some(id) = self.notifications.show(ctx, available) {
            self.restore_history(false, Some(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(center: &mut Notifications, ctx: &egui::Context, time: f64, hover: bool) {
        let pointer = ctx
            .memory(|m| m.area_rect(egui::Id::new("notification")))
            .map(|rect| rect.center())
            .unwrap_or_default();
        let _ = ctx.run(
            egui::RawInput {
                time: Some(time),
                events: if hover {
                    vec![egui::Event::PointerMoved(pointer)]
                } else {
                    vec![egui::Event::PointerGone]
                },
                ..Default::default()
            },
            |ctx| {
                center.show(ctx, None);
            },
        );
    }

    #[test]
    fn notification_progress_pauses_on_hover_and_resumes_remaining_ten_seconds() {
        for dark in [false, true] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let mut center = Notifications::default();
            center.success("Copied 12 active mod links.");
            frame(&mut center, &ctx, 0.0, false);
            assert_eq!(center.queue[0].progress(0.0), 0.0);
            frame(&mut center, &ctx, 0.09, false);
            assert!((center.queue[0].opacity(0.09) - 0.5).abs() < 0.01);
            frame(&mut center, &ctx, 4.0, true);
            assert_eq!(center.queue[0].progress(4.0), 0.4);
            frame(&mut center, &ctx, 40.0, true);
            assert_eq!(center.queue[0].progress(40.0), 0.4);
            frame(&mut center, &ctx, 40.0, false);
            frame(&mut center, &ctx, 46.0, false);
            assert_eq!(center.queue[0].progress(46.0), 1.0);
            frame(&mut center, &ctx, 46.09, false);
            assert!((center.queue[0].opacity(46.09) - 0.5).abs() < 0.01);
            frame(&mut center, &ctx, 46.2, false);
            assert!(center.queue.is_empty());
        }
    }

    #[test]
    fn repeated_copy_renews_one_notification_and_errors_remain_until_dismissed() {
        let ctx = egui::Context::default();
        let mut center = Notifications::default();
        center.success("Copied mod link.");
        frame(&mut center, &ctx, 0.0, false);
        frame(&mut center, &ctx, 7.0, false);
        center.success("Copied mod link.");
        frame(&mut center, &ctx, 7.0, false);
        assert_eq!(center.queue.len(), 1);
        assert_eq!(center.queue[0].progress(7.0), 0.0);
        center.push("Save failed\nOriginal error details".into(), true, None);
        frame(&mut center, &ctx, 8.0, false);
        center.success("Created group.");
        frame(&mut center, &ctx, 100.0, false);
        assert!(center.queue[0].error);
        assert_eq!(center.queue[0].text, "Save failed\nOriginal error details");
        center.queue[0].closing = Some(100.0);
        frame(&mut center, &ctx, 100.2, false);
        assert_eq!(center.queue[0].text, "Copied mod link.");
        assert_eq!(center.queue[0].progress(100.2), 0.0);
    }
}
