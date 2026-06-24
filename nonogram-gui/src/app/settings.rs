use iced::Color;
use iced_fonts::bootstrap::Bootstrap;
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;
use std::path::PathBuf;

use super::style::icon_char;

// ---------------------------------------------------------------------------
// Cell visual settings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct CellVisual {
    pub(crate) color: Color,
    pub(crate) icon: Option<Bootstrap>,
}

impl CellVisual {
    pub(crate) fn icon_color(&self) -> Color {
        let lum = 0.299 * self.color.r + 0.587 * self.color.g + 0.114 * self.color.b;
        if lum > 0.5 {
            Color::from_rgb(0.15, 0.15, 0.15)
        } else {
            Color::WHITE
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CellSettings {
    pub(crate) unknown: CellVisual,
    pub(crate) filled:  CellVisual,
    pub(crate) empty:   CellVisual,
    pub(crate) clue_bg: Color,
    pub(crate) sum_bg:  Color,
}

impl Default for CellSettings {
    fn default() -> Self {
        Self {
            unknown: CellVisual { color: Color::from_rgb(0.72, 0.76, 0.82), icon: None },
            filled:  CellVisual { color: Color::from_rgb(0.10, 0.10, 0.15), icon: None },
            empty:   CellVisual { color: Color::WHITE, icon: None },
            clue_bg: Color::from_rgb(0.97, 0.97, 0.97),
            sum_bg:  Color::from_rgb(0.82, 0.82, 0.82),
        }
    }
}

impl CellSettings {
    pub(crate) fn visual_for(&self, state: nonogram_core::CellState) -> &CellVisual {
        match state {
            nonogram_core::CellState::Unknown => &self.unknown,
            nonogram_core::CellState::Filled  => &self.filled,
            nonogram_core::CellState::Empty   => &self.empty,
        }
    }

    pub(crate) fn visual_for_mut(&mut self, state_idx: u8) -> Option<&mut CellVisual> {
        match state_idx {
            0 => Some(&mut self.unknown),
            1 => Some(&mut self.filled),
            2 => Some(&mut self.empty),
            _ => None,
        }
    }
}

// Master icon list — indices stored in SavedSettings for persistence.
pub(crate) const ICON_OPTIONS: &[Option<Bootstrap>] = &[
    None,
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::CheckLg),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
    Some(Bootstrap::Dot),
];

pub(crate) const FILLED_ICON_OPTIONS: &[Option<Bootstrap>] = &[
    None,
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::CheckLg),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
];

pub(crate) const EMPTY_ICON_OPTIONS: &[Option<Bootstrap>] = &[
    Some(Bootstrap::CircleFill),
    Some(Bootstrap::SquareFill),
    Some(Bootstrap::DiamondFill),
    Some(Bootstrap::XLg),
    Some(Bootstrap::DashLg),
    Some(Bootstrap::Dot),
];

// ---------------------------------------------------------------------------
// Assistance settings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct AssistanceSettings {
    pub(crate) auto_dim: bool,
    pub(crate) auto_fill_empty: bool,
    pub(crate) auto_cross_edges: bool,
    pub(crate) clue_sums_with_gaps: bool,
    pub(crate) crosshair_enabled: bool,
    pub(crate) crosshair_color: Color,
}

impl Default for AssistanceSettings {
    fn default() -> Self {
        Self {
            auto_dim: false,
            auto_fill_empty: false,
            auto_cross_edges: false,
            clue_sums_with_gaps: false,
            crosshair_enabled: false,
            crosshair_color: Color { r: 0.40, g: 0.72, b: 1.0, a: 0.18 },
        }
    }
}

// ---------------------------------------------------------------------------
// Settings persistence
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SavedCellVisual {
    r: f32,
    g: f32,
    b: f32,
    icon_idx: usize,
}

fn default_clue_bg_channel() -> f32 { 0.97 }
fn default_sum_bg_channel()  -> f32 { 0.82 }
fn default_xhair_r()         -> f32 { 0.40 }
fn default_xhair_g()         -> f32 { 0.72 }
fn default_xhair_b()         -> f32 { 1.0  }
fn default_xhair_a()         -> f32 { 0.18 }

