/// Default tiling size (as a fraction of the viewport width) assigned to
/// each new window created in a scrolling workspace.
pub const DEFAULT_TILING_SIZE: f32 = 0.5;

/// Computes the pixel width of a window in scrolling mode.
///
/// Unlike tiling mode, the window width is a direct fraction of the
/// viewport width and is not scaled down by the number of siblings.
///
/// # Example
/// ```
/// # use wm_scrolling::window_width;
/// let width = window_width(0.5, 1920);
/// assert_eq!(width, 960);
/// ```
#[must_use]
pub fn window_width(tiling_size: f32, viewport_width: i32) -> i32 {
  (tiling_size * viewport_width as f32).round() as i32
}

/// Computes the virtual canvas offset (distance in pixels from the left
/// edge of the virtual canvas) for a window given its index among direct
/// siblings and their widths.
///
/// # Parameters
/// - `prev_widths_px`: The pixel width of each preceding sibling, in
///   order from first to last.
/// - `inner_gap_px`: The gap in pixels between adjacent windows.
///
/// # Example
/// ```
/// # use wm_scrolling::window_virtual_offset;
/// // First window: no previous siblings.
/// assert_eq!(window_virtual_offset(&[], 20), 0);
///
/// // Second window with a 960px first sibling and 20px gap.
/// assert_eq!(window_virtual_offset(&[960], 20), 980);
/// ```
#[must_use]
pub fn window_virtual_offset(
  prev_widths_px: &[i32],
  inner_gap_px: i32,
) -> i32 {
  prev_widths_px
    .iter()
    .fold(0, |acc, &w| acc + w + inner_gap_px)
}

/// Computes the on-screen x-coordinate for a scrolling window.
///
/// Applies the workspace scroll offset to the window's virtual canvas
/// position so it renders at the correct pixel position on screen.
///
/// # Parameters
/// - `workspace_x`: Left edge of the workspace's visible area.
/// - `virtual_offset`: Offset from the left of the virtual canvas
///   (see [`window_virtual_offset`]).
/// - `scroll_offset`: Current horizontal scroll offset of the viewport.
///
/// # Example
/// ```
/// # use wm_scrolling::on_screen_x;
/// // No scrolling: first window at workspace left edge.
/// assert_eq!(on_screen_x(0, 0, 0), 0);
///
/// // Viewport scrolled 960px to the right: second window (at virtual
/// // offset 980) snaps to the left edge of the viewport.
/// assert_eq!(on_screen_x(0, 980, 980), 0);
/// ```
#[must_use]
pub fn on_screen_x(
  workspace_x: i32,
  virtual_offset: i32,
  scroll_offset: i32,
) -> i32 {
  workspace_x + virtual_offset - scroll_offset
}

/// Computes the new scroll offset needed to keep a given window fully
/// visible inside the viewport.
///
/// The viewport is assumed to extend from `current_scroll_offset` to
/// `current_scroll_offset + viewport_width`. The window occupies
/// `[virtual_x, virtual_x + window_width]` on the virtual canvas.
///
/// Returns the updated scroll offset (always ≥ 0).
///
/// # Example
/// ```
/// # use wm_scrolling::compute_scroll_offset;
/// // Window already fully visible: no change.
/// assert_eq!(compute_scroll_offset(0, 1920, 100, 960), 0);
///
/// // Window starts off the right edge: scroll right.
/// assert_eq!(compute_scroll_offset(0, 1920, 1000, 960), 40);
///
/// // Window starts off the left edge: scroll left.
/// assert_eq!(compute_scroll_offset(500, 1920, 200, 960), 200);
/// ```
#[must_use]
pub fn compute_scroll_offset(
  current_scroll_offset: i32,
  viewport_width: i32,
  window_virtual_x: i32,
  window_width: i32,
) -> i32 {
  let new_offset =
    if window_virtual_x < current_scroll_offset {
      // Window is off the left edge of the viewport.
      window_virtual_x
    } else if window_virtual_x + window_width
      > current_scroll_offset + viewport_width
    {
      // Window is off the right edge of the viewport.
      window_virtual_x + window_width - viewport_width
    } else {
      current_scroll_offset
    };

  new_offset.max(0)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_window_width() {
    assert_eq!(window_width(0.5, 1920), 960);
    assert_eq!(window_width(1.0, 1920), 1920);
    assert_eq!(window_width(0.5, 1921), 961);
  }

  #[test]
  fn test_window_virtual_offset() {
    assert_eq!(window_virtual_offset(&[], 20), 0);
    assert_eq!(window_virtual_offset(&[960], 20), 980);
    assert_eq!(window_virtual_offset(&[960, 960], 20), 1960);
  }

  #[test]
  fn test_on_screen_x() {
    assert_eq!(on_screen_x(0, 0, 0), 0);
    assert_eq!(on_screen_x(0, 980, 980), 0);
    assert_eq!(on_screen_x(100, 0, 0), 100);
    assert_eq!(on_screen_x(100, 960, 480), 580);
  }

  #[test]
  fn test_compute_scroll_offset_no_change() {
    // Window is fully visible.
    assert_eq!(compute_scroll_offset(0, 1920, 100, 960), 0);
  }

  #[test]
  fn test_compute_scroll_offset_scroll_right() {
    // Second window (virtual x=980, width=960) is 40px off the right edge.
    assert_eq!(compute_scroll_offset(0, 1920, 980, 960), 20);
  }

  #[test]
  fn test_compute_scroll_offset_scroll_left() {
    // After scrolling right, focusing the first window scrolls back.
    assert_eq!(compute_scroll_offset(500, 1920, 200, 960), 200);
  }

  #[test]
  fn test_compute_scroll_offset_clamp_to_zero() {
    // Never returns a negative offset.
    assert_eq!(compute_scroll_offset(100, 1920, 0, 960), 0);
  }
}
