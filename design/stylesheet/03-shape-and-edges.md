# 03 — Shape and edges

Governs: corner radius, curvature type, **how many edges of a component are drawn** (0 / 1 / 4), separators vs borders vs fills vs materials, nesting behavior, elevation.

This is the axis the brief called "注目コンポーネントに対する辺数". It splits into two questions that are usually confused:

- **Radius** — how round is a corner?
- **Edge count** — how many sides of a box are actually stroked, and if none, what does the grouping work instead?

## Corner radius

### Apple publishes no number — the rule is relational, not categorical

Apple's guidance is *concentricity*: a nested shape's radius is derived from its container, never chosen in isolation.

> "Across Apple platforms, the shape of the hardware informs the curvature, size, and shape of nested interface elements, including controls, sheets, popovers, windows, and more. Help maintain a sense of visual continuity in your interface by using rounded shapes that are concentric to their containers" `[HIG]`

> "A rounded corner of a rectangle is *concentric* relative to the container shape when the corner's radius shares a common center with the containing shape's rounded corner radius." `[API]` — `ConcentricRectangle`, iOS/iPadOS/macOS/tvOS/visionOS/watchOS **26.0+**

> "`ConcentricRectangle` automatically calculates each corner's radius relative to the container shape, so your view adapts correctly across devices and sizes **without hard-coded values**." `[API]`

Consequences, stated plainly:

- **No numeric radius exists to copy.** Apple delegates the value to `ConcentricRectangle` / `UICornerConfiguration` / `containerShape(_:)`. For CSS we have no such API, so we must supply the arithmetic ourselves.
- Apple states no formula. The standard concentric identity — **`inner_radius = outer_radius − gap`**, where `gap` is the padding between the two shapes — is `[DERIVED]`: it is the unique value that keeps the two arcs sharing a center. It is not quoted from Apple.
- Shape family: rounded rectangles are the primary control shape on iOS, iPadOS and macOS; circular on watchOS. Sheets use an increased radius vs earlier versions, and list/table sections use an increased radius "to match the curvature of controls" `[HIG]`.

One more Apple statement, useful but **scoped to visionOS** — do not silently generalize it:

> "In general, prefer circular or capsule-shape buttons. People's eyes tend to be drawn toward the corners in a shape, making it difficult to keep looking at the shape's center. The more rounded a button's shape, the easier it is for people to look steadily at it." `[HIG, visionOS section]`

The *rationale* is about human vision rather than about a headset, which makes it suggestive for other platforms — but treating it as cross-platform guidance is inference, not citation. `[UNSOURCED]` beyond visionOS.

### Windows publishes exact numbers

| Radius | Applies to | Tag |
| --- | --- | --- |
| **8 px** | Top-level containers: app windows, flyouts, dialogs (`ContentDialog`, `Flyout`, `MenuFlyout`, `TeachingTip`) | `[MS]` |
| **4 px** | In-page controls: `Button`, `CheckBox`, `ComboBox`, `TextBox`, `ListView`, list backplates | `[MS]` |
| **4 px** | Bar-shaped elements: `ProgressBar`, `ScrollBar`, `Slider` | `[MS]` |
| **4 px** | `ToolTip` — explicit exception to the 8 px overlay rule, "due to its small size" | `[MS]` |
| **0 px** | Straight edges that intersect other straight edges | `[MS]` |
| **0 px** | Window corners when snapped or maximized | `[MS]` |

Global resources: `ControlCornerRadius` (default 4 px), `OverlayCornerRadius` (default 8 px) `[MS]`.

Explicit do-not-round cases `[MS]`:
- Adjacent elements inside one container that touch (e.g. the two halves of a `SplitButton`) — "There should be no space when they contact."
- The side of a flyout that connects to the UI that invoked it.

The 0 px-when-maximized rule is a real obligation for a Tauri window on Windows and has no iOS analogue.

### Options — radius

| | Rule | Pro | Con |
| --- | --- | --- | --- |
| **A. Categorical (Fluent-style)** | Radius by component class: 8 for overlays, 4 for in-page, 0 on intersecting/maximized edges | Every value `[MS]`-sourced; trivially implementable as two CSS variables; native on the host OS | Ignores concentricity, so nested shapes will visibly disagree at the corners |
| **B. Relational (Apple-style)** | One root radius (from the window), all nested radii `= parent − gap`, floored at 0 | Matches what Apple actually asks for; nesting stays visually correct at any size | The arithmetic is ours `[DERIVED]`; needs discipline or it decays into arbitrary numbers; CSS has no automatic support |
| **C. Capsule-first for prominent actions** | Prominent/primary controls are capsules (`radius = height/2`); containers are rounded rects | Strongest "current Apple" read; a capsule is self-concentric so it never conflicts with its container | Capsules are wide; in a dense list they cost horizontal space and can read as tags rather than buttons |
| **D. Hybrid B+C** | Relational radii for containers, capsule for the one or two prominent actions per view | Reconciles Apple's nesting rule with its emphasis rule; matches the "one or two prominent buttons per view" cap in [02](02-color-and-contrast.md) | Most rules to hold in one's head; must be tokenized to survive |

