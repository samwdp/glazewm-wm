//! Geometry and view tracking for a workspace's scrolling layout.
//!
//! A scrolling layout places one tiling window per column on a horizontal
//! strip. Column widths are proportions of the workspace. The workspace
//! shows a view of that strip and remembers how far the view is scrolled.

use anyhow::Context;
use uuid::Uuid;
use wm_common::{WindowState, WorkspaceLayout};
use wm_platform::Rect;

use crate::{
  models::{
    Container, TilingContainer, TilingWindow, WindowContainer, Workspace,
  },
  traits::{
    CommonGetters, PositionGetters, TilingSizeGetters, WindowGetters,
    MIN_TILING_SIZE,
  },
};

/// Proportion of the workspace given to a newly opened column.
pub const DEFAULT_COLUMN_PROPORTION: f32 = 0.5;

/// A column's position along the strip, before scroll is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripColumn {
  pub id: Uuid,
  pub start: i32,
  pub width: i32,
}

/// Pixel width of a column.
///
/// `proportion` is a fraction of the workspace width. Inner gaps are
/// included so that columns of proportion `1/n` pack across the view
/// exactly, for any positive `n`.
#[must_use]
pub fn column_width_px(
  proportion: f32,
  view_width: i32,
  inner_gap: i32,
) -> i32 {
  let proportion = proportion.max(MIN_TILING_SIZE);
  #[allow(clippy::cast_precision_loss)]
  let width =
    proportion * view_width as f32 - (1.0 - proportion) * inner_gap as f32;

  #[allow(clippy::cast_possible_truncation)]
  let width_px = width.round().max(1.0) as i32;
  width_px
}

/// Proportion that produces `width_px` in `column_width_px`.
#[must_use]
pub fn proportion_for_width(
  width_px: i32,
  view_width: i32,
  inner_gap: i32,
) -> f32 {
  let denom = view_width + inner_gap;
  if denom <= 0 {
    return DEFAULT_COLUMN_PROPORTION;
  }

  #[allow(clippy::cast_precision_loss)]
  let proportion = (width_px + inner_gap) as f32 / denom as f32;
  proportion.max(MIN_TILING_SIZE)
}

/// Scales a pixel offset when the workspace width changes.
#[must_use]
pub fn scaled_offset(offset: i32, old_width: i32, new_width: i32) -> i32 {
  if old_width <= 0 {
    return offset;
  }

  let scaled =
    i64::from(offset) * i64::from(new_width) / i64::from(old_width);

  #[allow(clippy::cast_possible_truncation)]
  let scaled_px = scaled as i32;
  scaled_px
}

/// Scroll offset after a strip change that may sit entirely left of the
/// view.
///
/// Columns that intersect the view keep their on-screen position when they
/// all shift by the same amount. A change that removes or splits those
/// columns leaves the offset alone.
#[must_use]
pub fn held_offset(
  offset: i32,
  view_width: i32,
  before: &[StripColumn],
  after: &[StripColumn],
) -> i32 {
  let anchors = anchor_columns(before, offset, view_width);
  if anchors.is_empty() {
    return offset;
  }

  let mut deltas = Vec::with_capacity(anchors.len());
  for (id, old_start) in anchors {
    let Some(column) = after.iter().find(|column| column.id == id) else {
      return offset;
    };
    deltas.push(column.start - old_start);
  }

  let delta = deltas[0];
  if deltas.iter().all(|candidate| *candidate == delta) {
    offset + delta
  } else {
    offset
  }
}

