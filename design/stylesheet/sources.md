# Sources

Every external source used, with what it **does** and **does not** publish. Retrieved 2026-08-01.

Recording the negative half matters as much as the positive half: most of the wrong numbers in circulation exist because someone assumed a source said something it never said.

## Apple — Human Interface Guidelines

| Page | URL | Publishes | Does **not** publish |
| --- | --- | --- | --- |
| Layout | https://developer.apple.com/design/human-interface-guidelines/layout | tvOS insets (60/80 pt); visionOS button centers (60 pt); the five grouping mechanisms (space, background shapes, colors, materials, separator lines); adaptability and safe-area principles; macOS notes (avoid controls at window bottom, avoid camera housing) | **Any spacing scale, grid unit, or margin value for iOS or macOS.** No 8 pt grid. No 16 pt gutter. |
| Accessibility | https://developer.apple.com/design/human-interface-guidelines/accessibility | Control sizes per platform (iOS 44×44 default / 28×28 min; **macOS 28×28 default / 20×20 min**); text default/minimum per platform (iOS 17/11, macOS 13/10); contrast table (≤17 pt → 4.5:1, 18 pt → 3:1, bold → 3:1); padding guidance (~12 pt bezeled, ~24 pt unbezeled); 200% enlargement target | Per-component metrics; list row heights |
| Color | https://developer.apple.com/design/human-interface-guidelines/color | 12 system hues × light/dark × default/increased-contrast; iOS system grays 1–6 × same; accent-color policy; the instruction **not** to hard-code values; the prohibition on redefining semantic color meanings | **Values for the semantic colors** (`label`, `separator`, `systemBackground`, elevated variants) |
| Typography | https://developer.apple.com/design/human-interface-guidelines/typography | **Complete macOS text-style table** (size + line height + emphasized weight per style, at 144 ppi @2x); system fonts; "macOS doesn't support Dynamic Type"; Dynamic Type layout obligations | iOS Dynamic Type ramp *in extractable text form* — the table is on the page but was not retrievable as text here |
| Materials | https://developer.apple.com/design/human-interface-guidelines/materials | Liquid Glass layer rules (control layer only, never content layer); regular vs clear variants and when each applies; 35% dark dimming layer for clear glass over bright content; thickness/contrast trade-off | Blur radii; opacity values for the standard material ladder |
| Buttons | https://developer.apple.com/design/human-interface-guidelines/buttons | 44×44 pt hit region (60×60 visionOS); "keep the number of prominent buttons to one or two per view"; style-not-size for emphasis; macOS button types (push / square / help); visionOS size table (28/32/44/52/64 pt) and shape guidance | iOS/macOS button corner radii or padding values. **The Mini/Small/Regular/Large/Extra-large size table is visionOS-scoped** — it is frequently misquoted as an iOS or macOS table |
| Motion | https://developer.apple.com/design/human-interface-guidelines/motion | "avoid adding motion to UI interactions that occur frequently"; "let people cancel motion"; brevity/precision; make motion optional; realistic/mirrored feedback | Durations; easing curves |
| Dark Mode | https://developer.apple.com/design/human-interface-guidelines/dark-mode | Minimum 4.5:1; 7:1 target for custom colors especially small text; base vs elevated background behavior | Numeric background values |
| Designing for macOS | https://developer.apple.com/design/human-interface-guidelines/designing-for-macos | Viewing distance "about 1 to 3 feet"; input-mode expectations; density guidance ("more content in fewer nested levels… comfortable information density") | Any metric |

> Retrieval note: HIG pages are client-rendered and return only a title to a plain fetch. The text above came from the JSON that backs the pages: `https://developer.apple.com/tutorials/data/design/human-interface-guidelines/<page>.json`. The system-color values are carried in the swatch images' `alt` attributes (`R-nnn,G-nnn,B-nnn`), which is how the tables in [02](02-color-and-contrast.md) were obtained.

## Apple — framework documentation

