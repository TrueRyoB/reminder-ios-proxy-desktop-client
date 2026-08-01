# 02 — Color and contrast

Governs: palette structure, semantic role assignment, light/dark/increased-contrast variants, accent color policy, contrast ratios.

## The constraint that shapes this whole axis

> "Avoid hard-coding system color values in your app. Documented color values are for your reference during the app design process. The actual color values may fluctuate from release to release, based on a variety of environmental variables. Use APIs like [Color] to apply system colors." `[HIG]`

A native app obeys this by calling `Color.red` / `UIColor.label`. **A Tauri + CSS app cannot.** There is no API to call; a hex value must be written down. So this axis is not "which iOS colors do we use" — it is "how do we accept the drift Apple is warning us about". The options below differ mainly in *how* they absorb that drift.

Apple also requires four appearance contexts, not two:

> "Make sure all your app's colors work well in light, dark, and increased contrast contexts. … If you define a custom color, make sure to supply light and dark variants, and an increased contrast option for each variant" `[HIG]`

That is **light / dark / light+contrast / dark+contrast** — a 4-cell matrix per color, and the reason the tables below have four columns.

## Contrast requirements

Apple restates WCAG Level AA as its own guidance `[HIG]`:

| Text size | Text weight | Minimum contrast ratio |
| --- | --- | --- |
| Up to 17 pt | All | 4.5:1 |
| 18 pt | All | 3:1 |
| All | Bold | 3:1 |

Dark Mode raises the bar for custom colors:

> "At a minimum, make sure the contrast ratio between colors is no lower than 4.5:1. For custom foreground and background colors, strive for a contrast ratio of 7:1, especially in small text." `[HIG]`

W3C normative floors:

- SC 1.4.3 Contrast (Minimum), AA: text and images of text ≥ **4.5:1** `[W3C]`
- SC 1.4.11 Non-text Contrast, AA: "Visual information required to identify user interface components and states" and meaningful parts of graphics ≥ **3:1** against adjacent colors `[W3C]`

SC 1.4.11 is the one most often missed: a control's *boundary* must be discernible, not just its label. This matters directly for the flag toggle and checkbox in `gui/src/main.ts`, whose unfilled states are outline glyphs.

Apple also names an alternative metric — "Two popular standards of measure for color contrast are the [WCAG ratio] and the Accessible Perceptual Contrast Algorithm (APCA)" `[HIG]` — so a project may legitimately validate against APCA *in addition to*, never instead of, the WCAG floor (WCAG is what the law and SC 1.4.3 reference).

## Apple's published values (reference snapshot)

Extracted from the HIG Color page's own specification swatches. Values are RGB as Apple states them; hex is `[DERIVED]` by conversion only.

### System colors (unified across platforms; visionOS uses the dark values) `[HIG]`

| Hue | Light | Dark | Light +contrast | Dark +contrast |
| --- | --- | --- | --- | --- |
| Red | #FF383C (255,56,60) | #FF4245 (255,66,69) | #E9152D (233,21,45) | #FF6165 (255,97,101) |
| Orange | #FF8D28 (255,141,40) | #FF9230 (255,146,48) | #C55300 (197,83,0) | #FFA056 (255,160,86) |
| Yellow | #FFCC00 (255,204,0) | #FFD600 (255,214,0) | #A16A00 (161,106,0) | #FEDF43 (254,223,67) |
| Green | #34C759 (52,199,89) | #30D158 (48,209,88) | #008932 (0,137,50) | #4AD968 (74,217,104) |
| Mint | #00C8B3 (0,200,179) | #00DAC3 (0,218,195) | #008575 (0,133,117) | #54DFCB (84,223,203) |
| Teal | #00C3D0 (0,195,208) | #00D2E0 (0,210,224) | #008198 (0,129,152) | #3BDDEC (59,221,236) |
| Cyan | #00C0E8 (0,192,232) | #3CD3FE (60,211,254) | #007EAE (0,126,174) | #6DD9FF (109,217,255) |
| Blue | #0088FF (0,136,255) | #0091FF (0,145,255) | #1E6EF4 (30,110,244) | #5CB8FF (92,184,255) |
| Indigo | #6155F5 (97,85,245) | #6D7CFF (109,124,255) | #564ADE (86,74,222) | #A7AAFF (167,170,255) |
| Purple | #CB30E0 (203,48,224) | #DB34F2 (219,52,242) | #B02FC2 (176,47,194) | #EA8DFF (234,141,255) |
| Pink | #FF2D55 (255,45,85) | #FF375F (255,55,95) | #E7124D (231,18,77) | #FF8AC4 (255,138,196) |
| Brown | #AC7F5E (172,127,94) | #B78A66 (183,138,102) | #956D51 (149,109,81) | #DBA679 (219,166,121) |

### iOS/iPadOS system grays `[HIG]`

