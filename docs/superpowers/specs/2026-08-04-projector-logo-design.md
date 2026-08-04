# Projector Logo Integration

## Goal

Replace the clapperboard emoji in the site marquee with the supplied gold projector artwork. The result should feel native to the existing dark cinema-and-gold visual language without substantially increasing the header height.

## Asset Treatment

- Keep `new_logo.png` as the untouched source artwork.
- Create `frontend/public/projector-logo.png` as a transparent, tightly cropped and resized derivative for the frontend.
- Crop tightly around the projector, retaining the projector body and a short portion of the light at the lens while removing most of the long beam and excess transparent canvas.
- Size the derivative for crisp high-density display without shipping the 1.9 MB source image to visitors.

## Header Integration

- Replace the inline `🎬` in `Marquee` with the optimized image.
- Place the image directly to the left of the existing translated brand heading in a centered flex row.
- Keep the heading text, tagline, marquee bulbs, navigation, colors, and overall structure unchanged.
- Display the projector at 92 CSS pixels wide on desktop and 72 pixels wide at the existing 560px mobile breakpoint.
- Add a restrained gold drop-shadow matching the existing marquee glow.
- Keep spacing tight enough that the logo and heading read as one brand lockup.
- At the 560px mobile breakpoint, reduce the heading font size to `1.45rem` and letter spacing to `.1em` so both translated brand names fit without horizontal overflow.

## Accessibility

The projector is decorative because the adjacent heading communicates the brand. Use an empty alt attribute so assistive technology does not announce redundant content. Preserve the semantic `h1`.

## Verification

- Add a component-level assertion that the marquee uses the projector asset and no longer renders the clapperboard emoji.
- Run the frontend test suite.
- Run the production frontend build.
- Check the header treatment at desktop and mobile widths for alignment, clipping, and overflow.

## Scope

This change is limited to the marquee brand mark. It does not change the favicon, page structure, copy, navigation behavior, or backend.
