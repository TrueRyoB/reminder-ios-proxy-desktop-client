# 05 — Motion and presentation

Governs: which presentation form an action gets (inline / popover / modal / sheet / navigation), animation budget, cancellability, reduced-motion behavior.

This axis is included because it is where this app has already been observed to fail: the reminder-edit action — the single most frequent action in a reminder app — is presented through an animated Framework7 Sheet Modal, a pattern originally chosen for the login flow (`gui/src/main.ts`, `app.sheet.create`). Apple's guidance addresses this case directly.

## What the sources publish

### Apple's frequency rule — the load-bearing quote `[HIG]`

> "In apps, generally avoid adding motion to UI interactions that occur frequently. The system already provides subtle animations for interactions with standard interface elements. For a custom element, you generally want to avoid making people spend extra time paying attention to unnecessary motion every time they interact with it."

And on blocking:

> "Let people cancel motion. As much as possible, don't make people wait for an animation to complete before they can do anything, especially if they have to experience the animation more than once."

Together these say: **animation cost scales with frequency, and repeated animation must never gate input.** That is an external, citable basis for ordering presentation forms by cost rather than by taste.

Supporting guidance `[HIG]`:

- "Add motion purposefully, supporting the experience without overshadowing it. Don't add motion for the sake of adding motion. Gratuitous or excessive animation can distract people and may make them feel disconnected or physically uncomfortable."
- "Make motion optional. Not everyone can or wants to experience the motion in your app… it's essential to avoid using it as the only way to communicate important information."
- "Aim for brevity and precision in feedback animations. When animated feedback is brief and precise, it tends to feel lightweight and unobtrusive."
- "Strive for realistic feedback motion that follows people's gestures and expectations… if someone reveals a view by sliding it down from the top, they don't expect to dismiss the view by sliding it to the side."
- Platform considerations: "No additional considerations for iOS, iPadOS, macOS, or tvOS" — i.e. the rules above apply unchanged on desktop.

Apple publishes **no durations and no easing curves** anywhere in this guidance. `[UNSOURCED]`

On system-provided transitions: in the current design language, "sheets, alerts, and popovers automatically adopt Liquid Glass… with the system handling material, concentric corner radius, and morphing transitions" `[HIG]`. Native apps get correct transitions for free. A web-tech app does not, which is why this axis needs an explicit decision here rather than inheriting one.

### Reduced motion `[W3C]`

`prefers-reduced-motion: no-preference | reduce` — "Detecting the desire for less motion on the page". This is the mechanism by which Apple's "make motion optional" requirement is actually satisfied in CSS; without honoring it, the requirement is unmet regardless of how tasteful the animation is.

## Presentation cost ladder

Apple states the *principle* (cost scales with frequency) but publishes no ranking of presentation forms. The ordering below is `[DERIVED]` — ranked by the two costs Apple names, time-to-interactive and attention demanded:

| Rank | Form | Time cost | Reversibility | Suited to |
| --- | --- | --- | --- | --- |
| 1 | **Inline edit in place** | none | immediate | the most frequent action in the app |
| 2 | **Popover / anchored panel** | minimal | click-away | frequent, contextual, few fields |
| 3 | **Instant modal** (no entrance animation) | one frame | Esc | occasional, many fields |
| 4 | **Animated sheet** | animation duration, twice (in and out) | Esc after animation | rare, ceremonial, consequential |
| 5 | **Full navigation to another screen** | transition + loses context | back | mode changes, not edits |

The rule that follows from Apple's frequency quote: **sort actions by frequency descending; presentation cost must not increase down that list.** A rank-4 form on the app's most frequent action is a violation with a citable basis, not a matter of preference.

This is the same check `/artist` runs as its 動線表; this file is where its cost ordering is grounded.

## Options

### Option A — System-mimicking motion

Reproduce iOS-like transitions (sheet slide-up, push/pop) with hand-written CSS transitions.

- Pro: strongest "feels like an iOS app" impression; matches the aesthetic brief most literally.
- Con: Apple publishes no durations or curves, so all values are `[UNSOURCED]` and will be approximations of a moving target. Applied to frequent actions it directly contradicts the frequency rule. Highest maintenance.

### Option B — Motion only for state changes that need explanation

Animate when motion carries information — an item leaving a list, a sheet that genuinely represents a mode change — and use no entrance animation for routine edits.

- Pro: satisfies "add motion purposefully" and the frequency rule simultaneously; the smallest amount of motion that still explains what happened.
- Con: requires classifying every action by frequency first, so it cannot be implemented before the 動線表 exists. Will look plainer than iOS in side-by-side comparison.

### Option C — Near-zero motion

No transitions except instant state changes and a loading indicator.

- Pro: fastest possible perceived interaction, which is `design/idea/README.md`'s stated top value ("App speed is fast enough so that it does not interfere with the top-level human action"); nothing to maintain; trivially satisfies reduced-motion.
- Con: loses the affordance Apple's "realistic feedback motion" provides — without any transition, a view replacement can read as a glitch rather than a navigation. Also gives up part of the aesthetic brief.

### Option D — Reduced-motion as the baseline, motion as the enhancement

Author the interface as Option C, then add Option B's informational motion **only** inside `@media (prefers-reduced-motion: no-preference)`.

- Pro: the accessible path is the default rather than a fallback, so it can never be the untested branch; motion becomes additive and removable; satisfies "make motion optional" structurally rather than by discipline.
- Con: two visual behaviors to verify instead of one; the motion layer risks being under-tested precisely because the app works without it.

## Invariants

1. Motion is never the only carrier of information. `[HIG]`
2. No animation blocks input, and no animation must complete before the next action is possible — especially for repeated actions. `[HIG]`
3. `prefers-reduced-motion: reduce` is honored for every transition. `[W3C]`
4. Presentation cost does not increase as action frequency increases. `[DERIVED from HIG]`
5. Dismissal mirrors presentation: a view revealed by one gesture is dismissed by its inverse. `[HIG]`

## Open / unmeasured

- Durations and easing curves — published by no source consulted. `[UNSOURCED]`
- Framework7's default sheet/modal animation timings are currently in force and uninventoried.
- Actual action frequencies for this app are unmeasured (no telemetry). The one datum on record is a user statement that editing a reminder is wanted immediately — which is enough to place editing at the top of the frequency list, but not enough to order the rest.

## Decision

*(empty)*

- Option:
- Presentation form per action (see 動線表):
- Reduced-motion strategy:
- Date / who:
