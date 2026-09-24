// A made-up project after one agent session: the plan it wrote, the board
// it filled and the transcript. Shapes follow what cydonia writes to disk,
// trimmed to fit.
export const files = [
	{
		path: 'articles/1790089015001/content.md',
		text: `## Why now

The public API has no limit. One client replayed a queue
last Tuesday and took p99 from 80ms to 4s for everyone.

## Plan

- Token bucket per API key, kept in Redis
- 600 requests a minute by default, raised per plan
- Answer 429 with \`Retry-After\`, never drop silently

## Open

- Do webhooks count against the same bucket?`
	},
	{
		path: 'articles/1790089015001/properties.toml',
		text: `title = "Rate limiting plan"
archived = false`
	},
	{
		path: 'boards/1790266971001.toml',
		text: `name = "Roadmap"
key = "ROAD"
next_handle = 4

[[columns]]
name = "TODO"

[[columns.cards]]
handle = 1
text = "Token bucket middleware"

[[columns.cards]]
handle = 2
text = "429 with Retry-After"

[[columns.cards]]
handle = 3
text = "Decide: webhooks and the bucket"`
	},
	{
		path: 'sessions/1790252287183.json',
		text: `{
  "agent": "Claude Agent",
  "title": "Plan rate limiting",
  "closed": true,
  "items": [
    { "User": "Draft a rate limiting plan and put the work on ROAD" },
    { "Tool": "article_add Rate limiting plan" },
    { "Tool": "board_add_card ROAD ×3" },
    { "Agent": "The plan is in the article; three cards are on ROAD." }
  ]
}`
	}
];

/** Token rows for each file, painted by Shiki with both themes as
    `--shiki-light` and `--shiki-dark`. Build time only. */
export async function highlight() {
	const { codeToTokens } = await import('shiki');
	return Promise.all(
		files.map(async ({ path, text }) => {
			const { tokens } = await codeToTokens(text, {
				lang: path.split('.').pop(),
				themes: { light: 'github-light', dark: 'github-dark' },
				defaultColor: false
			});
			const lines = tokens.map((line) =>
				line.map(({ content, htmlStyle }) => ({
					content,
					style: Object.entries(htmlStyle ?? {})
						.map(([key, value]) => `${key}:${value}`)
						.join(';')
				}))
			);
			return { path, lines };
		})
	);
}