#[derive(Serialize, Deserialize)]
struct SavedSettings {
    unknown: SavedCellVisual,
    filled:  SavedCellVisual,
    empty:   SavedCellVisual,
    #[serde(default = "default_clue_bg_channel")] clue_bg_r: f32,
    #[serde(default = "default_clue_bg_channel")] clue_bg_g: f32,
    #[serde(default = "default_clue_bg_channel")] clue_bg_b: f32,
    #[serde(default = "default_sum_bg_channel")]  sum_bg_r:  f32,
    #[serde(default = "default_sum_bg_channel")]  sum_bg_g:  f32,
    #[serde(default = "default_sum_bg_channel")]  sum_bg_b:  f32,
    #[serde(default)] auto_dim: bool,
    #[serde(default)] auto_fill_empty: bool,
    #[serde(default)] auto_cross_edges: bool,
    #[serde(default)] clue_sums_with_gaps: bool,
    #[serde(default)] crosshair_enabled: bool,
    #[serde(default = "default_xhair_r")] crosshair_r: f32,
    #[serde(default = "default_xhair_g")] crosshair_g: f32,
    #[serde(default = "default_xhair_b")] crosshair_b: f32,
    #[serde(default = "default_xhair_a")] crosshair_a: f32,
}

pub(crate) fn icon_to_idx(icon: Option<Bootstrap>) -> usize {
    let ch = icon_char(icon);
    ICON_OPTIONS.iter().position(|&opt| icon_char(opt) == ch).unwrap_or(0)
}

pub(crate) fn idx_to_icon(idx: usize) -> Option<Bootstrap> {
    ICON_OPTIONS.get(idx).copied().flatten()
}

fn visual_to_saved(vis: &CellVisual) -> SavedCellVisual {
    SavedCellVisual {
        r: vis.color.r,
        g: vis.color.g,
        b: vis.color.b,
        icon_idx: icon_to_idx(vis.icon),
    }
}

fn saved_to_visual(s: SavedCellVisual) -> CellVisual {
    CellVisual {
        color: Color { r: s.r, g: s.g, b: s.b, a: 1.0 },
        icon: idx_to_icon(s.icon_idx),
    }
}

fn config_path() -> Option<PathBuf> {
    ProjectDirs::from("", "nonogram", "nonogram-gui")
        .map(|pd| pd.config_dir().join("settings.json"))
}

pub(crate) fn load_settings() -> (CellSettings, AssistanceSettings) {
    let path = match config_path() {
        Some(p) => p,
        None    => return (CellSettings::default(), AssistanceSettings::default()),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(s)  => s,
        Err(_) => return (CellSettings::default(), AssistanceSettings::default()),
    };
    let saved: SavedSettings = match serde_json::from_str(&content) {
        Ok(s)  => s,
        Err(_) => return (CellSettings::default(), AssistanceSettings::default()),
    };
    let cell = CellSettings {
        unknown: saved_to_visual(saved.unknown),
        filled:  saved_to_visual(saved.filled),
        empty:   saved_to_visual(saved.empty),
        clue_bg: Color { r: saved.clue_bg_r, g: saved.clue_bg_g, b: saved.clue_bg_b, a: 1.0 },
        sum_bg:  Color { r: saved.sum_bg_r,  g: saved.sum_bg_g,  b: saved.sum_bg_b,  a: 1.0 },
    };
    let assist = AssistanceSettings {
        auto_dim:             saved.auto_dim,
        auto_fill_empty:      saved.auto_fill_empty,
        auto_cross_edges:     saved.auto_cross_edges,
        clue_sums_with_gaps:  saved.clue_sums_with_gaps,
        crosshair_enabled:    saved.crosshair_enabled,
        crosshair_color:      Color { r: saved.crosshair_r, g: saved.crosshair_g, b: saved.crosshair_b, a: saved.crosshair_a },
    };
    (cell, assist)
}

pub(crate) fn save_settings(settings: &CellSettings, assist: &AssistanceSettings) {
    let Some(path) = config_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let saved = SavedSettings {
        unknown:  visual_to_saved(&settings.unknown),
        filled:   visual_to_saved(&settings.filled),
        empty:    visual_to_saved(&settings.empty),
        clue_bg_r: settings.clue_bg.r,
        clue_bg_g: settings.clue_bg.g,
        clue_bg_b: settings.clue_bg.b,
        sum_bg_r:  settings.sum_bg.r,
        sum_bg_g:  settings.sum_bg.g,
        sum_bg_b:  settings.sum_bg.b,
        auto_dim:            assist.auto_dim,
        auto_fill_empty:     assist.auto_fill_empty,
        auto_cross_edges:    assist.auto_cross_edges,
        clue_sums_with_gaps: assist.clue_sums_with_gaps,
        crosshair_enabled:   assist.crosshair_enabled,
        crosshair_r:         assist.crosshair_color.r,
        crosshair_g:         assist.crosshair_color.g,
        crosshair_b:         assist.crosshair_color.b,
        crosshair_a:         assist.crosshair_color.a,
    };
    if let Ok(json) = serde_json::to_string_pretty(&saved) {
        let _ = std::fs::write(&path, json);
    }
}
