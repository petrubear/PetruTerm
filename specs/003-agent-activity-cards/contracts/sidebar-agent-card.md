# Contract: Agents card appearance

Supersedes the row layout of `specs/001-agents-sidebar-tab/contracts/sidebar-agents-ui.md`. Click, right-click, rename, ordering and the scan lifecycle are unchanged.

## Card

A raised, rounded card with a 1 px outline; cards are separated by a gap.

| Part | Rule |
|------|------|
| Background | `ui_surface_hover` (`ui_surface_active` when selected) with the state tint over it |
| Outline | `ui_border` (the agent's own color when selected (`ui_accent` if it has none)) |
| Left edge | 3 px strip inside the card, opaque state color, clipped to the rounded corners; transparent for idle/unreported |
| Padding | content is inset past the edge so text never shifts between states |
| Overflow | the card clips; every text line truncates with an ellipsis; nothing is drawn outside the card |

## Lines (top to bottom)

1. **Head**: state dot (state color; orange when unread/needs-input), name (custom name or agent type, in the agent's color), `msg` badge when accepting messages, then the age at the far right.
2. **Status**: agent type, the state word in the state color (`working`, `needs input`, `done`, `idle`, `running`), the short session id when known.
3. **Body** (only when there is text): the live activity while working (level per `agents.activity`), otherwise the last message.
4. **Location**: `workspace / tab`, with ` #N` when the tab has more than one pane.

Rename errors appear under line 1.

## State color

| State | Tint (alpha 0.12) | Edge / state word / dot |
|-------|-------------------|-------------------------|
| working | `ui_accent` | `ui_accent` |
| needs input | `ATTENTION_COLOR` | `ATTENTION_COLOR` |
| done | `ui_success` | `ui_success` |
| idle / unreported | none | none (muted) |

Text on the tinted card keeps its contrast: readable text (4.5:1) stays readable; text already below that gets no more than 10% worse (enforced by a unit test for both shipped themes).