/// Minimum scroll that keeps the focused column fully in the view.
///
/// A column wider than the view is left-aligned. When the strip is
/// narrower than the view, the offset is zero. With no focused column,
/// the offset stays in range so the strip is not scrolled into empty
/// space.
#[must_use]
pub fn clamped_offset(
  offset: i32,
  view_width: i32,
  columns: &[StripColumn],
  focused: Option<Uuid>,
) -> i32 {
  let strip_end = columns
    .last()
    .map_or(0, |column| column.start + column.width);

  if strip_end <= view_width {
    return 0;
  }

  // Keep the focused column fully in view, then stay inside the strip.
  // A short last column must not be left-aligned: that hides its neighbor
  // and leaves empty space on the right.
  let max_offset = (strip_end - view_width).max(0);
  let offset = if let Some(focused) = focused {
    columns
      .iter()
      .find(|column| column.id == focused)
      .map_or(offset, |column| clamp_to_column(offset, view_width, column))
  } else {
    offset
  };

  offset.clamp(0, max_offset)
}

/// Intersection of two rects, when both axes overlap.
#[must_use]
pub fn intersection(a: &Rect, b: &Rect) -> Option<Rect> {
  let left = a.left.max(b.left);
  let top = a.top.max(b.top);
  let right = a.right.min(b.right);
  let bottom = a.bottom.min(b.bottom);

  if right > left && bottom > top {
    Some(Rect::from_ltrb(left, top, right, bottom))
  } else {
    None
  }
}

/// Tiling windows that form the strip, left to right.
pub fn columns(workspace: &Workspace) -> Vec<TilingWindow> {
  workspace
    .tiling_children()
    .filter_map(|container| match container {
      TilingContainer::TilingWindow(window) => Some(window),
      TilingContainer::Split(_) => None,
    })
    .collect()
}

/// Strip positions of the workspace's columns.
pub fn column_spans(
  workspace: &Workspace,
) -> anyhow::Result<Vec<StripColumn>> {
  let view_width = workspace.to_rect()?.width();
  let gap = inner_gap_px(workspace)?;
  let columns = columns(workspace);
  let mut start = 0;
  let mut spans = Vec::with_capacity(columns.len());

  for (index, column) in columns.iter().enumerate() {
    let width = column_width_px(column.tiling_size(), view_width, gap);
    spans.push(StripColumn {
      id: column.id(),
      start,
      width,
    });
    start += width;
    if index + 1 != columns.len() {
      start += gap;
    }
  }

  Ok(spans)
}

/// Logical column rect in screen coordinates, including the off-screen
/// part.
pub fn column_screen_rect(window: &TilingWindow) -> anyhow::Result<Rect> {
  let workspace = window.workspace().context("No workspace.")?;
  let view = workspace.to_rect()?;
  let spans = column_spans(&workspace)?;
  let column = spans
    .iter()
    .find(|column| column.id == window.id())
    .context("Window is not a column.")?;

  Ok(Rect::from_xy(
    view.x() + column.start - workspace.scroll_offset(),
    view.y(),
    column.width,
    view.height(),
  ))
}

/// Whether a tiling window on a scrolling workspace intersects the view.
///
/// Other windows are always treated as in view.
pub fn column_in_view(window: &WindowContainer) -> anyhow::Result<bool> {
  if window.state() != WindowState::Tiling {
    return Ok(true);
  }

  let Some(workspace) = window.workspace() else {
    return Ok(true);
  };

  if workspace.layout() != WorkspaceLayout::Scrolling {
    return Ok(true);
  }

  let logical = window.to_rect()?;
  let view = workspace.to_rect()?;
  Ok(intersection(&logical, &view).is_some())
}

/// Keeps on-screen columns still across a strip mutation, then runs
/// `mutate`.
pub fn preserve_view(
  workspace: &Workspace,
  mutate: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
  if workspace.layout() != WorkspaceLayout::Scrolling {
    return mutate();
  }

  let Ok(view) = workspace.to_rect() else {
    return mutate();
  };

  let before = column_spans(workspace).unwrap_or_default();
  let offset = workspace.scroll_offset();
  mutate()?;
  let after = column_spans(workspace).unwrap_or_default();
  workspace.set_scroll_offset(held_offset(
    offset,
    view.width(),
    &before,
    &after,
  ));
  Ok(())
}

