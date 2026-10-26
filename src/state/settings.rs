#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GuiTheme {
    Light,
    Dark,
}

#[derive(PartialEq, Debug, strum::EnumIter, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum SortBy {
    Enabled,
    Name,
    Priority,
    Provider,
    RequiredStatus,
    ApprovalCategory,
    /// Upload time of the newest cached mod file. Mods without it sort first.
    LastUpdated,
}

/// Row layout of the mod list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModListView {
    #[default]
    Compact,
    Normal,
}

/// When a row shows its load priority control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityDisplay {
    /// `non_zero` was written by unreleased development builds. It reads as the default.
    #[default]
    #[serde(alias = "non_zero")]
    Always,
    /// Edited in the details panel only.
    Hidden,
}

/// Contrast of the alternating mod row background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStriping {
    Off,
    /// The theme's faint background, used before this setting existed.
    Subtle,
    #[default]
    Medium,
    Strong,
}

/// Presentation choices for the mod list. They never change saved mod data.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ModListSettings {
    pub view: ModListView,
    pub compact_thumbnails: bool,
    pub normal_thumbnails: bool,
    /// Allows downloading mod.io thumbnails. When false no thumbnail request is made.
    pub download_thumbnails: bool,
    pub priority: PriorityDisplay,
    pub show_tags: bool,
    pub striping: RowStriping,
    /// Draws a thin line between rows, with or without striping.
    pub row_lines: bool,
    pub details_width: Option<f32>,
}

impl Default for ModListSettings {
    fn default() -> Self {
        Self {
            view: ModListView::Compact,
            compact_thumbnails: false,
            normal_thumbnails: true,
            download_thumbnails: true,
            priority: PriorityDisplay::Always,
            show_tags: true,
            striping: RowStriping::Medium,
            row_lines: false,
            details_width: None,
        }
    }
}

impl ModListSettings {
    pub fn thumbnails(&self) -> bool {
        self.download_thumbnails
            && match self.view {
                ModListView::Compact => self.compact_thumbnails,
                ModListView::Normal => self.normal_thumbnails,
            }
    }
}
