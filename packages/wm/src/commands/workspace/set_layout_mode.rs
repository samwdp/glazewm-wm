use wm_common::{LayoutMode, TilingDirection, WmEvent};
use wm_scrolling::{
  tiling_sizes_for_scrolling, tiling_sizes_for_tiling, toggle_layout_mode,
};

use crate::{
  models::Workspace,
  traits::{
    CommonGetters, PositionGetters, TilingDirectionGetters,
    TilingSizeGetters,
  },
  wm_state::WmState,
};

/// Switches a workspace to the given layout mode.
///
/// When switching to scrolling:
/// - All tiling children are set to the default scrolling size (50 %).
/// - The scroll offset is reset to 0.
///
/// When switching to tiling:
/// - All tiling children are given equal shares of the available width.
/// - The scroll offset is reset to 0.
pub fn set_layout_mode(
  workspace: Workspace,
  mode: LayoutMode,
  state: &mut WmState,
) -> anyhow::Result<()> {
  if workspace.layout_mode() == mode {
    return Ok(());
  }

  workspace.set_layout_mode(mode.clone());
  workspace.set_scroll_offset(0);

  // Ensure scrolling workspaces always tile horizontally.
  if mode == LayoutMode::Scrolling {
    workspace.set_tiling_direction(TilingDirection::Horizontal);
  }

  let tiling_children =
    workspace.tiling_children().collect::<Vec<_>>();
  let count = tiling_children.len();

  let new_sizes = match mode {
    LayoutMode::Scrolling => tiling_sizes_for_scrolling(count),
    LayoutMode::Tiling => tiling_sizes_for_tiling(count),
  };

  for (child, size) in tiling_children.iter().zip(new_sizes.iter()) {
    child.set_tiling_size(*size);
  }

  state.emit_event(WmEvent::WorkspaceLayoutChanged {
    changed_workspace: workspace.to_dto()?,
  });

  state
    .pending_sync
    .queue_containers_to_redraw(workspace.tiling_children());

  Ok(())
}

/// Toggles the layout mode of a workspace between tiling and scrolling.
pub fn toggle_workspace_layout_mode(
  workspace: Workspace,
  state: &mut WmState,
) -> anyhow::Result<()> {
  let new_mode = toggle_layout_mode(workspace.layout_mode());
  set_layout_mode(workspace, new_mode, state)
}

/// Updates the horizontal scroll offset of a scrolling workspace so that
/// the given tiling window is fully visible in the viewport.
///
/// Calls the pure [`wm_scrolling::compute_scroll_offset`] function to
/// calculate the new offset, then writes it back to the workspace.
pub fn update_scroll_offset(
  workspace: &Workspace,
  focused_window_index: usize,
) -> anyhow::Result<()> {
  if workspace.layout_mode() != LayoutMode::Scrolling {
    return Ok(());
  }

  let workspace_rect = workspace.to_rect()?;
  let viewport_width = workspace_rect.width();

  // Gather the tiling sizes of all direct tiling children.
  let tiling_children: Vec<_> =
    workspace.tiling_children().collect();

  // Compute gaps (horizontal inner gap in scrolling mode).
  let inner_gap_px = if let Some(first) = tiling_children.first() {
    let (h_gap, _) = first.inner_gaps()?;
    h_gap
  } else {
    return Ok(());
  };

  // Get the focused window entry; do nothing if index is out of range.
  let Some(focused_window) = tiling_children.get(focused_window_index)
  else {
    return Ok(());
  };

  let prev_widths: Vec<i32> = tiling_children
    .iter()
    .take(focused_window_index)
    .map(|c| {
      wm_scrolling::window_width(c.tiling_size(), viewport_width)
    })
    .collect();

  let virtual_x =
    wm_scrolling::window_virtual_offset(&prev_widths, inner_gap_px);
  let win_width = wm_scrolling::window_width(
    focused_window.tiling_size(),
    viewport_width,
  );

  let new_offset = wm_scrolling::compute_scroll_offset(
    workspace.scroll_offset(),
    viewport_width,
    virtual_x,
    win_width,
  );

  workspace.set_scroll_offset(new_offset);

  Ok(())
}