/// Scales the offset when the workspace width changes, then clamps it.
pub fn reconcile(
  workspace: &Workspace,
  focused_column: Option<Uuid>,
) -> anyhow::Result<()> {
  if workspace.layout() != WorkspaceLayout::Scrolling {
    return Ok(());
  }

  let view_width = workspace.to_rect()?.width().max(0);
  let basis = workspace.scroll_basis_width();
  if basis > 0 && view_width > 0 && basis != view_width {
    workspace.set_scroll_offset(scaled_offset(
      workspace.scroll_offset(),
      basis,
      view_width,
    ));
  }

  if view_width > 0 {
    workspace.set_scroll_basis_width(view_width);
  }

  let columns = column_spans(workspace)?;
  workspace.set_scroll_offset(clamped_offset(
    workspace.scroll_offset(),
    view_width,
    &columns,
    focused_column,
  ));
  Ok(())
}

/// Child index that inserts `moving_id` at `rank` among the other columns.
///
/// `rank` `0` is before the first other column. The index is adjusted for
/// `VecDequeExt::shift_to_index` when `moving_id` is already a child.
pub fn child_index_for_rank(
  workspace: &Workspace,
  rank: usize,
  moving_id: Option<Uuid>,
) -> usize {
  let children = workspace.children();
  let others = children
    .iter()
    .enumerate()
    .filter(|(_, child)| {
      child.as_tiling_window().is_some() && Some(child.id()) != moving_id
    })
    .map(|(index, _)| index)
    .collect::<Vec<_>>();

  let raw = if rank >= others.len() {
    others.last().map_or(0, |index| index + 1)
  } else {
    others[rank]
  };

  let Some(moving_id) = moving_id else {
    return raw;
  };

  match children.iter().position(|child| child.id() == moving_id) {
    Some(self_index) if self_index < raw => raw - 1,
    _ => raw,
  }
}

/// Rank among columns where a cursor at `cursor_x` is nearest a boundary.
pub fn cursor_rank(
  workspace: &Workspace,
  cursor_x: i32,
) -> anyhow::Result<usize> {
  let view = workspace.to_rect()?;
  let offset = workspace.scroll_offset();
  let columns = column_spans(workspace)?;

  if columns.is_empty() {
    return Ok(0);
  }

  let mut best_rank = 0;
  let mut best_distance = i32::MAX;

  for (rank, column) in columns.iter().enumerate() {
    let boundary = view.x() + column.start - offset;
    let distance = (cursor_x - boundary).abs();
    if distance < best_distance {
      best_distance = distance;
      best_rank = rank;
    }
  }

  let last = columns.last().context("No column.")?;
  let end = view.x() + last.start + last.width - offset;
  if (cursor_x - end).abs() < best_distance {
    best_rank = columns.len();
  }

  Ok(best_rank)
}

/// Index passed to `shift_to_index` that lands immediately before
/// `column`.
#[must_use]
pub fn shift_target_before(
  self_index: usize,
  column_index: usize,
) -> usize {
  if self_index < column_index {
    column_index - 1
  } else {
    column_index
  }
}

/// Index passed to `shift_to_index` that lands immediately after `column`.
#[must_use]
pub fn shift_target_after(column_index: usize) -> usize {
  column_index + 1
}

fn anchor_columns(
  columns: &[StripColumn],
  offset: i32,
  view_width: i32,
) -> Vec<(Uuid, i32)> {
  let visible = columns
    .iter()
    .filter(|column| {
      let end = column.start + column.width;
      end > offset && column.start < offset + view_width
    })
    .map(|column| (column.id, column.start))
    .collect::<Vec<_>>();

  if !visible.is_empty() {
    return visible;
  }

  if let Some(next) = columns.iter().find(|column| column.start >= offset)
  {
    return vec![(next.id, next.start)];
  }

  columns
    .last()
    .map(|column| vec![(column.id, column.start)])
    .unwrap_or_default()
}

fn clamp_to_column(
  offset: i32,
  view_width: i32,
  column: &StripColumn,
) -> i32 {
  if column.width >= view_width {
    return column.start;
  }

  if column.start < offset {
    column.start
  } else if column.start + column.width > offset + view_width {
    column.start + column.width - view_width
  } else {
    offset
  }
}

