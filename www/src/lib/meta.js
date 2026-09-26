import { latest } from './changelog.js';

export const repo = 'https://github.com/crabtalk/cydonia';

/** The people behind cydonia. Their site, not their GitHub org: the org is
    what the source link already points into. */
export const crabtalk = 'https://crabtalk.ai';

/** The invite is permanent — a link with an expiry would rot on the page. */
export const discord = 'https://discord.gg/yGZDYnwbx6';

/** Canonical URL. The one place the host is named — sitemap, robots, share cards. */
export const site = 'https://cydonia.sh';

/** The app in one line — search results and share cards. */
export const tagline =
	'A desktop workspace for the coding agents you run. Open any directory as a project, put an ACP agent to work in it, and keep what comes out as durable artifacts on your own disk — articles, boards and tables, not a chat log.';

/** Where the demo clips and stills live. A changelog entry names a path under
    it rather than repeating the host in every release. */
export { cdn } from './media.js';

/** One line per shell, each served from `www/static`. */
export const install = `curl -fsSL ${site}/install.sh | sh`;
export const installWindows = `irm ${site}/install.ps1 | iex`;

/** Built from source, for anyone who would rather. */
export const cargo = 'cargo install cydonia';

/** The Linux and Windows files carry no version, so the latest release always
    has them under the same name. */
export const latestAsset = (name) => `${repo}/releases/latest/download/${name}`;
export const builds = [
	{ label: 'Linux x86_64', file: 'cydonia-linux-x86_64.tar.gz' },
	{ label: 'Linux aarch64', file: 'cydonia-linux-aarch64.tar.gz' },
	{ label: 'Windows x86_64', file: 'cydonia-windows-x86_64-setup.exe' }
];

/** Rebuilt from `main` on every merge, ahead of the latest release. */
export const nightly = `${repo}/releases/tag/nightly`;

/** A release names both its tag and its asset after the version, so every
    version in the changelog can say where its own dmg is. */
export const dmgFor = (version) =>
	`${repo}/releases/download/v${version}/cydonia-${version}-arm64.dmg`;

/** The tag page for a version — the assets, the notes GitHub keeps, the diff. */
export const releaseFor = (version) => `${repo}/releases/tag/v${version}`;

/** The one the buttons point at: the newest release that is out. */
export const dmg = dmgFor(latest.version);
