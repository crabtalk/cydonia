export const repo = 'https://github.com/crabtalk/cydonia';

/** Canonical URL. The one place the host is named — sitemap, robots, share cards. */
export const site = 'https://cydonia.app';

/** The app in one line — search results and share cards. */
export const tagline =
	'A desktop workspace for your notes and your coding agents. Open any directory as a project, write in it, and put an agent to work in the same place — on your own disk.';

/**
 * There are no packaged builds yet, so this is how you get it. `--git` rather
 * than the README's `--path .`, which needs a clone first.
 */
export const install = `cargo install --git ${repo}`;
