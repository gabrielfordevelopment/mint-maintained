use std::collections::HashMap;

#[derive(Default)]
pub(super) struct GroupViews(HashMap<(String, String), GroupView>);

#[derive(Default)]
pub(super) struct GroupView {
    pub open: bool,
    pub rect: Option<egui::Rect>,
    hover_since: Option<f64>,
    temporary: bool,
}

impl GroupViews {
    pub fn get(&mut self, profile: &str, group: &str) -> &mut GroupView {
        self.0.entry((profile.into(), group.into())).or_default()
    }

    pub fn open(&mut self, profile: &str, group: &str) {
        let state = self.get(profile, group);
        state.open = true;
        state.temporary = false;
        state.hover_since = None;
    }
}

impl GroupView {
    pub fn hover(&mut self, hovering: bool, now: f64) -> bool {
        if !hovering {
            if self.temporary {
                self.open = false;
            }
            self.temporary = false;
            self.hover_since = None;
        } else if !self.open {
            let since = *self.hover_since.get_or_insert(now);
            if now - since >= 0.5 {
                self.open = true;
                self.temporary = true;
            }
        }
        hovering && !self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.temporary = false;
        self.hover_since = None;
    }
}
