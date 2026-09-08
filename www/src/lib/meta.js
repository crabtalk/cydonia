export const repo = 'https://github.com/crabtalk/cydonia';

/** The invite is permanent — a link with an expiry would rot on the page. */
export const discord = 'https://discord.gg/yGZDYnwbx6';

/** Canonical URL. The one place the host is named — sitemap, robots, share cards. */
export const site = 'https://cydonia.app';

/** The app in one line — search results and share cards. */
export const tagline =
	'A desktop workspace for the coding agents you run. Open any directory as a project, put an ACP agent to work in it, and keep what comes out as durable artifacts on your own disk — articles, boards and tables, not a chat log.';

/** The other way in, for anyone who would rather build it. */
export const install = 'cargo install cydonia';

/** Injected by Vite from Cargo.toml — see vite.config.js. */
export const version = __CYDONIA_VERSION__;

/** The release names both the tag and the asset after the version, so neither
    the URL nor the version has to be repeated anywhere. */
export const dmg = `${repo}/releases/download/v${version}/cydonia-${version}-arm64.dmg`;
