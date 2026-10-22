use uuid::Uuid;

use crate::models::Container;

#[derive(Debug, Clone)]
pub struct InsertionTarget {
  pub target_parent: Container,
  pub target_index: usize,
  pub prev_tiling_size: f32,
  pub prev_sibling_count: usize,
  /// Column that was immediately left of this window when it left a
  /// strip.
  pub left_column_id: Option<Uuid>,
  /// Column that was immediately right of this window when it left a
  /// strip.
  pub right_column_id: Option<Uuid>,
}
