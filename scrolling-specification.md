# Scrolling Layout Feature Specification

## Overview

This document specifies the design and implementation steps for adding a **scrolling layout mode** to GlazeWM, inspired by [niri](https://github.com/niri-wm/niri). In scrolling mode, a workspace behaves as an infinite horizontal canvas: windows are arranged side-by-side and the viewport scrolls to keep the focused window visible, rather than resizing windows to fit a fixed screen width.

---

## Goals

- A workspace can be switched between **tiling** mode (current behaviour) and **scrolling** mode.
- A global default layout mode can be set in the user config.
- Each workspace config can override the default.
- In scrolling mode:
  - New windows are created at **50 % of the workspace width**.
  - Additional windows extend the virtual canvas to the right; they do not shrink existing windows.
  - Focusing left/right navigates between windows and scrolls the viewport.
  - **Maximising** a window expands it to 100 % of the visible viewport width, but `focus --direction left/right` (and `move --direction left/right`) still navigate among all windows on the virtual canvas.
- The feature is implemented with minimal changes to existing tiling behaviour.

---

## Inspiration — niri

Key niri concepts that map to this implementation:

| niri concept | GlazeWM equivalent |
|---|---|
| Infinite horizontal canvas | Virtual scrolling canvas tracked by `scroll_offset` on the workspace |
| Column width (default 50 %) | `tiling_size` set to `0.5` when a window is created in scrolling mode |
| Viewport scrolling | Recalculate `scroll_offset` on every focus change in scrolling mode |
| Full-width column | `tiling_size` set to `1.0` (window-level "maximised" in scrolling mode) |

---

## Configuration

### 1. New `LayoutMode` enum (`wm-common`)

```rust
// packages/wm-common/src/layout_mode.rs
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    #[default]
    Tiling,
    Scrolling,
}
```

Export from `wm-common/src/lib.rs`.

### 2. Extend `GeneralConfig` (`wm-common/src/parsed_config.rs`)

Add a global default:

```rust
pub struct GeneralConfig {
    // ...existing fields...
    /// Default layout mode applied to all workspaces unless overridden.
    pub default_layout: LayoutMode,
}
```

Default value: `LayoutMode::Tiling` (backwards compatible).

### 3. Extend `WorkspaceConfig` (`wm-common/src/parsed_config.rs`)

Add a per-workspace override:

```rust
pub struct WorkspaceConfig {
    pub name: String,
    pub display_name: Option<String>,
    pub bind_to_monitor: Option<u32>,
    pub keep_alive: bool,
    /// Overrides `general.default_layout` for this workspace.
    pub layout: Option<LayoutMode>,
}
```

### 4. Sample config additions (`resources/assets/sample-config.yaml`)

```yaml
general:
  # ...
  # Default layout mode for all workspaces: 'tiling' or 'scrolling'.
  default_layout: 'tiling'

workspaces:
  - name: '1'
  - name: '2'
    layout: 'scrolling'   # override for workspace 2
```

---

## Data Model Changes

### 5. Extend `WorkspaceInner` (`packages/wm/src/models/workspace.rs`)

```rust
struct WorkspaceInner {
    // ...existing fields...
    layout_mode: LayoutMode,
    /// Horizontal scroll offset in logical pixels.
    scroll_offset: i32,
}
```

Add public accessors:

```rust
pub fn layout_mode(&self) -> LayoutMode { self.0.borrow().layout_mode.clone() }
pub fn set_layout_mode(&self, mode: LayoutMode) { self.0.borrow_mut().layout_mode = mode; }
pub fn scroll_offset(&self) -> i32 { self.0.borrow().scroll_offset }
pub fn set_scroll_offset(&self, offset: i32) { self.0.borrow_mut().scroll_offset = offset; }
```

Initialise from config in `Workspace::new(...)`.

### 6. Extend `WorkspaceDto` (`packages/wm-common/src/dtos/workspace_dto.rs`)

```rust
pub struct WorkspaceDto {
    // ...existing fields...
    pub layout_mode: LayoutMode,
    pub scroll_offset: i32,
}
```

Update `Workspace::to_dto()` to populate the new fields.

---

## Window Placement — Scrolling Mode

### 7. Update `manage_window` (`packages/wm/src/commands/window/manage_window.rs`)

In `create_window`, after the workspace is identified, check the workspace's `layout_mode`. When it is `LayoutMode::Scrolling`:

1. Always insert the new window as the last child of the workspace (ignore the split-container nesting logic used by tiling mode).
2. Set `tiling_size = 0.5` on the new `TilingWindow` so it occupies 50 % of the viewport width.

```rust
if target_workspace.layout_mode() == LayoutMode::Scrolling {
    window.set_tiling_size(0.5);
}
```

> **Note:** In scrolling mode the workspace tiling direction is always `Horizontal`. The `tiling_size` field already exists and is used by the position-calculation macro.

---

## Position Calculation — Scrolling Mode

### 8. Introduce `ScrollingWorkspace::to_rect` behaviour

The existing `impl_position_getters_as_resizable!` macro calculates a window rect relative to its parent. In scrolling mode the parent is the workspace, and windows are laid out on a virtual canvas wider than the viewport.

**Virtual canvas width** = sum of all window widths + inner gaps.

**Window rect (scrolling):**

```
x = workspace.x() - scroll_offset + sum of widths of preceding siblings + preceding_gap_count * inner_gap
y = workspace.y()
width = tiling_size * workspace.width()
height = workspace.height()
```

Only the portion that falls within `[workspace.x(), workspace.x() + workspace.width()]` is visible; the OS will clip anything outside the monitor boundary automatically (or we clamp at draw time).

**Viewport scrolling rule** (applied whenever focus changes in scrolling mode):

```
// Target: keep the focused window fully visible.
let window_left  = focused_window_virtual_x;
let window_right = window_left + focused_window_width;
let viewport_left  = scroll_offset;
let viewport_right = scroll_offset + workspace_width;

if window_right > viewport_right {
    new_offset = window_right - workspace_width;
} else if window_left < viewport_left {
    new_offset = window_left;
}
scroll_offset = new_offset.max(0);
```

This logic should live in a helper function `update_scroll_offset(workspace, focused_window)` called from:
- `focus_in_direction` (scrolling path)
- `manage_window` (after a new window is inserted)
- `platform_sync` (after any redraw in scrolling mode)

---

## Focus & Navigation — Scrolling Mode

### 9. Update `focus_in_direction` (`packages/wm/src/commands/container/focus_in_direction.rs`)

When the focused container's workspace has `layout_mode == LayoutMode::Scrolling` and the direction is `Left` or `Right`:

1. Find the previous/next tiling sibling of the focused window within the workspace (the flat list; no split containers are used in scrolling mode).
2. Set that sibling as the focused descendant.
3. Call `update_scroll_offset(workspace, new_focused_window)` to recalculate the viewport position.
4. Queue a redraw of all workspace children.

```rust
if workspace.layout_mode() == LayoutMode::Scrolling
    && matches!(direction, Direction::Left | Direction::Right)
{
    // scrolling-mode navigation
    ...
    return Ok(());
}
// fall-through to existing tiling logic
```

### 10. Update `move_window_in_direction` (`packages/wm/src/commands/window/move_window_in_direction.rs`)

When the workspace is in scrolling mode and the direction is `Left` or `Right`:

1. Swap the window with its immediate left/right sibling in the flat workspace child list.
2. Recalculate scroll offset so the moved window stays visible.
3. Queue a full workspace redraw.

---

## Maximised Windows in Scrolling Mode

### 11. Scrolling-mode "maximise"

Scrolling mode introduces a lightweight "wide" state distinct from the WM's fullscreen/maximised states. This is implemented by setting `tiling_size = 1.0` on the window while leaving it as a `TilingWindow`.

- `set-scrolling-maximized` (new `InvokeCommand` variant, or reuse `Size --width 100%`) sets `tiling_size = 1.0`.
- `toggle-scrolling-maximized` toggles between `0.5` and `1.0`.
- The existing left/right navigation continues to work because the window remains in the flat sibling list.

Alternatively, re-use the existing `Resize` command with `--width 100%`.

---

## New Commands

### 12. `SetLayoutMode` command (`wm-common/src/app_command.rs`)

```rust
SetLayoutMode {
    #[clap(required = true)]
    mode: LayoutMode,
},
ToggleLayoutMode,
```

Handler in `packages/wm/src/commands/workspace/set_layout_mode.rs`:

```rust
pub fn set_layout_mode(
    workspace: Workspace,
    mode: LayoutMode,
    state: &mut WmState,
) -> anyhow::Result<()> {
    workspace.set_layout_mode(mode);
    workspace.set_scroll_offset(0);
    // Re-distribute tiling sizes:
    //   - switching to scrolling: set all tiling children to 0.5
    //   - switching to tiling:    set all tiling children to equal shares
    state.pending_sync.queue_containers_to_redraw(workspace.tiling_children());
    Ok(())
}
```

---

## IPC / Events

### 13. Extend `WmEvent` (`wm-common/src/wm_event.rs`)

Add a new variant:

```rust
WorkspaceLayoutChanged {
    changed_workspace: WorkspaceDto,
},
```

Emit this event from `set_layout_mode`.

### 14. Extend `SubscribableEvent` (`wm-common/src/app_command.rs`)

```rust
WorkspaceLayoutChanged,
```

---

## Pending Sync

### 15. `pending_sync` scroll offset update

Add a method `queue_scroll_offset_update(workspace: Workspace)` to `PendingSync`. This will call `update_scroll_offset` for the workspace's focused window during the next sync cycle (`platform_sync`).

---

## Implementation Steps (Ordered)

The following is the recommended implementation order to allow incremental testing:

1. **[Step 1]** Add `LayoutMode` enum to `wm-common` and re-export it from `lib.rs`.
2. **[Step 2]** Add `default_layout: LayoutMode` to `GeneralConfig` and `layout: Option<LayoutMode>` to `WorkspaceConfig` in `parsed_config.rs`.
3. **[Step 3]** Add `layout_mode` and `scroll_offset` fields to `WorkspaceInner` and `WorkspaceDto`. Update `Workspace::new`, accessors, and `to_dto`.
4. **[Step 4]** Update `activate_workspace` to read `layout_mode` from workspace config (falling back to `general.default_layout`).
5. **[Step 5]** Update `manage_window` / `create_window` to set `tiling_size = 0.5` for new windows in scrolling mode.
6. **[Step 6]** Implement `update_scroll_offset` helper function.
7. **[Step 7]** Update `impl_position_getters_as_resizable!` (or add a scrolling-specific path in `TilingWindow::to_rect` / `Workspace::to_rect`) to apply `scroll_offset` when computing x-coordinates.
8. **[Step 8]** Update `focus_in_direction` to navigate in scrolling mode and call `update_scroll_offset`.
9. **[Step 9]** Update `move_window_in_direction` for scrolling mode swap behaviour.
10. **[Step 10]** Add `SetLayoutMode` and `ToggleLayoutMode` `InvokeCommand` variants, and implement their handlers in `packages/wm/src/commands/workspace/`.
11. **[Step 11]** Wire new commands into the command dispatcher (`wm.rs` / `ipc_server.rs`).
12. **[Step 12]** Emit `WorkspaceLayoutChanged` WM event and add `WorkspaceLayoutChanged` to `SubscribableEvent`.
13. **[Step 13]** Update `sample-config.yaml` with `default_layout` and a commented-out per-workspace example.
14. **[Step 14]** Add unit tests for `update_scroll_offset` and the new `LayoutMode` parsing.

---

## Acceptance Criteria

| # | Criterion |
|---|---|
| AC-1 | Setting `default_layout: 'scrolling'` in `general` config makes all workspaces use scrolling mode by default. |
| AC-2 | A workspace with `layout: 'tiling'` uses tiling mode even when the global default is `'scrolling'`. |
| AC-3 | In scrolling mode, the first window opened occupies 50 % of the viewport width. |
| AC-4 | In scrolling mode, a second window is placed to the right of the first, also at 50 % width, and is initially fully visible (viewport scrolls right). |
| AC-5 | In scrolling mode, a third (and subsequent) window extends the virtual canvas; existing windows are **not** resized. |
| AC-6 | `focus --direction left` and `focus --direction right` navigate between scrolling windows and scroll the viewport to keep the focused window fully visible. |
| AC-7 | Resizing a window to 100 % width (`resize --width 100%`) fills the viewport, but `focus --direction left/right` still navigates to adjacent windows. |
| AC-8 | `set-layout-mode --mode scrolling` (or `toggle-layout-mode`) switches the current workspace's layout mode at runtime. |
| AC-9 | Switching a workspace from scrolling to tiling redistributes window sizes to equal shares. |
| AC-10 | Switching a workspace from tiling to scrolling sets all existing windows to 50 % width and resets the scroll offset. |
| AC-11 | `WorkspaceDto` contains `layoutMode` and `scrollOffset` fields visible via IPC. |
| AC-12 | The `workspace_layout_changed` event is emitted when the layout mode of a workspace changes. |
| AC-13 | All existing tiling behaviour is unaffected when workspaces use the default `tiling` layout mode. |

---

## File Change Summary

| File | Change |
|---|---|
| `packages/wm-common/src/layout_mode.rs` | **New file** — `LayoutMode` enum |
| `packages/wm-common/src/lib.rs` | Export `LayoutMode` |
| `packages/wm-common/src/parsed_config.rs` | Add `default_layout` to `GeneralConfig`; add `layout` to `WorkspaceConfig` |
| `packages/wm-common/src/dtos/workspace_dto.rs` | Add `layout_mode`, `scroll_offset` |
| `packages/wm-common/src/wm_event.rs` | Add `WorkspaceLayoutChanged` event variant |
| `packages/wm-common/src/app_command.rs` | Add `SetLayoutMode`, `ToggleLayoutMode` command variants; add `WorkspaceLayoutChanged` to `SubscribableEvent` |
| `packages/wm/src/models/workspace.rs` | Add `layout_mode`, `scroll_offset` to `WorkspaceInner`; add accessors; update `new` and `to_dto` |
| `packages/wm/src/commands/workspace/activate_workspace.rs` | Read `layout_mode` from config when creating workspace |
| `packages/wm/src/commands/window/manage_window.rs` | Set `tiling_size = 0.5` in scrolling mode; call `update_scroll_offset` |
| `packages/wm/src/commands/container/focus_in_direction.rs` | Add scrolling-mode navigation path; call `update_scroll_offset` |
| `packages/wm/src/commands/window/move_window_in_direction.rs` | Add scrolling-mode swap path |
| `packages/wm/src/commands/workspace/set_layout_mode.rs` | **New file** — handler for `SetLayoutMode` / `ToggleLayoutMode` |
| `packages/wm/src/commands/workspace/mod.rs` | Export `set_layout_mode` |
| `packages/wm/src/traits/position_getters.rs` | Apply `scroll_offset` to x-coordinate in scrolling mode |
| `packages/wm/src/wm.rs` | Dispatch `SetLayoutMode` / `ToggleLayoutMode` commands |
| `resources/assets/sample-config.yaml` | Add `default_layout` field and per-workspace `layout` example |
