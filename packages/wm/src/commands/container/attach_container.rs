use anyhow::bail;
use wm_common::WorkspaceLayout;

use super::resize_tiling_container;
use crate::{
  models::Container,
  traits::{CommonGetters, TilingSizeGetters},
};

/// Inserts a child container at the specified index.
///
/// The inserted child will be resized to fit the available space.
/// A column inserted into a scrolling workspace keeps its proportion.
pub fn attach_container(
  child: &Container,
  target_parent: &Container,
  target_index: Option<usize>,
) -> anyhow::Result<()> {
  if !child.is_detached() {
    bail!("Cannot attach an already attached container.");
  }

  let scrolling_workspace = target_parent
    .as_workspace()
    .filter(|workspace| workspace.layout() == WorkspaceLayout::Scrolling)
    .cloned();
  let keep_proportion = scrolling_workspace.is_some();
  let is_column = child.as_tiling_window().is_some();

  let attach = || -> anyhow::Result<()> {
    insert_child(child, target_parent, target_index);

    if keep_proportion {
      return Ok(());
    }

    resize_attached_tiling_child(child);
    Ok(())
  };

  if let Some(workspace) = scrolling_workspace.filter(|_| is_column) {
    crate::scrolling::preserve_view(&workspace, attach)
  } else {
    attach()
  }
}

fn insert_child(
  child: &Container,
  target_parent: &Container,
  target_index: Option<usize>,
) {
  if let Some(target_index) = target_index {
    // Ensure target index is within the bounds of the parent's children.
    let target_index = target_index.clamp(0, target_parent.child_count());

    // Insert the child at the specified index.
    target_parent
      .borrow_children_mut()
      .insert(target_index, child.clone());
  } else {
    target_parent.borrow_children_mut().push_back(child.clone());
  }

  target_parent
    .borrow_child_focus_order_mut()
    .push_back(child.id());

  *child.borrow_parent_mut() = Some(target_parent.clone());
}

fn resize_attached_tiling_child(child: &Container) {
  let Ok(child) = child.as_tiling_container() else {
    return;
  };

  let tiling_siblings = child.tiling_siblings().collect::<Vec<_>>();

  if tiling_siblings.is_empty() {
    child.set_tiling_size(1.0);
    return;
  }

  // Set initial tiling size to 0, and then size up the container
  // to the target size.
  #[allow(clippy::cast_precision_loss)]
  let target_size = 1.0 / (tiling_siblings.len() + 1) as f32;
  child.set_tiling_size(0.0);
  resize_tiling_container(&child, target_size);
}
