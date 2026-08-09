# Header Lamp Chase Animation

## Summary

Animate the two bulb strips on the header (`Marquee.tsx` → `.bulbs`) with a slow,
calm traveling-chase effect: a single glowing lamp sweeps bulb-to-bulb along each
strip. Top strip travels one way, bottom strip the opposite way. Pure CSS — no
JSX, JS, or test changes.

## Motivation

The header already has a cinema-marquee aesthetic (gold bulbs, `Limelight` font,
glow). Adding gentle movement makes it feel alive. User wants it **slow** and
**not noisy**.

## Design

### Scope

Only `frontend/src/index.css` is modified.

### Mechanism

`.bulbs` keeps its existing markup and base background (dim bulbs via a repeating
radial gradient, `22px` spacing). A second `background-image` layer is stacked on
the same element:

- **Base layer** (unchanged): dim gold bulbs, tile `22px 10px`, static.
- **Overlay layer**: one bright bulb per wider repeating tile — `110px` wide
  (`5 × 22px`) — so only ~1 in 5 bulbs is ever lit.
- Animate `background-position-x` from `0` to `-110px` (exactly one tile, a
  seamless loop) with `animation-timing-function: steps(5, end)`. The bright spot
  hops bulb-to-bulb along the strip.

Keyframes:

```css
@keyframes bulb-chase {
  0%   { background-position-x: 0; }
  100% { background-position-x: -110px; }
}
```

### Direction

- Top strip: `animation-direction: normal`.
- Bottom strip: `animation-direction: reverse`.

The two strips therefore travel in opposite directions. (Exact left/right sense is
visual — swap the two values if it reads backwards.)

### Pace

Cycle duration ~`4s` per tile sweep, exposed as a `--bulb-chase` custom property
for easy tuning. One glow crosses 5 bulbs in 4s ≈ a new bulb every 0.8s.

### Noise control & accessibility

- Lit/unlit brightness contrast is modest (a soft glow highlight, not a harsh
  white flash).
- `@media (prefers-reduced-motion: reduce)` disables the animation, leaving the
  strips at today's static look.

## Testing / Verification

- `cd frontend && npm test` — CSS-only change, suite should stay green.
- Manual check in the Vite dev server (`npm run dev`):
  - chase moves slowly, opposite directions on the two strips;
  - loop is seamless (no jump);
  - with `prefers-reduced-motion: reduce` (devtools → rendering) the strips are
    static.

## Out of scope

- No changes to `Marquee.tsx`, logo, nav, or any other header element.
- No JS timer/interval or test additions.
