export const repo = 'https://github.com/crabtalk/cydonia';

/** The invite is permanent — a link with an expiry would rot on the page. */
export const discord = 'https://discord.gg/yGZDYnwbx6';

/** Canonical URL. The one place the host is named — sitemap, robots, share cards. */
export const site = 'https://cydonia.sh';

/** The app in one line — search results and share cards. */
export const tagline =
	'A desktop workspace for the coding agents you run. Open any directory as a project, put an ACP agent to work in it, and keep what comes out as durable artifacts on your own disk — articles, boards and tables, not a chat log.';

/** Where the demo clips and stills live. A changelog entry names a path under
    it rather than repeating the host in every release. */
export const cdn = 'https://cdn.crabtalk.ai';

/** The other way in, for anyone who would rather build it. */
export const install = 'cargo install cydonia';

/** A release names both its tag and its asset after the version, so every
    version in the changelog can say where its own dmg is. */
export const dmgFor = (version) =>
	`${repo}/releases/download/v${version}/cydonia-${version}-arm64.dmg`;

/** The tag page for a version — the assets, the notes GitHub keeps, the diff. */
export const releaseFor = (version) => `${repo}/releases/tag/v${version}`;

/** The one the buttons point at. Injected by Vite from Cargo.toml — see vite.config.js. */
export const dmg = dmgFor(__CYDONIA_VERSION__);
