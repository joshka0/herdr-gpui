//! Explains missing-glyph boxes in a prompt. Fonts are never bundled, so a
//! machine without a Nerd Font draws every Private Use Area icon as a box;
//! this names the fix once a pane actually shows one.
use crate::{config_diagnostic::ConfigDiagnostic, terminal_painter::CellSeparator};
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Names the fix without running it: installing fonts is the user's call.
const TEXT: &str = if cfg!(target_os = "macos") {
    "Prompt icons need a Nerd Font, and none is installed.\n\
     brew install --cask font-symbols-only-nerd-font\n\
     Then restart Herdr GPUI."
} else {
    "Prompt icons need a Nerd Font, and none is installed.\n\
     Install Symbols Nerd Font Mono from nerdfonts.com,\n\
     then restart Herdr GPUI."
};

#[derive(Debug, Default)]
pub(crate) struct IconFontNotice {
    /// Latched once a frame showed an icon, so a prompt that scrolls away
    /// does not take the explanation with it.
    shown_icon: bool,
    /// The last frame scanned, by revision, so a tick without a new frame
    /// costs nothing.
    scanned: Option<(u64, u64)>,
    card: ConfigDiagnostic,
}

impl IconFontNotice {
    /// Follows the resolved config and the frame on screen. Frames are scanned
    /// only while the terminal has no icon font and no icon was seen yet.
    /// Returns whether the card's visibility changed.
    pub(crate) fn observe(&mut self, missing: bool, surface: Option<&PaneSurfaceFrame>) -> bool {
        let before = self.visible().is_some();
        if missing
            && !self.shown_icon
            && let Some(surface) = surface
        {
            let revision = (surface.projection_revision, surface.surface_revision);
            if self.scanned != Some(revision) {
                self.scanned = Some(revision);
                self.shown_icon = shows_icon(&surface.frame);
            }
        }
        self.card.sync((missing && self.shown_icon).then_some(TEXT));
        before != self.visible().is_some()
    }

    pub(crate) fn visible(&self) -> Option<&Arc<[String]>> {
        self.card.visible()
    }

    pub(crate) fn dismiss(&mut self, lines: &Arc<[String]>) -> bool {
        self.card.dismiss(lines)
    }
}

/// Whether any drawn cell needs an icon font. The solid separators are
/// painted as paths, so they draw without one.
fn shows_icon(frame: &FrameData) -> bool {
    frame
        .cells
        .iter()
        .filter(|cell| !cell.skip && CellSeparator::from_symbol(&cell.symbol).is_none())
        .any(|cell| cell.symbol.chars().any(is_private_use))
}

fn is_private_use(c: char) -> bool {
    matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}
