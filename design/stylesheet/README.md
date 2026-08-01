# Stylesheet constraints

This folder records, separately:

1. the **governing variables** of the app's visual style,
2. the **documented options** for each variable, with an external source for every number,
3. the **decision** this project has made.

Keeping (2) and (3) apart is the point. A governing variable with no recorded decision must surface as *undecided*, never be filled in at implementation time with a plausible-sounding default. This is the same discipline `/artist` applies to screen-level design; these files are the type-A grounding source its tables cite.

## Reference frames

This app is a **Windows desktop app wearing an iOS design language** (Tauri + Framework7 iOS theme). That makes "follow iOS" underdetermined, because iOS metrics assume a fingertip. Three frames are therefore kept distinct:

| Frame | What it is | What it is good for |
| --- | --- | --- |
| **F1 — iOS (touch)** | Apple HIG iOS/iPadOS numbers | The *aesthetic*. Metrics assume a finger and ~1 ft viewing distance. |
| **F2 — macOS (desktop)** | Apple HIG macOS numbers | Same design language, pointer input, **1–3 ft viewing distance**. The frame whose *metrics* match our input model. |
| **F3 — Windows (Fluent)** | Microsoft Learn / WinUI numbers | The platform we actually run on. Relevant to window-level geometry and to not feeling alien. |

Two non-negotiable **floors** sit under all three:

- **WCAG 2.2** — normative accessibility minimums. These are not options.
- **CSS user-preference media features** — the mechanism by which OS accessibility settings (Dark Mode, Increase Contrast, Reduce Motion, Reduce Transparency) actually reach a web-tech app. Without these, honoring the HIG is impossible in principle.

**Rule:** choose a frame *per axis*, record it, and never mix frames inside one axis without saying so explicitly.

## Confidence tags

Every number in these files carries one:

| Tag | Meaning |
| --- | --- |
| `[HIG]` | Stated in Apple's Human Interface Guidelines |
| `[API]` | Stated in Apple framework API documentation (verifiable in code) |
| `[MS]` | Stated in Microsoft Learn (Windows/WinUI/Fluent) |
| `[W3C]` | Stated in a W3C normative specification |
| `[AND]` | Stated in Android developer documentation |
| `[DERIVED]` | Arithmetic on cited numbers only — no taste added |
| `[UNSOURCED]` | **No source publishes this.** A human must decide and record it. |

`[UNSOURCED]` is the most important tag here. The investigation turned up a result worth stating plainly up front:

> **Several of the most widely quoted "iOS numbers" are not published by Apple at all.** The HIG Layout page contains no spacing scale, no grid unit, and no margin values for iOS or macOS — its only numeric content is for tvOS (60/80 pt insets) and visionOS (60 pt button centers). The "8 pt grid" and the "16 pt gutter" are community convention, not Apple specification. Apple also explicitly instructs *against* hard-coding its system color values.

So for a CSS-based app, a large part of "the iOS style" cannot be copied — it has to be **chosen**. That is why every axis file below ends in options plus an empty Decision line rather than a single prescribed number.

## Files

| File | Governing variables |
| --- | --- |
| [`01-spacing-and-density.md`](01-spacing-and-density.md) | base unit, component padding, inter-control gap, container margins, row height, information density |
| [`02-color-and-contrast.md`](02-color-and-contrast.md) | palette structure, semantic roles, light/dark/high-contrast triplets, contrast ratios |
| [`03-shape-and-edges.md`](03-shape-and-edges.md) | corner radius, curvature type, borders vs separators vs fills, nesting/concentricity, elevation & materials |
| [`04-typography.md`](04-typography.md) | type scale, weights, line height, minimum sizes, scaling behavior |
| [`05-motion-and-presentation.md`](05-motion-and-presentation.md) | presentation form per action, animation budget, reduced-motion behavior |
| [`sources.md`](sources.md) | every source used, with what it does and does **not** publish |

## How to use this with `/artist`

- `/artist` Gate 1's tables cite these files as **source type A (product value / documented constraint)**.
- An axis whose **Decision** line is still empty is an **E (ungrounded)** for `/artist` purposes: it must be surfaced to a human, not defaulted.
- When a decision is made, record it on the Decision line *with the frame chosen* (`F1`/`F2`/`F3`) and the date. That converts the axis from E to A for every subsequent design pass.