| API | URL | Publishes |
| --- | --- | --- |
| `ConcentricRectangle` | https://developer.apple.com/documentation/swiftui/concentricrectangle | Definition of concentric ("shares a common center with the containing shape's rounded corner radius"); automatic radius derivation "without hard-coded values"; available **26.0+** on all platforms |
| `Material` | https://developer.apple.com/documentation/swiftui/material | The named ladder `.ultraThin` → `.thin` → `.regular` → `.thick` → `.ultraThick`, plus `.bar` |
| `UIView.layoutMargins` | https://developer.apple.com/documentation/uikit/uiview/layoutmargins | Default for subviews is "8 points on each side"; a view controller's root view instead "reflects the system minimum margins and safe area insets" |
| `UIViewController.systemMinimumLayoutMargins` | https://developer.apple.com/documentation/uikit/uiviewcontroller/systemminimumlayoutmargins | That it acts as a floor (greater of custom vs system). **The "20 points" in the docs is an illustrative example, not the actual default** — the real values are not published |
| Adopting Liquid Glass | https://developer.apple.com/documentation/TechnologyOverviews/adopting-liquid-glass | Concentricity as the governing principle; rounded rectangles as the primary control shape on iOS/iPadOS/macOS; increased sheet and list-section radii; the concentricity APIs. **No formula and no numeric radii** |

## W3C

| Spec | URL | Publishes |
| --- | --- | --- |
| WCAG 2.2 SC 2.5.8 Target Size (Minimum), AA | https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html | "at least 24 by 24 CSS pixels", plus the Spacing / Equivalent / Inline / User Agent Control / Essential exceptions and the 24 px-diameter-circle spacing test |
| WCAG 2.2 SC 1.4.11 Non-text Contrast, AA | https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html | ≥ 3:1 for "visual information required to identify user interface components and states" and for meaningful parts of graphics |
| WCAG 2.2 SC 1.4.3 Contrast (Minimum), AA | https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum | ≥ 4.5:1 for text and images of text |
| Media Queries Level 5 | https://www.w3.org/TR/mediaqueries-5/ | `prefers-color-scheme: light \| dark`; `prefers-contrast: no-preference \| less \| more`; `prefers-reduced-motion: no-preference \| reduce`; `prefers-reduced-transparency: no-preference \| reduce`; `forced-colors: none \| active` |

## Microsoft

| Page | URL | Publishes |
| --- | --- | --- |
| Geometry in Windows 11 | https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry | 8 px top-level containers / 4 px in-page controls / 4 px bars / 4 px ToolTip exception / 0 px intersecting edges / 0 px snapped-or-maximized windows; `ControlCornerRadius` = 4, `OverlayCornerRadius` = 8; the do-not-round cases (SplitButton contact edge, flyout's connected side) |
| Content layout and spacing | https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics | The relational spacing ladder: 8 epx button↔button, button↔flyout, control↔header; 12 epx control↔label, content↔content; 16 epx surface edge↔text, control↔expander; 48 epx expander indent; 32 epx list icons; type-ramp substitution rules for confined space |
| Targeting | https://learn.microsoft.com/en-us/windows/apps/develop/input/guidelines-for-targeting | Touch target "7.5mm square range (40x40 pixels on a 135 PPI display at a 1.0x scaling plateau)"; the frequency/error-consequence adjustment factors |

> **Excluded claim.** A "compact sizing / 32×32 epx target grid" figure is widely repeated and appeared in search results attributed to the pages above. It is **not present** on the Microsoft page usually cited for it, so it is deliberately omitted from these files rather than reproduced. If that number is needed, its real source must be found first.

## Android

| Page | URL | Publishes |
| --- | --- | --- |
| Accessibility in apps | https://developer.android.com/guide/topics/ui/accessibility/apps | "we recommend that each interactive UI element have a focusable area, or touch target size, of at least 48dp x 48dp. Larger is even better." |

## Consulted but unusable

| Source | Why |
| --- | --- |
| Material Design 3 — Grids & spacing (`m3.material.io/foundations/layout/understanding-layout/spacing`) | Client-rendered; no text retrievable. Material's spacing scale is therefore **not** cited in these files. Widely repeated M3 figures (4 dp grid, 8/16/24/32/48 dp scale) were seen only in search summaries and are excluded on the same grounds as the Microsoft compact-density number. |
| Material Design 2 — Spacing methods (`m2.material.io/design/layout/spacing-methods.html`) | Same — client-rendered. |

## Internal references (not external sources)

These are cited in the axis files as project constraints, not as design authority:

- `design/idea/README.md` — the "traditional iOS app design pattern" instruction, the Speed value, and the Guardrail principle (data must stay usable in the real iOS app)
- `handan/0023-qa-feedback-comprehensive-backlog.md` — the QA backlog that identified the presentation-cost and field-coverage failures
- `core/src/reminders.rs` — the domain model, including the user-authored `color_hex` this stylesheet cannot control
- `gui/src/main.ts` — current implementation of the sheet presentation, flag toggle, and smart-list predicates
