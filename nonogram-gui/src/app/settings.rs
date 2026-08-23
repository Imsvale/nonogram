use iced::{Color, keyboard::key::Named as NamedKey};
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
// Focus-mode key binding
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum FocusKey { F9, F10, F11, F12, Escape }

impl FocusKey {
    pub(crate) const ALL: &'static [Self] =
        &[Self::F9, Self::F10, Self::F11, Self::F12, Self::Escape];

    pub(crate) fn matches(&self, named: &NamedKey) -> bool {
        match self {
            Self::F9     => matches!(named, NamedKey::F9),
            Self::F10    => matches!(named, NamedKey::F10),
            Self::F11    => matches!(named, NamedKey::F11),
            Self::F12    => matches!(named, NamedKey::F12),
            Self::Escape => matches!(named, NamedKey::Escape),
        }
    }
}

impl std::fmt::Display for FocusKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::F9     => write!(f, "F9"),
            Self::F10    => write!(f, "F10"),
            Self::F11    => write!(f, "F11"),
            Self::F12    => write!(f, "F12"),
            Self::Escape => write!(f, "Esc"),
        }
    }
}

/// Secondary (character-key) fullscreen toggle. Esc is always hardcoded as
/// an exit-only key and is not listed here.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum SecondaryFocusKey { None, F, G, H, Z }

impl SecondaryFocusKey {
    pub(crate) const ALL: &'static [Self] =
        &[Self::None, Self::F, Self::G, Self::H, Self::Z];

    pub(crate) fn matches_str(&self, c: &str) -> bool {
        match self {
            Self::None => false,
            Self::F    => matches!(c, "f" | "F"),
            Self::G    => matches!(c, "g" | "G"),
            Self::H    => matches!(c, "h" | "H"),
            Self::Z    => matches!(c, "z" | "Z"),
        }
    }
}

impl std::fmt::Display for SecondaryFocusKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::F    => write!(f, "F"),
            Self::G    => write!(f, "G"),
            Self::H    => write!(f, "H"),
            Self::Z    => write!(f, "Z"),
        }
    }
}

// ---------------------------------------------------------------------------
// Assistance settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssistFlag {
    AutoDim,
    AutoFillEmpty,
    ClueSumsWithGaps,
    AutoCrossEdges,
    CrosshairEnabled,
    CrosshairSkipHover,
    AxisLock,
}

// ---------------------------------------------------------------------------
// Run length display settings
// ---------------------------------------------------------------------------

/// Which axis label (if any) occupies a 3×3 subcell within a grid cell.
/// Each subcell holds at most one of H, V, or Empty.
/// TODO: extend to a 5×5 cross widget covering the primary cell plus its 4
/// orthogonal and 4 end-of-arm neighbours (9 cells × 9 subcells = 81 targets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SubcellKind { #[default] Empty, H, V }

fn default_subcells() -> [[SubcellKind; 3]; 3] {
    let mut s = [[SubcellKind::Empty; 3]; 3];
    s[2][0] = SubcellKind::H;  // bottom-left
    s[0][2] = SubcellKind::V;  // top-right
    s
}

fn default_num_size() -> f32 { 9.0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RunLengthSettings {
    pub show_start:      bool,
    pub start_threshold: u32,
    pub show_end:        bool,
    pub end_threshold:   u32,
    pub show_hover:      bool,
    pub hover_threshold: u32,
    pub adj_label_enabled:        bool,
    pub adj_label_h_prefer_after: bool,
    pub adj_label_v_prefer_after: bool,
    pub four_dir_labels_enabled:  bool,
    // 3×3 subcell grid: each cell holds Empty, H, or V
    #[serde(default = "default_subcells")]
    pub subcells: [[SubcellKind; 3]; 3],
    // Label text size in pixels
    #[serde(default = "default_num_size")]
    pub num_size: f32,
    // Per-axis custom label colors (auto = white/dark from bg)
    pub label_h_custom:           bool,
    pub label_h_r:                f32,
    pub label_h_g:                f32,
    pub label_h_b:                f32,
    pub label_v_custom:           bool,
    pub label_v_r:                f32,
    pub label_v_g:                f32,
    pub label_v_b:                f32,
}

impl Default for RunLengthSettings {
    fn default() -> Self {
        Self {
            show_start:      false,
            start_threshold: 3,
            show_end:        false,
            end_threshold:   3,
            show_hover:      true,
            hover_threshold: 2,
            adj_label_enabled:        false,
            adj_label_h_prefer_after: true,
            adj_label_v_prefer_after: true,
            four_dir_labels_enabled:  false,
            subcells:         default_subcells(),
            num_size:         9.0,
            label_h_custom:           false,
            label_h_r:                0.40,
            label_h_g:                0.72,
            label_h_b:                1.0,
            label_v_custom:           false,
            label_v_r:                0.40,
            label_v_g:                0.72,
            label_v_b:                1.0,
        }
    }
}

impl RunLengthSettings {
    /// Returns all subcell positions that hold the given kind, in row-major order.
    pub(crate) fn subcells_of(&self, kind: SubcellKind) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for sr in 0..3 {
            for sc in 0..3 {
                if self.subcells[sr][sc] == kind { out.push((sr, sc)); }
            }
        }
        out
    }

    /// Returns the first subcell position holding the given kind, if any.
    pub(crate) fn first_subcell_of(&self, kind: SubcellKind) -> Option<(usize, usize)> {
        self.subcells_of(kind).into_iter().next()
    }

}