| Gray | Light | Dark | Light +contrast | Dark +contrast |
| --- | --- | --- | --- | --- |
| systemGray | #8E8E93 (142,142,147) | #8E8E93 (142,142,147) | #6C6C70 (108,108,112) | #AEAEB2 (174,174,178) |
| systemGray2 | #AEAEB2 (174,174,178) | #636366 (99,99,102) | #8E8E93 (142,142,147) | #7C7C80 (124,124,128) |
| systemGray3 | #C7C7CC (199,199,204) | #48484A (72,72,74) | #AEAEB2 (174,174,178) | #545456 (84,84,86) |
| systemGray4 | #D1D1D6 (209,209,214) | #3A3A3C (58,58,60) | #BCBCC0 (188,188,192) | #444446 (68,68,70) |
| systemGray5 | #E5E5EA (229,229,234) | #2C2C2E (44,44,46) | #D8D8DC (216,216,220) | #363638 (54,54,56) |
| systemGray6 | #F2F2F7 (242,242,247) | #1C1C1E (28,28,30) | #EBEBF0 (235,235,240) | #242426 (36,36,38) |

**Not published as values:** the semantic colors (`label`, `secondaryLabel`, `separator`, `systemBackground`, and the elevated background variants). The HIG names them and defines their *purpose* but publishes no numbers `[UNSOURCED]`. Apple's rule about them is a prohibition, and it is a real constraint on us:

> "Avoid redefining the semantic meanings of dynamic system colors. … don't use the [separator] color as a text color, or [label] color as a background color." `[HIG]`

## Accent color policy `[HIG]`

- "Keep the number of prominent buttons to one or two per view."
- "To emphasize primary actions, apply color to the background rather than to symbols or text."
- "Refrain from adding color to the background of multiple controls."
- If the app's content is largely monochromatic, a brand color as accent is effective; if content is colorful, "prefer a monochromatic appearance for toolbars and tab bars".
- Do not rely on color alone: "people who are color blind might not be able to distinguish some color combinations" `[HIG]`.

The last point has a concrete consequence here: list color and flag state are currently color-carried signals in `gui/src/main.ts`. Each needs a non-color redundancy (glyph, text, or position) to satisfy it.

## The delivery mechanism (mandatory, not optional)

Apple's four contexts reach a web app only through CSS user-preference media features `[W3C]`:

| Feature | Values | Maps to |
| --- | --- | --- |
| `prefers-color-scheme` | `light` \| `dark` | Light / Dark appearance |
| `prefers-contrast` | `no-preference` \| `less` \| `more` | Increase Contrast |
| `prefers-reduced-transparency` | `no-preference` \| `reduce` | Reduce Transparency |
| `forced-colors` | `none` \| `active` | Windows High Contrast / forced palette |

`forced-colors: active` is a Windows-specific obligation this app inherits from its host OS and that an iOS-only reading of the brief would miss entirely.

## Options

### Option A — Snapshot Apple's values as literal tokens

Copy the tables above into CSS custom properties, four contexts each.

- Pro: closest visual match to iOS; zero ambiguity for implementers.
- Con: knowingly violates Apple's own "don't hard-code" instruction, so the palette silently ages every OS release; and it still doesn't give us the semantic colors, which are the ones a list UI needs most.

### Option B — Semantic tokens, own values

Define our own token set by *role* (`--bg`, `--bg-elevated`, `--label`, `--label-secondary`, `--separator`, `--accent`, `--danger`), each with four context values chosen by us to pass the contrast floors.

- Pro: honors Apple's semantic model, which is the part Apple actually asks for; no drift problem because we never claimed to be Apple's values; testable — each token pair has a measurable ratio.
- Con: the visual match to iOS is *our* judgment, so it can be wrong in a way Option A cannot; needs a contrast test to have any teeth.

### Option C — Hybrid: Apple hues, own neutrals and semantics

Take the accent/status hues from the table above (they are the recognizable part of the iOS look, and they change least), and define neutrals/semantics ourselves as in B.

- Pro: gets the recognizable Apple color identity where it matters and owned, testable values where Apple publishes nothing anyway.
- Con: two provenances in one palette — must be labeled in the token file or it becomes untraceable.

### Option D — Defer to the host OS

Use `AccentColor`/system colors via CSS system color keywords and `forced-colors` support, minimizing our own palette.

- Pro: always current, always accessible, least maintenance.
- Con: abandons the iOS aesthetic brief almost entirely; CSS system color keywords are coarse and give no control over hierarchy.

## Invariants

1. Body text ≥ **4.5:1**; ≥ 18 pt or bold text ≥ **3:1**. `[HIG]` `[W3C]`
2. Control boundaries and state indicators ≥ **3:1** against adjacent color. `[W3C]` SC 1.4.11
3. Custom foreground/background pairs in dark mode: aim **7:1**, especially small text. `[HIG]`
4. Every color-carried meaning has a non-color redundancy. `[HIG]`
5. All four appearance contexts are defined for every token. An undefined `prefers-contrast: more` value is a defect, not a default.

## Open / unmeasured

- Apple's semantic color values — not published, must be chosen. `[UNSOURCED]`
- Whether list colors (from CloudKit `color_hex`, `core/src/reminders.rs`) can pass 3:1 against both light and dark backgrounds. These are **user-authored values we do not control**, so a compliance strategy is needed (e.g. render them as a bordered chip rather than as text or background). Currently unaddressed.

## Decision

*(empty)*

- Frame / option:
- Token provenance:
- Contrast validation method:
- Date / who:
