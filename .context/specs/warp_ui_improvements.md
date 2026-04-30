# Warp UI Improvements — Chat Panel & Sidebar

Source studied: `/Users/edison/Documents/Projects/Github/warp`
Key files: `app/src/ai_assistant/transcript.rs`, `panel.rs`, `workspace/view/left_panel.rs`, `view_components/action_button.rs`

---

## W-1: Full-width message background tinting

**Priority: 1 — Low effort, high visual impact.**

**What Warp does** (`transcript.rs:536,597`): each message block gets a distinct full-width background. User messages → `surface_1`, assistant messages → `surface_2`. The entire row region is filled with that color. No text prefix needed for role identification — the bg does it.

**PetruTerm today**: text prefixes `"   You  "` / `"    AI  "` at `renderer.rs:1128-1129` + a 3px left accent bar.

**Implementation in `build_chat_panel_instances` (`renderer.rs`):**
- Track each message's start/end row after wrap.
- Before emitting glyph vertices for a message, push a full-width `RoundedRectInstance` (radius 0, just a flat rect) spanning all columns × all rows of that message.
- User messages: `colors.ui_surface` with a warm tint overlay (~8% alpha blend toward `ansi[3]` yellow or the user_fg color).
- Assistant messages: `colors.ui_surface` (base panel bg, no tint) or a cool 5% tint toward `ui_accent`.
- The text prefixes `"   You  "` / `"    AI  "` can be replaced with a shorter role indicator or dropped entirely once bg color distinguishes roles.