fn inner_gap_px(workspace: &Workspace) -> anyhow::Result<i32> {
  let Some(column) = columns(workspace).into_iter().next() else {
    return Ok(0);
  };

  let (horizontal_gap, _) = column.inner_gaps()?;
  Ok(horizontal_gap)
}

/// Parent workspace when it uses the scrolling layout.
pub fn scrolling_parent(container: &Container) -> Option<Workspace> {
  container
    .parent()
    .and_then(|parent| parent.as_workspace().cloned())
    .filter(|workspace| workspace.layout() == WorkspaceLayout::Scrolling)
}

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::{
    clamped_offset, column_width_px, held_offset, proportion_for_width,
    scaled_offset, StripColumn,
  };

  fn column(start: i32, width: i32) -> StripColumn {
    StripColumn {
      id: Uuid::new_v4(),
      start,
      width,
    }
  }

  #[test]
  fn half_columns_fit_the_view() {
    let width = column_width_px(0.5, 1000, 10);
    assert_eq!(width + 10 + width, 1000);
    assert!((proportion_for_width(width, 1000, 10) - 0.5).abs() < 0.0001);
  }

  #[test]
  fn quarter_columns_fit_the_view() {
    let width = column_width_px(0.25, 1000, 16);
    assert_eq!(4 * width + 3 * 16, 1000);
  }

  #[test]
  fn clamp_scrolls_the_minimum_to_reveal_focus() {
    let columns =
      vec![column(0, 500), column(510, 500), column(1020, 500)];
    let focused = columns[2].id;
    assert_eq!(clamped_offset(0, 1000, &columns, Some(focused)), 520);
    assert_eq!(clamped_offset(520, 1000, &columns, Some(focused)), 520);
  }

  #[test]
  fn clamp_left_aligns_a_column_wider_than_the_view() {
    let wide = column(0, 1400);
    assert_eq!(clamped_offset(40, 1000, &[wide], Some(wide.id)), 0);
  }

  #[test]
  fn clamp_packs_a_short_strip_to_the_left() {
    let only = column(0, 495);
    assert_eq!(clamped_offset(80, 1000, &[only], Some(only.id)), 0);
  }

  #[test]
  fn hold_keeps_the_picture_when_a_left_column_is_removed() {
    let visible = column(1000, 500);
    let before = vec![column(0, 500), column(510, 480), visible];
    let after = vec![
      StripColumn {
        id: before[1].id,
        start: 0,
        width: 480,
      },
      StripColumn {
        id: visible.id,
        start: 490,
        width: 500,
      },
    ];

    // View starts at 1000, showing `visible`. Removal shifts it to 490.
    assert_eq!(held_offset(1000, 1000, &before, &after), 490);
  }

  #[test]
  fn hold_ignores_a_change_inside_the_view() {
    let left = column(0, 500);
    let right = column(510, 500);
    let before = vec![left, right];
    let after = vec![StripColumn {
      id: right.id,
      start: 0,
      width: 500,
    }];

    assert_eq!(held_offset(0, 1000, &before, &after), 0);
  }

  #[test]
  fn offset_scales_with_workspace_width() {
    assert_eq!(scaled_offset(200, 1000, 1500), 300);
  }

  #[test]
  fn overscroll_keeps_the_neighbor_beside_the_last_column() {
    let columns =
      vec![column(0, 495), column(505, 495), column(1010, 495)];
    let offset = clamped_offset(5000, 1000, &columns, Some(columns[2].id));

    assert_eq!(offset, 505);
    assert!(columns[1].start + columns[1].width > offset);
  }

  #[test]
  fn negative_offset_is_pulled_back_to_the_strip() {
    let columns = vec![column(0, 495), column(505, 495)];
    assert_eq!(
      clamped_offset(-200, 1000, &columns, Some(columns[0].id)),
      0
    );
  }
}
