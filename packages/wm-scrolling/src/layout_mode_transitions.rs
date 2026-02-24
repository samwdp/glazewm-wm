use wm_common::LayoutMode;

use crate::DEFAULT_TILING_SIZE;

/// Determines the effective layout mode for a workspace, combining the
/// workspace-level override with the global default.
///
/// # Example
/// ```
/// # use wm_common::LayoutMode;
/// # use wm_scrolling::effective_layout_mode;
/// // Workspace override takes precedence.
/// let mode = effective_layout_mode(
///     Some(LayoutMode::Scrolling),
///     LayoutMode::Tiling,
/// );
/// assert_eq!(mode, LayoutMode::Scrolling);
///
/// // Fall back to global default when no workspace override.
/// let mode = effective_layout_mode(None, LayoutMode::Scrolling);
/// assert_eq!(mode, LayoutMode::Scrolling);
/// ```
#[must_use]
pub fn effective_layout_mode(
  workspace_layout: Option<LayoutMode>,
  default_layout: LayoutMode,
) -> LayoutMode {
  workspace_layout.unwrap_or(default_layout)
}

/// Toggles between tiling and scrolling layout modes.
///
/// # Example
/// ```
/// # use wm_common::LayoutMode;
/// # use wm_scrolling::toggle_layout_mode;
/// assert_eq!(toggle_layout_mode(LayoutMode::Tiling), LayoutMode::Scrolling);
/// assert_eq!(toggle_layout_mode(LayoutMode::Scrolling), LayoutMode::Tiling);
/// ```
#[must_use]
pub fn toggle_layout_mode(current: LayoutMode) -> LayoutMode {
  match current {
    LayoutMode::Tiling => LayoutMode::Scrolling,
    LayoutMode::Scrolling => LayoutMode::Tiling,
  }
}

/// Computes the new tiling sizes to assign to each window when switching
/// to scrolling mode.
///
/// In scrolling mode every window gets [`DEFAULT_TILING_SIZE`] regardless
/// of its previous tiling size.
///
/// # Example
/// ```
/// # use wm_scrolling::{tiling_sizes_for_scrolling, DEFAULT_TILING_SIZE};
/// let sizes = tiling_sizes_for_scrolling(3);
/// assert_eq!(sizes, vec![DEFAULT_TILING_SIZE; 3]);
/// ```
#[must_use]
pub fn tiling_sizes_for_scrolling(window_count: usize) -> Vec<f32> {
  vec![DEFAULT_TILING_SIZE; window_count]
}

/// Computes the new tiling sizes to assign to each window when switching
/// to tiling mode.
///
/// In tiling mode every window gets an equal share of the available
/// space (1.0 / window_count), or 1.0 if there is only one window.
///
/// # Example
/// ```
/// # use wm_scrolling::tiling_sizes_for_tiling;
/// let sizes = tiling_sizes_for_tiling(4);
/// let expected = vec![0.25; 4];
/// for (a, b) in sizes.iter().zip(expected.iter()) {
///     assert!((a - b).abs() < f32::EPSILON);
/// }
/// ```
#[must_use]
pub fn tiling_sizes_for_tiling(window_count: usize) -> Vec<f32> {
  if window_count == 0 {
    return Vec::new();
  }

  let share = 1.0 / window_count as f32;
  vec![share; window_count]
}

#[cfg(test)]
mod tests {
  use wm_common::LayoutMode;

  use super::*;

  #[test]
  fn test_effective_layout_mode_override() {
    assert_eq!(
      effective_layout_mode(
        Some(LayoutMode::Scrolling),
        LayoutMode::Tiling
      ),
      LayoutMode::Scrolling
    );
  }

  #[test]
  fn test_effective_layout_mode_fallback() {
    assert_eq!(
      effective_layout_mode(None, LayoutMode::Scrolling),
      LayoutMode::Scrolling
    );
    assert_eq!(
      effective_layout_mode(None, LayoutMode::Tiling),
      LayoutMode::Tiling
    );
  }

  #[test]
  fn test_toggle_layout_mode() {
    assert_eq!(
      toggle_layout_mode(LayoutMode::Tiling),
      LayoutMode::Scrolling
    );
    assert_eq!(
      toggle_layout_mode(LayoutMode::Scrolling),
      LayoutMode::Tiling
    );
  }

  #[test]
  fn test_tiling_sizes_for_scrolling() {
    assert_eq!(tiling_sizes_for_scrolling(3), vec![0.5, 0.5, 0.5]);
    assert_eq!(tiling_sizes_for_scrolling(0), Vec::<f32>::new());
  }

  #[test]
  fn test_tiling_sizes_for_tiling() {
    let sizes = tiling_sizes_for_tiling(4);
    assert_eq!(sizes.len(), 4);
    for s in &sizes {
      assert!((s - 0.25).abs() < f32::EPSILON);
    }
    assert_eq!(tiling_sizes_for_tiling(0), Vec::<f32>::new());
  }
}
