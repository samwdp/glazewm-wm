use wm_common::{WmEvent, WorkspaceLayout};

use crate::{
  commands::container::move_container_within_tree,
  models::{TilingWindow, Workspace},
  scrolling::{self, DEFAULT_COLUMN_PROPORTION},
  traits::{
    CommonGetters, PositionGetters, TilingDirectionGetters,
    TilingSizeGetters,
  },
  wm_state::WmState,
};

/// Switches a workspace between split layout and scrolling layout.
pub fn toggle_workspace_layout(
  workspace: &Workspace,
  state: &mut WmState,
) -> anyhow::Result<()> {
  match workspace.layout() {
    WorkspaceLayout::Split => enter_scrolling(workspace, state)?,
    WorkspaceLayout::Scrolling => enter_split(workspace),
  }

  scrolling::reconcile(workspace, focused_column(workspace, state))?;

  state
    .pending_sync
    .queue_container_to_redraw(workspace.clone())
    .queue_workspace_to_reorder(workspace.clone());

  state.emit_event(WmEvent::WorkspaceUpdated {
    updated_workspace: workspace.to_dto()?,
  });

  Ok(())
}

/// Applies the layout from config when it differs from the live workspace.
pub fn apply_workspace_layout(
  workspace: &Workspace,
  layout: WorkspaceLayout,
  state: &mut WmState,
) -> anyhow::Result<()> {
  if workspace.layout() == layout {
    return Ok(());
  }

  toggle_workspace_layout(workspace, state)
}

fn enter_scrolling(
  workspace: &Workspace,
  state: &mut WmState,
) -> anyhow::Result<()> {
  let mut windows = workspace
    .descendants()
    .filter_map(|container| container.as_tiling_window().cloned())
    .filter_map(|window| {
      let rect = window.to_rect().ok()?;
      Some((window, rect.x(), rect.y()))
    })
    .collect::<Vec<_>>();

  windows.sort_by_key(|(_, x, y)| (*x, *y));

  workspace.set_layout(WorkspaceLayout::Scrolling);
  workspace.set_scroll_offset(0);
  workspace.set_scroll_basis_width(0);

  for (window, _, _) in windows {
    window.set_tiling_size(DEFAULT_COLUMN_PROPORTION);
    move_container_within_tree(
      &window.into(),
      &workspace.clone().into(),
      workspace.child_count(),
      state,
    )?;
  }

  workspace.set_scroll_offset(0);
  Ok(())
}

fn enter_split(workspace: &Workspace) {
  let columns = scrolling::columns(workspace);
  let proportions = columns
    .iter()
    .map(TilingWindow::tiling_size)
    .collect::<Vec<_>>();
  let sum = proportions.iter().sum::<f32>();
  let sum = if sum <= f32::EPSILON { 1.0 } else { sum };

  workspace.set_layout(WorkspaceLayout::Split);
  workspace.set_tiling_direction(wm_common::TilingDirection::Horizontal);
  workspace.set_scroll_offset(0);

  for (window, proportion) in columns.iter().zip(proportions) {
    window.set_tiling_size(proportion / sum);
  }
}

fn focused_column(
  workspace: &Workspace,
  state: &WmState,
) -> Option<uuid::Uuid> {
  state.focused_container().and_then(|container| {
    let window = container.as_tiling_window()?;
    window
      .workspace()
      .is_some_and(|candidate| candidate.id() == workspace.id())
      .then_some(window.id())
  })
}
