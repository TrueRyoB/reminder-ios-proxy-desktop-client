# 01 — Spacing and density

Governs: base unit, padding inside a component, gap between controls, container margins, list row height, hit-target size, overall information density.

## What the sources actually publish

### Apple — the negative result first

The HIG Layout page prescribes **no spacing scale, no grid unit, and no margin numbers** for iOS or macOS. Its only numeric content belongs to other platforms: tvOS "Inset primary content 60 points from the top and bottom of the screen, and 80 points from the sides", and visionOS "place buttons so their centers are at least 60 points apart" `[HIG]`.

What Apple *does* publish that bears on spacing:

| Item | Value | Tag |
| --- | --- | --- |
| Padding around elements **with** a bezel | "about 12 points" | `[HIG]` |
| Padding around elements **without** a bezel | "about 24 points … around the element's visible edges" | `[HIG]` |
| `UIView.layoutMargins` default (subviews) | "8 points on each side" | `[API]` |
| `systemMinimumLayoutMargins` (VC root view) | **not published** — the docs' "20 points" is an illustrative example, not the value | `[UNSOURCED]` |
| Control size — iOS/iPadOS | default **44×44 pt**, minimum **28×28 pt** | `[HIG]` |
| Control size — **macOS** | default **28×28 pt**, minimum **20×20 pt** | `[HIG]` |
| Control size — visionOS / watchOS / tvOS | 60×60 / 44×44 / 66×66 pt default | `[HIG]` |
| macOS viewing distance | "about 1 to 3 feet" | `[HIG]` |

The macOS row is the load-bearing one for this app: **Apple's own desktop metric is 28×28 pt, not 44×44 pt.** Copying 44 pt into a pointer-driven window produces a finger-sized UI that Apple does not ask for on desktop.

### Windows / Fluent

Spacing values, all in effective pixels `[MS]`:

| Between | Value |
| --- | --- |
| button ↔ button | 8 epx |
| button ↔ flyout | 8 epx |
| control ↔ header | 8 epx |
| control ↔ label | 12 epx |
| content area ↔ content area | 12 epx |
| surface edge ↔ text | 16 epx |
| control ↔ expander button | 16 epx |
| indent for controls inside an expander | 48 epx |
| icon size in multi-line lists | 32 epx |

Touch target: "set your touch target size to 7.5mm square range (40x40 pixels on a 135 PPI display at a 1.0x scaling plateau)" `[MS]`.

> Note: a "compact sizing / 32×32 epx target grid" figure circulates widely. It is **not** on the Microsoft page that is usually cited for it, so it is deliberately excluded here rather than repeated. `[UNSOURCED]`

### Other frames, for calibration

- Android: "at least 48dp x 48dp. Larger is even better." `[AND]`
- WCAG 2.2 SC 2.5.8 Target Size (Minimum), **Level AA**: "The size of the target for pointer inputs is at least 24 by 24 CSS pixels", with exceptions for *Spacing* (a 24 px diameter circle centered on each undersized target must not intersect another target's circle), *Equivalent*, *Inline*, *User Agent Control*, and *Essential* `[W3C]`.

## Invariants (hold regardless of the option chosen)

1. Every pointer target is **≥ 24×24 CSS px**, or satisfies the SC 2.5.8 spacing exception. `[W3C]`
2. Interactive elements are never packed tighter than Apple's stated padding guidance for their class (bezeled ≈ 12 pt, unbezeled ≈ 24 pt) without a recorded reason. `[HIG]`
3. The base unit is a single number used everywhere. Two competing units in one stylesheet is the failure this file exists to prevent.

## Options

### Option A — iOS touch frame (F1)

Base unit 8, targets 44, container margin 16.

- Pro: matches the aesthetic reference most literally; safest for any future touch/tablet target.
- Con: **on a Windows desktop window this wastes vertical space**, showing markedly fewer reminders per screen than Apple's own desktop apps. Directly at odds with `design/idea/README.md`'s Speed value (fewer items visible ⇒ more scrolling per task).
- Honesty note: the 8 and the 16 here are `[UNSOURCED]`. Only the 44 is `[HIG]`.

### Option B — macOS desktop frame (F2) — *closest to Apple's own desktop answer*

Base unit 4, targets 28 (min 20), padding 12 around bezeled controls.

- Every number traces to `[HIG]`/`[API]` except the base unit itself, which is `[DERIVED]` (4 is the largest integer dividing 28, 20, 12, and 24).
- Pro: correct input model, correct viewing distance, high information density, still unmistakably Apple.
- Con: 28 pt rows feel tight if the app is ever used by touch or on a high-DPI display at low scaling; requires care to keep ≥ 24 CSS px after any scaling.

### Option C — Windows-native frame (F3)

8/12/16 epx spacing ladder, 40×40 targets.

- Pro: every number is `[MS]`-sourced and unambiguous; feels native on the host OS; the ladder is already differentiated by *relationship* (8 = sibling controls, 12 = label/section, 16 = surface edge), which is more expressive than a single multiplier.
- Con: abandons the iOS aesthetic brief; 40 px targets are between B and A in density.

### Option D — Hybrid: Fluent's *relational* ladder at macOS *magnitudes*

Adopt F3's idea that spacing encodes relationship, but scale it to F2's metrics. E.g. sibling 4 / label 8 / group 12 / surface edge 16, targets 28.

- Pro: keeps the iOS look and desktop density while making spacing semantically meaningful rather than an arbitrary multiple.
- Con: `[DERIVED]`, not published by anyone. Must be recorded as an explicit project decision, and it is the option most likely to drift if not tokenized.

## Open / unmeasured

- **List row height.** No source publishes an iOS or macOS list row height. Framework7's iOS theme ships its own; whatever we use should be recorded here, not inherited silently. `[UNSOURCED]`
- **Density at scale.** How many reminder rows must be visible at once for the dashboard to work as a "priority action queue" (`handan/0023`) is a product question, not a style question — but it constrains this axis and is currently unmeasured.

## Decision

*(empty — see README: an empty Decision line is an `E` for `/artist` and must be raised with a human)*

- Frame:
- Base unit:
- Target size:
- Container margin:
- Row height:
- Date / who:
