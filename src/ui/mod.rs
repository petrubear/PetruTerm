pub mod context_menu;
pub mod info_overlay;
pub mod palette;
pub mod panes;
pub mod search_bar;
pub mod sidebar;
pub mod status_bar;
pub mod tabs;

pub use context_menu::{ContextAction, ContextMenu};
pub use info_overlay::InfoOverlay;
pub use palette::CommandPalette;
pub use panes::{PaneInfo, PaneManager, PaneSeparator, Rect, SplitDir};
pub use search_bar::SearchBar;
pub use sidebar::SidebarState;
pub use tabs::{Tab, TabManager};
