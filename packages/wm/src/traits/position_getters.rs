use ambassador::delegatable_trait;
use wm_common::Rect;

#[delegatable_trait]
pub trait PositionGetters {
  fn to_rect(&self) -> anyhow::Result<Rect>;
}

/// Implements the `PositionGetters` trait for tiling containers that can
/// be resized. This is used by `SplitContainer` and `TilingWindow`.
///
/// Expects that the struct has a wrapping `RefCell` containing a struct
/// with an `id` and a `parent` field.
#[macro_export]
macro_rules! impl_position_getters_as_resizable {
  ($struct_name:ident) => {
    impl PositionGetters for $struct_name {
      fn to_rect(&self) -> anyhow::Result<Rect> {
        let parent = self
          .parent()
          .and_then(|parent| parent.as_direction_container().ok())
          .context("Parent does not have a tiling direction.")?;

        let parent_rect = parent.to_rect()?;

        let (horizontal_gap, vertical_gap) = self.inner_gaps()?;
        let inner_gap = match parent.tiling_direction() {
          TilingDirection::Vertical => vertical_gap,
          TilingDirection::Horizontal => horizontal_gap,
        };

        // Detect whether the parent is a scrolling workspace.
        let scroll_offset = parent
          .as_workspace()
          .filter(|ws| {
            ws.layout_mode() == wm_common::LayoutMode::Scrolling
          })
          .map_or(0, |ws| ws.scroll_offset());

        let is_scrolling = scroll_offset != 0
          || parent.as_workspace().is_some_and(|ws| {
            ws.layout_mode() == wm_common::LayoutMode::Scrolling
          });

        #[allow(
          clippy::cast_precision_loss,
          clippy::cast_possible_truncation,
          clippy::cast_possible_wrap
        )]
        let (width, height) = match parent.tiling_direction() {
          TilingDirection::Vertical => {
            let available_height = parent_rect.height()
              - inner_gap * self.tiling_siblings().count() as i32;

            let height =
              (self.tiling_size() * available_height as f32) as i32;

            (parent_rect.width(), height)
          }
          TilingDirection::Horizontal => {
            if is_scrolling {
              // In scrolling mode each window is an independent fraction
              // of the viewport width; siblings do not shrink each other.
              let width = wm_scrolling::window_width(
                self.tiling_size(),
                parent_rect.width(),
              );
              (width, parent_rect.height())
            } else {
              let available_width = parent_rect.width()
                - inner_gap * self.tiling_siblings().count() as i32;

              let width =
                (available_width as f32 * self.tiling_size()).round()
                  as i32;

              (width, parent_rect.height())
            }
          }
        };

        let (x, y) = {
          let mut prev_siblings = self
            .prev_siblings()
            .filter_map(|sibling| sibling.as_tiling_container().ok());

          match prev_siblings.next() {
            None => {
              // First child: apply scroll offset if in scrolling mode.
              let base_x = parent_rect.x() - scroll_offset;
              (base_x, parent_rect.y())
            }
            Some(sibling) => {
              let sibling_rect = sibling.to_rect()?;

              match parent.tiling_direction() {
                TilingDirection::Vertical => (
                  parent_rect.x(),
                  sibling_rect.y() + sibling_rect.height() + inner_gap,
                ),
                TilingDirection::Horizontal => (
                  sibling_rect.x() + sibling_rect.width() + inner_gap,
                  parent_rect.y(),
                ),
              }
            }
          }
        };

        Ok(Rect::from_xy(x, y, width, height))
      }
    }
  };
}