**Notes:**
- Do NOT remove the left accent bar — it adds a second visual cue for accessibility.
- The rect must be pushed before glyph vertices (painter's order) so glyphs render on top.

---

## W-2: Input box as a bordered card

**Priority: 2 — Low effort, clear UX improvement.**

**What Warp does** (`panel.rs:873-894`): the input editor is a `Container` with border `1px outline`, corner radius 4px, background `surface_2`, 16px uniform padding. It is a visually distinct card inside the panel, not just a region separated by a line.

**PetruTerm today**: input section separated from messages by a horizontal line (`sep_row`), with prefix `"  ▸  "`.

**Implementation in `build_chat_panel_instances` / `build_chat_panel_input_rows` (`renderer.rs`):**
- Emit a `RoundedRectInstance` background behind the input rows (`sep_row+1` to `screen_rows-2`) with:
  - `radius`: 4px (or 1 cell equivalent in pixel terms)
  - `bg`: slightly lighter than panel bg (e.g., `ui_surface` + 10% lighter blend)
  - `border`: 1px `ui_muted` color on all sides
- The separator line (`sep_row`) becomes the visual top gap above the card — keep it but make it thinner/dimmer (1px, `ui_muted` alpha 50%).
- The hint row (`screen_rows-1`) stays outside the card — it is a status/hint bar.

---

## W-3: Code block background + left accent bar

**Priority: 3 — Low effort, improves markdown readability.**

**What Warp does** (`transcript.rs:659-711`): code blocks are cards with:
- Border 1px `outline` color, corner radius 6px, padding 12px
- When selected: border becomes 1.5px accent color
- Copy/run action buttons inside the block

**PetruTerm today**: code blocks get syntax-highlighted text but no distinct background or border treatment in the cell grid.

**Implementation in `build_chat_panel_instances` (`renderer.rs`):**
- In the wrap cache, code block spans are annotated with `ParseState::CodeBlock` (or equivalent). Detect these when iterating message lines.
- For each code block span (start_row to end_row):
  - Push a `RoundedRectInstance` rect covering those rows at panel x-offset with `bg = ui_surface_active` (slightly distinct from message bg) and `radius 3px`.
  - Push a 2px-wide vertical `RoundedRectInstance` at the left edge of the panel content area, same height as the block, color = `ui_accent` at 80% alpha. This is a "language stripe" giving the block a distinctive left border.
- Add a small `[c]` hint at the end of the last code block line in `ui_muted` color — maps to existing copy keybind.

---

## W-4: Sidebar active/inactive color contrast

**Priority: 4 — Low effort, improves nav clarity.**

**What Warp does** (`left_panel.rs:850-905`): toolbelt/nav buttons use two icon color states:
- Active: `foreground` color (full brightness)
- Inactive: `sub_text_color(background)` (~50% muted)
- Active state also gets a `fg_overlay_3` background (translucent accent pill behind the item)

**PetruTerm today** (`renderer.rs:1880,1927,1981`): active rows get `sidebar_item_active_bg` rect. Section headers always render at the same brightness regardless of which section is active.

**Implementation in `build_workspace_sidebar_instances` (`renderer.rs`):**
- Section header text (WORKSPACES, MCP SERVERS, SKILLS, STEERING): when that section is active (`info_sidebar_section == N`), render header label in full `foreground` color. When inactive, render in `ui_muted` (foreground at 35% alpha).
- Section items: active section items in `foreground`, inactive sections items in `ui_muted`.
- The active-row bg pill (`sidebar_item_active_bg`) is good — keep it, but also change the text color of that specific row to `foreground` + bold-weight if the font supports it, or just brighter color.
- The 3px accent dot at `renderer.rs:1793-1796` is the right visual indicator — keep it only on the active item row, not on section headers.

---

## W-5: Zero state / empty panel

**Priority: 5 — Medium effort, improves first-open UX.**

**What Warp does** (`panel.rs:896-1015`): when `transcript.is_empty()`, renders a centered column:
- 44x44 AI logo icon (large)
- Subtitle: `"Ask a question below"` in muted color, 14px
- 3 example prompt pill buttons: "How do I undo commits?", "How do I find files?", "Write a script to..."

**PetruTerm today**: open panel just shows the header and empty space above the input.

**Implementation in `build_chat_panel_instances` (`renderer.rs`):**
- When `panel.messages.is_empty()` and `state == Idle`:
  - Calculate a vertical center in the message area.
  - Render 3 centered rows:
    - Row center-2: `"  ✦  "` in `ui_accent` color (large icon-like character), centered in panel width.
    - Row center-1: `"  Ask a question below  "` in `ui_muted`, centered.
    - Row center+1: `"  [ fix last error ]  "` as a pill row — background rect + text, mouse-clickable.
    - Row center+2: `"  [ explain command ]  "` same.
  - These pill rows are clickable: clicking pre-fills the input and optionally submits.
- State needed: `zero_state_hover: Option<u8>` (which suggestion is hovered) in `ChatPanel`.
- The pill rows use `RoundedRectInstance` with `bg = ui_surface_hover` and 4px radius.

---

## W-6: Header — icon anchor + right-aligned action buttons

**Priority: 6 — Medium effort, improves header scannability.**

**What Warp does** (`panel.rs:705-774`): header is a row:
- Left: 20x20 logo icon + title "Warp AI" (16px Semibold) + `Shrinkable` spacer
- Right (shown only when transcript non-empty): "Restart" text button + Copy icon + X close icon
- "Restart" button: 12px text, padding `4/4/8/8`, hover bg = `surface_3`, border-radius 4px
- Header floats as an overlay (not part of scroll area) via `Stack + OffsetPositioning`

**PetruTerm today**: single text row `"✦ AI provider:model [mcp:N skills:M]"` with all info concatenated.

**Implementation in `build_chat_panel_instances` (`renderer.rs`):**
- Keep row 0 as the header.
- Split into 3 zones:
  - Left zone (~10 cols): `"  ✦  "` icon char + model short-name in `ui_accent` → bright color.
  - Center zone (flexible): `provider:model` in `ui_muted` (dims after you know what it is).
  - Right zone (~15 cols): when messages exist, show `[↺]` (restart), `[⎘]` (copy), `[✕]` (close) as separately colored cells. Each is mouse-clickable via existing hit-testing. When no messages, show nothing or just `[✕]`.
- Map clicks in the right zone to existing `UiManager` actions (clear_history, copy_transcript, toggle_panel).
- Nerd Font: use `\u{e00b}` (AI/sparkle) or `\u{f4bc}` (robot) as the icon glyph if available in Monolisa.

---

## W-7: Prepared response pill buttons

**Priority: 7 — Medium effort, useful after long conversations.**

**What Warp does** (`transcript.rs:765-795`): after an assistant response completes, 3 pill buttons appear below the last message: "What should I do next?", "Show examples.", "How do I fix this?". These pre-fill the input and auto-submit.

**PetruTerm today**: no quick-action suggestions.

**Implementation:**
- State: add `show_suggestions: bool` on `ChatPanel` — set to `true` when `state` transitions from `Streaming` → `Idle`.
- In `build_chat_panel_instances`, when `show_suggestions && !messages.is_empty()`, render 2 pill rows immediately after the last assistant message (before the separator):
  - `"  [ Fix last error ]  "` in `ui_surface_hover` bg + `foreground` text
  - `"  [ Explain more ]  "` same
- These rows are mouse-clickable → fill input with the suggested text + submit.
- Clicking anywhere else in the panel or starting to type sets `show_suggestions = false`.
- The pill rows take up 2 extra rows — account for this in the message area height calculation.

---

## W-8: Resizable panel width via mouse drag

**Priority: 8 — High effort, high impact for power users.**

**What Warp does**: `ResizableStateHandle` with `DragBarSide::Left` on the AI panel (right-side panel). Min width 300px / ~30 cols. Max width 40% of window. Mouse drag on the left edge resizes live.

**PetruTerm today**: fixed `PANEL_COLS: u16 = 55` in `chat_panel.rs:8`.

**Implementation:**
- Add `panel_cols: u16` to `ChatPanel` (replaces the constant, default = 55, min = 30, max = 90).
- Add `panel_resize_drag: bool` to `UiManager` or `App`.
- In `handle_mouse` (`input/mod.rs`): detect when mouse is within 1-cell of the panel left edge while panel is open → show resize cursor hint (render the edge cell in `ui_accent`).
- On `MouseButton::Left` press at that edge: set `panel_resize_drag = true`.
- On `CursorMoved` while `panel_resize_drag`: update `panel.panel_cols = (screen_cols - cursor_col).clamp(30, 90)`, mark panel dirty.
- On `MouseButton::Left` release: clear `panel_resize_drag`.
- Replace all `PANEL_COLS` references with `panel.panel_cols` (or pass it through the render call).
- `ensure_wrap_cache` already rebuilds on width change (`width_cols` check at `chat_panel.rs:227`) — the resize will automatically re-wrap.

---

## Implementation Order

```
W-1 → W-2 → W-3 → W-4  (pure rendering, no state changes, do in one pass)
W-5 → W-6              (small state additions + render changes)
W-7                    (requires ChatPanel state + input wiring)
W-8                    (requires mouse input plumbing, do last)
```

W-1 through W-4 can all be done in a single focused session touching only `renderer.rs`.
