use std::str::FromStr;

use anyhow::bail;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Layout mode for a workspace.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, ValueEnum)]
#[clap(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
  /// Standard tiling layout where windows share the available screen space.
  #[default]
  Tiling,
  /// Infinite horizontal scrolling layout inspired by niri. Windows are
  /// placed side-by-side on a virtual canvas that extends beyond the
  /// screen width, and the viewport scrolls to keep the focused window
  /// visible.
  Scrolling,
}

impl FromStr for LayoutMode {
  type Err = anyhow::Error;

  fn from_str(unparsed: &str) -> anyhow::Result<Self> {
    match unparsed {
      "tiling" => Ok(Self::Tiling),
      "scrolling" => Ok(Self::Scrolling),
      _ => bail!("Not a valid layout mode: {}", unparsed),
    }
  }
}