## Edge count — how much of a box do you actually draw?

Apple names the available grouping mechanisms and pointedly does **not** privilege borders:

> "you might use negative space, background shapes, colors, materials, or separator lines to show when elements are related and to separate information into distinct areas. When you do so, ensure that content and controls remain clearly distinct." `[HIG]`

So there are five mechanisms — space, fill, color, material, stroke — and stroke is one of five, not the default. That reframes "how many edges" as "which mechanism, and only then how many strokes".

| | Approach | Edges drawn | Pro | Con |
| --- | --- | --- | --- | --- |
| **E0. Space + fill only** | Grouped-list idiom: a filled rounded container, rows separated by nothing but rhythm | 0 | Calmest, most iOS-like; nothing competes with content; no 1.4.11 obligation on a stroke that doesn't exist | Row boundaries can become ambiguous in dense lists; relies entirely on spacing discipline from [01](01-spacing-and-density.md) |
| **E1. Single separator** | One hairline between rows, inset to the text's leading edge | 1 | The classic iOS list read; cheapest possible boundary cue; inset communicates hierarchy for free | Hairlines are the first thing to fail contrast — must clear **3:1** per SC 1.4.11 if they carry meaning `[W3C]` |
| **E4. Bordered card** | Full stroke on all four sides | 4 | Unambiguous grouping; survives busy or user-colored content; robust under `forced-colors: active` | Heaviest; four strokes per card multiplies visual noise in a list; least Apple-like |
| **EM. Material / elevation** | No stroke; separation by translucency and layering | 0 | Matches the current Liquid Glass hierarchy model | Must degrade under `prefers-reduced-transparency` and `forced-colors`, so a fallback in E0/E1/E4 is required anyway |

**Practical consequence:** whichever is chosen, EM alone is never sufficient — a transparency-based boundary must have a non-transparency fallback. So the real decision is "which of E0/E1/E4 is the floor", with EM as an enhancement on top.

## Materials and elevation

Current Apple model `[HIG]`:

- **Liquid Glass** is the material for the *control/navigation layer* that floats above content. "Don't use Liquid Glass in the content layer" — use standard materials for content backgrounds instead.
- "Use Liquid Glass effects sparingly. … Limit these effects to the most important functional elements in your app."
- Two variants: **regular** (blurs and adjusts luminosity to keep foreground legible; "Most system components use this variant"; use it when there is significant text — alerts, sidebars, popovers) and **clear** (highly translucent; only for components over visually rich backgrounds such as photo/video).
- For clear glass over bright content: "consider adding a dark dimming layer of **35% opacity**". Not needed if the underlying content is already dark. `[HIG]`
- Thickness trade-off: "Thicker materials, which are more opaque, can provide better contrast for text… Thinner materials, which are more translucent, can help people retain their context."

SwiftUI's standard material ladder, most translucent to most opaque `[API]`: `.ultraThin` → `.thin` → `.regular` → `.thick` → `.ultraThick`, plus `.bar` (matches system toolbars, outside the ordering).

For a CSS app, the whole ladder collapses to `backdrop-filter: blur()` plus a background alpha; the named variants are a *vocabulary* to pick from, not values to copy — Apple publishes no blur radii. `[UNSOURCED]`

## Invariants

1. Any stroke, separator, or state indicator that carries meaning clears **3:1** against adjacent color. `[W3C]` SC 1.4.11
2. Nested shapes never disagree at the corner: either all radii derive from a container (Option B/D) or all come from one categorical table (Option A) — never both in one view.
3. Window corners are square when snapped or maximized on Windows. `[MS]`
4. No translucency-only boundary. Every material-based separation has a fallback for `prefers-reduced-transparency: reduce` and `forced-colors: active`.
5. Liquid Glass–style treatment stays out of the content layer and is limited to the most important functional elements. `[HIG]`

## Open / unmeasured

- Numeric corner radii for iOS/macOS — not published by Apple in any form. `[UNSOURCED]`
- Blur radii / material opacity values — not published. `[UNSOURCED]`
- Hairline separator thickness and inset — not published; Framework7's iOS theme supplies its own, currently uninventoried.

## Decision

*(empty)*

- Radius option:
- Root radius / radius table:
- Edge floor (E0 / E1 / E4):
- Material usage:
- Date / who:
