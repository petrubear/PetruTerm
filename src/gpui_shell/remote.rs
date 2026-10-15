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
