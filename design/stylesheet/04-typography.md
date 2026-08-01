# 04 — Typography

Governs: type scale, weights, line height, minimum sizes, emphasis mechanism, scaling behavior.

## What the sources publish

### Minimum and default sizes `[HIG]`

> "Each platform has different default and minimum sizes for system-defined type styles to promote readability. If you're using custom type styles, follow the recommended defaults."

| Platform | Default size | Minimum size |
| --- | --- | --- |
| iOS, iPadOS | 17 pt | 11 pt |
| **macOS** | **13 pt** | **10 pt** |
| tvOS | 29 pt | 23 pt |
| visionOS | 17 pt | 12 pt |
| watchOS | 16 pt | 12 pt |

Plus: "If you're using a custom font with a thin weight, aim for larger than the recommended sizes to increase legibility." `[HIG]`

The iOS/macOS gap here is the same 44-vs-28 story as [01](01-spacing-and-density.md): Apple's desktop body text is **13 pt**, not 17 pt. A window full of 17 pt body text is an iOS decision, not an Apple-desktop decision.

### macOS built-in text styles — the complete scale `[HIG]`

Apple publishes this one in full. Point sizes assume "image resolution of 144 ppi for @2x designs".

| Text style | Weight | Size (pt) | Line height (pt) | Emphasized weight |
| --- | --- | --- | --- | --- |
| Large Title | Regular | 26 | 32 | Bold |
| Title 1 | Regular | 22 | 26 | Bold |
| Title 2 | Regular | 17 | 22 | Bold |
| Title 3 | Regular | 15 | 20 | Semibold |
| Headline | Bold | 13 | 16 | Heavy |
| Body | Regular | 13 | 16 | Semibold |
| Callout | Regular | 12 | 15 | Semibold |
| Subheadline | Regular | 11 | 14 | Semibold |
| Footnote | Regular | 10 | 13 | Semibold |
| Caption 1 | Regular | 10 | 13 | Medium |
| Caption 2 | Medium | 10 | 13 | Semibold |

Two things worth extracting from that table rather than eyeballing it:

- **Line height is roughly 1.2–1.3× size**, and it is *not* a constant multiplier — Apple pins each style individually (13/16 ≈ 1.23, 10/13 = 1.30, 26/32 ≈ 1.23). A single CSS `line-height: 1.25` is an approximation, not a reproduction. `[DERIVED]`
- **Emphasis is a separate weight per style**, not one bold. "The emphasized weights can be medium, semibold, bold, or heavy." `[HIG]` A single `font-weight: 700` for all emphasis flattens a distinction Apple makes deliberately.

### iOS Dynamic Type scale

The HIG hosts the iOS/iPadOS Dynamic Type table and a larger-accessibility-sizes table, but serves them in a form this investigation could not extract as text. The two anchors above (default 17 pt, minimum 11 pt) are the confirmed iOS numbers here; the full iOS ramp is **not reproduced in this file rather than reproduced from memory**. If the iOS frame is chosen, read the table from the source page directly.

### Fonts and scaling `[HIG]`

- SF Pro is the system font on both iOS/iPadOS and macOS. NY is also available (on macOS only via Mac Catalyst).
- **"macOS doesn't support Dynamic Type."** iOS, iPadOS, tvOS, visionOS and watchOS do.
- Accessibility target: "give people the option to enlarge text by at least **200 percent**" (140% on watchOS) `[HIG]`.
- Layout obligations when text grows `[HIG]`: verify legibility at all sizes; scale meaningful interface icons with the text; "Keep text truncation to a minimum as font size increases"; consider re-flowing layout at large sizes because "inline items (like glyphs and timestamps) and container boundaries can crowd text"; "Maintain a consistent information hierarchy regardless of the current font size."

That last cluster is directly relevant: a reminder row with title + due-date timestamp + flag glyph on one line is exactly the "inline items crowd text" case Apple warns about.

### Windows type ramp `[MS]`

Microsoft's spacing guidance references the ramp by role rather than by number: use Title / Subtitle / Body with 12 epx spacing; use **Body Strong instead of Title** in confined space, with no extra spacing between text blocks; use Caption "for very confined spaces where text is needed, such as command buttons"; for multi-line lists use Body + Caption with 32 epx icons, and Body Strong for section headers.

## Options

### Option A — iOS frame (F1): 17 pt body, Dynamic Type

- Pro: literal match to the aesthetic reference; Dynamic Type support is genuinely the most accessible answer and satisfies the 200% requirement structurally.
- Con: 17 pt body in a desktop window is large; paired with 44 pt rows against Option B's 28 pt, a given window height holds about **1.6× fewer rows** (44/28 ≈ 1.57) `[DERIVED]`. The full iOS ramp must be read from the source since it is not reproduced here.

### Option B — macOS frame (F2): 13 pt body, fixed ramp

- Pro: the entire scale is published `[HIG]`, including per-style line heights and per-style emphasis weights — the most completely sourced option in this whole folder. Correct for a 1–3 ft viewing distance.
- Con: macOS has no Dynamic Type, so **we** must provide the 200% enlargement path ourselves; a fixed ramp gives no built-in scaling and 10 pt captions are near the published minimum.

### Option C — macOS ramp + a user text-size setting

Option B's scale as the 100% baseline, with a user-controlled scale factor (CSS `rem` root scaling) up to 200%.

- Pro: keeps the fully-sourced desktop scale and satisfies the 200% accessibility target that Option B leaves unmet; a web stylesheet is unusually well-suited to root-relative scaling.
- Con: every fixed pixel value elsewhere in the stylesheet must then be `rem`-based or the layout breaks at 200% — a cross-cutting constraint on [01](01-spacing-and-density.md) and [03](03-shape-and-edges.md), not a local typography choice.

### Option D — Role-based, Windows-style

Define styles by role (Title / Subtitle / Body / Body Strong / Caption) and pick sizes to fit, following Microsoft's substitution rules for confined space.

- Pro: the substitution guidance ("use Body Strong instead of Title in confined space") is practical advice neither Apple frame gives, and a dense reminder list is exactly a confined space.
- Con: sizes end up `[UNSOURCED]`; loses the Apple type identity.

## Invariants

1. No text below the platform minimum: **11 pt** if the iOS frame is chosen, **10 pt** if macOS. `[HIG]`
2. Text can be enlarged to **200%** by some mechanism. On a macOS-derived fixed ramp this is our responsibility, not the system's. `[HIG]`
3. Information hierarchy is preserved at every text size; primary elements stay primary. `[HIG]`
4. Contrast thresholds are size- and weight-dependent — 4.5:1 up to 17 pt, 3:1 at 18 pt or bold. Changing the type scale changes which threshold applies. See [02](02-color-and-contrast.md). `[HIG]`
5. Line height is set per style, not by one global multiplier, if the macOS ramp is adopted.

## Open / unmeasured

- The iOS/iPadOS Dynamic Type ramp is not reproduced here (see above). `[UNSOURCED]` in this file, sourced at the HIG page.
- Framework7's iOS-theme type scale is currently uninventoried; whatever it ships is in force by default and has never been compared against either table above.
- Whether a reminder row can hold title + timestamp + flag on one line at 200% text without truncation — unmeasured, and the most likely place this axis breaks.

## Decision

*(empty)*

- Frame / option:
- Body size / ramp:
- Line-height policy:
- Enlargement mechanism:
- Date / who:
