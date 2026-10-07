Run `npm run og` from `www` to generate the share cards. The same generator
runs before Vite builds and when the dev server starts. Docs and release edits
restart the dev server to refresh the cards.

Cards use a flat background in the site's dark `--panel-high`, soft white Cydonia symbol and type. Home
and changelog cards are stable; published releases get version cards, and docs
get title cards. The home card shows the poster of the hero release (`hero`
in `src/lib/media.js`), downloaded once into `.cache/`. Entries marked `nightly` are excluded from
cards, release routes and the sitemap.

Generated PNGs live in `static/og/<page>.<content-hash>.png`. The generated
`src/lib/generated/og.json` maps page keys to paths and alt text. Both are
ignored by git. `static/og.png` remains a compatibility copy of the home card.
Image hashes do not invalidate social platforms' cached page previews: announce
releases with `/changelog/<version>/`, after deploying the page and card.

Inter is downloaded once into `.cache/Inter.ttf`, pinned to a google/fonts
commit and checked against SHA-256. A cold build needs network access for the
font; a failed download stops generation. Font license: SIL Open Font License,
https://github.com/google/fonts/tree/main/ofl/inter.

After building, run `node --test tests/og.test.mjs` to check prerendered
metadata, image hashes, release links and nightly filtering.
