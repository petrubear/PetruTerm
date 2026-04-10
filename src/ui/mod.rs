pub mod panes;
pub mod palette;
pub mod tabs;
pub mod context_menu;
pub mod status_bar;
pub mod search_bar;

pub use panes::{PaneManager, PaneInfo, PaneSeparator, Rect, SplitDir};
pub use tabs::{Tab, TabManager};
pub use palette::CommandPalette;
pub use context_menu::{ContextMenu, ContextAction};
pub use search_bar::SearchBar;
