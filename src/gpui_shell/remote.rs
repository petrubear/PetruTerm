//! `PaneView` over the gpui shell's terminals, so the remote host can list and attach.

use crate::remote::bridge::PaneView;
use crate::term::Terminal;
use remote_proto::msg::PaneInfo;
use std::collections::HashMap;
use std::rc::Rc;

pub(super) struct GpuiPanes<'a>(pub &'a HashMap<usize, Rc<Terminal>>);

impl PaneView for GpuiPanes<'_> {
    fn panes(&self) -> Vec<PaneInfo> {
        let mut ids: Vec<_> = self.0.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter()
            .map(|id| {
                let t = &self.0[&id];
                PaneInfo {
                    uid: t.pane_uid,
                    title: format!("terminal {id}"),
                    workspace: String::new(),
                    cols: t.cols.get(),
                    rows: t.rows.get(),
                    state: None,
                    unread: false,
                }
            })
            .collect()
    }

    fn terminal(&self, uid: u64) -> Option<&Terminal> {
        self.0.values().find(|t| t.pane_uid == uid).map(|t| &**t)
    }
}

impl super::GpuiShellRoot {
    /// Palette / `Leader R`: start remote access (and show the pairing QR) or stop it.
    pub(super) fn toggle_remote(&mut self, cx: &mut gpui::Context<Self>) {
        if crate::remote::manager().is_active() {
            crate::remote::manager().stop();
            self.remote_overlay.close();
            self.show_toast(
                "Remote access stopped.",
                std::time::Duration::from_secs(4),
                cx,
            );
            cx.notify();
        } else {
            self.pair_remote(cx);
        }
    }

    /// Palette "Remote: Pair a phone": start remote access if needed, then show a fresh QR.
    pub(super) fn pair_remote(&mut self, cx: &mut gpui::Context<Self>) {
        let m = crate::remote::manager();
        let result = m.start().and_then(|_| m.new_pairing_code());
        match result {
            Ok((code, relay)) => {
                self.remote_overlay
                    .open(code, relay, crate::remote::OFFER_TTL);
                cx.notify();
            }
            Err(e) => self.show_toast(
                format!("Remote access failed: {e}"),
                std::time::Duration::from_secs(6),
                cx,
            ),
        }
    }
}
