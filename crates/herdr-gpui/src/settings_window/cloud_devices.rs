//! Settings > Cloud Devices: one card per cloud provider whose machines this
//! app uses as devices. Each provider's card keeps its own fields and account
//! state; this section only lists them, so another provider adds a card.

#[cfg(feature = "coder")]
mod coder;

#[cfg(feature = "coder")]
pub(super) use coder::CoderCard;

use super::SettingsWindow;
use crate::cloud::CloudProvider;
use gpui::{prelude::*, *};

impl SettingsWindow {
    /// Build each provider's card on first view, then refresh what it reads.
    pub(super) fn open_cloud_devices(&mut self, cx: &mut Context<Self>) {
        for &provider in CloudProvider::ALL {
            match provider {
                #[cfg(feature = "coder")]
                CloudProvider::Coder => self.open_coder_card(cx),
            }
        }
    }

    /// Called after the config file reloads, so each card follows an account
    /// that was just saved or edited by hand.
    pub(super) fn cloud_config_changed(&mut self, cx: &mut Context<Self>) {
        for &provider in CloudProvider::ALL {
            match provider {
                #[cfg(feature = "coder")]
                CloudProvider::Coder => self.coder_config_changed(cx),
            }
        }
    }

    pub(super) fn render_cloud_devices(&self, cx: &mut Context<Self>) -> Div {
        let mut section = div().flex().flex_col().gap(px(24.)).min_w_0();
        for &provider in CloudProvider::ALL {
            section = section.child(match provider {
                #[cfg(feature = "coder")]
                CloudProvider::Coder => self.render_coder_card(cx),
            });
        }
        section
    }
}