#[derive(Clone, Debug)]
pub(crate) struct AssistanceSettings {
    pub(crate) auto_dim: bool,
    pub(crate) auto_fill_empty: bool,
    pub(crate) auto_cross_edges: bool,
    pub(crate) clue_sums_with_gaps: bool,
    pub(crate) crosshair_enabled: bool,
    pub(crate) axis_lock: bool,
    pub(crate) crosshair_h_color: Color,
    pub(crate) crosshair_v_color: Color,
    pub(crate) crosshair_skip_hover: bool,
    pub(crate) focus_key: FocusKey,
    pub(crate) focus_key2: SecondaryFocusKey,
    pub(crate) run_length: RunLengthSettings,
}

impl Default for AssistanceSettings {
    fn default() -> Self {
        Self {
            auto_dim: false,
            auto_fill_empty: false,
            auto_cross_edges: false,
            clue_sums_with_gaps: false,
            crosshair_enabled: false,
            axis_lock: false,
            crosshair_h_color: Color { r: 0.40, g: 0.72, b: 1.0, a: 0.18 },
            crosshair_v_color: Color { r: 0.40, g: 0.72, b: 1.0, a: 0.18 },
            crosshair_skip_hover: false,
            focus_key: FocusKey::F11,
            focus_key2: SecondaryFocusKey::F,
            run_length: RunLengthSettings::default(),
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
fn default_focus_key()       -> FocusKey { FocusKey::F11 }
fn default_focus_key2()      -> SecondaryFocusKey { SecondaryFocusKey::F }
fn default_xhair_h_r()       -> f32 { 0.40 }
fn default_xhair_h_g()       -> f32 { 0.72 }
fn default_xhair_h_b()       -> f32 { 1.0  }
fn default_xhair_h_a()       -> f32 { 0.18 }
fn default_xhair_v_r()       -> f32 { 0.40 }
fn default_xhair_v_g()       -> f32 { 0.72 }
fn default_xhair_v_b()       -> f32 { 1.0  }
fn default_xhair_v_a()       -> f32 { 0.18 }

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
    #[serde(default)] axis_lock: bool,
    #[serde(default = "default_xhair_h_r")] crosshair_h_r: f32,
    #[serde(default = "default_xhair_h_g")] crosshair_h_g: f32,
    #[serde(default = "default_xhair_h_b")] crosshair_h_b: f32,
    #[serde(default = "default_xhair_h_a")] crosshair_h_a: f32,
    #[serde(default = "default_xhair_v_r")] crosshair_v_r: f32,
    #[serde(default = "default_xhair_v_g")] crosshair_v_g: f32,
    #[serde(default = "default_xhair_v_b")] crosshair_v_b: f32,
    #[serde(default = "default_xhair_v_a")] crosshair_v_a: f32,
    #[serde(default)] crosshair_skip_hover: bool,
    #[serde(default = "default_focus_key")]  focus_key:  FocusKey,
    #[serde(default = "default_focus_key2")] focus_key2: SecondaryFocusKey,
    #[serde(default)] run_length: RunLengthSettings,
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
        axis_lock:            saved.axis_lock,
        crosshair_h_color:    Color { r: saved.crosshair_h_r, g: saved.crosshair_h_g, b: saved.crosshair_h_b, a: saved.crosshair_h_a },
        crosshair_v_color:    Color { r: saved.crosshair_v_r, g: saved.crosshair_v_g, b: saved.crosshair_v_b, a: saved.crosshair_v_a },
        crosshair_skip_hover: saved.crosshair_skip_hover,
        focus_key:            saved.focus_key,
        focus_key2:           saved.focus_key2,
        run_length:           saved.run_length,
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
        axis_lock:           assist.axis_lock,
        crosshair_h_r:       assist.crosshair_h_color.r,
        crosshair_h_g:       assist.crosshair_h_color.g,
        crosshair_h_b:       assist.crosshair_h_color.b,
        crosshair_h_a:       assist.crosshair_h_color.a,
        crosshair_v_r:       assist.crosshair_v_color.r,
        crosshair_v_g:       assist.crosshair_v_color.g,
        crosshair_v_b:       assist.crosshair_v_color.b,
        crosshair_v_a:       assist.crosshair_v_color.a,
        crosshair_skip_hover: assist.crosshair_skip_hover,
        focus_key:           assist.focus_key,
        focus_key2:          assist.focus_key2,
        run_length:          assist.run_length.clone(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&saved) {
        let _ = std::fs::write(&path, json);
    }
}
