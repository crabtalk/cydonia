/** The folders the landing page opens: the project's `.cydonia`, the config
    directory and the data directory. The project is made up, one agent
    session in: the plan it wrote, the board it filled and the transcript.
    Shapes follow what cydonia writes to disk, trimmed to fit. `extra` rows
    show in the tree and cannot be opened. */
export const roots = [
	{
		name: '.cydonia',
		extra: ['entries.db'],
		files: [
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
		]
	},
	{
		name: '~/.config/cydonia',
		extra: ['cache/icons/', 'state.toml'],
		files: [
			{
				path: 'settings.toml',
				text: `agents_enabled = true
auto_update = true

[features]
sessions = true
boards = true

[mcp]
serve = true
write = true

[appearance]
mode = "system"
mono_font = "Monaco"

[[agents]]
name = "Claude Agent"
id = "claude-acp"
command = "~/.local/share/cydonia/agents/claude-acp/node_modules/.bin/claude-agent-acp"
args = []

[[agents]]
name = "Codex"
id = "codex-acp"
command = "~/.local/share/cydonia/agents/codex-acp/node_modules/.bin/codex-acp"
args = []`
			},
			{
				path: 'mcp.toml',
				text: `[[servers]]
name = "github"
command = "github-mcp-server"
args = ["stdio"]

[[servers]]
name = "linear"
url = "https://mcp.linear.app/mcp"
enabled = false`
			},
			{
				path: 'cache/registry.json',
				note: '41 agents',
				text: `{
  "version": "1.0.0",
  "agents": [
    { "id": "agoragentic-acp", "name": "Agoragentic" },
    { "id": "amp-acp", "name": "Amp" },
    { "id": "antigravity-acp", "name": "Google Antigravity" },
    { "id": "auggie", "name": "Auggie CLI" },
    { "id": "autohand", "name": "Autohand Code" },
    { "id": "claude-acp", "name": "Claude Agent" },
    { "id": "cline", "name": "Cline" },
    { "id": "codebuddy-code", "name": "Codebuddy Code" },
    { "id": "codex-acp", "name": "Codex" },
    { "id": "cortex-code", "name": "Cortex Code" },
    { "id": "corust-agent", "name": "Corust Agent" },
    { "id": "crow-cli", "name": "crow-cli" },
    { "id": "cursor", "name": "Cursor" },
    { "id": "deepagents", "name": "DeepAgents" },
    { "id": "devin", "name": "Devin" },
    { "id": "dimcode", "name": "DimCode" },
    { "id": "dirac", "name": "Dirac" },
    { "id": "factory-droid", "name": "Factory Droid" },
    { "id": "fast-agent", "name": "fast-agent" },
    { "id": "gemini", "name": "Gemini CLI" },
    { "id": "github-copilot-cli", "name": "GitHub Copilot" },
    { "id": "glm-acp-agent", "name": "GLM Agent" },
    { "id": "goose", "name": "goose" },
    { "id": "grok-build", "name": "Grok Build" },
    { "id": "harn", "name": "Harn" },
    { "id": "junie", "name": "Junie" },
    { "id": "kilo", "name": "Kilo" },
    { "id": "kimchi", "name": "Kimchi" },
    { "id": "kimi", "name": "Kimi CLI" },
    { "id": "minimax-code", "name": "MiniMax Code" },
    { "id": "minion-code", "name": "Minion Code" },
    { "id": "mistral-vibe", "name": "Mistral Vibe" },
    { "id": "nova", "name": "Nova" },
    { "id": "opencode", "name": "OpenCode" },
    { "id": "pi-acp", "name": "pi ACP" },
    { "id": "poolside", "name": "Poolside" },
    { "id": "qoder", "name": "Qoder CLI" },
    { "id": "qwen-code", "name": "Qwen Code" },
    { "id": "sigit", "name": "siGit Code" },
    { "id": "stakpak", "name": "Stakpak" },
    { "id": "vtcode", "name": "VT Code" }
  ]
}`
			}
		]
	},
	{
		name: '~/.local/share/cydonia',
		extra: ['grammars/'],
		files: [
			{
				path: 'agents/claude-acp/package.json',
				text: `{
  "dependencies": {
    "@agentclientprotocol/claude-agent-acp": "^0.81.2"
  }
}`
			},
			{
				path: 'agents/codex-acp/package.json',
				text: `{
  "dependencies": {
    "@agentclientprotocol/codex-acp": "^1.13.1"
  }
}`
			}
		]
	}
];

/** Every root with each file as token rows, painted by Shiki with both themes
    as `--shiki-light` and `--shiki-dark`. Build time only. */
export async function highlight() {
	const { codeToTokens } = await import('shiki');
	const paint = async ({ path, text, note }) => {
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
		return { path, note, lines };
	};
	return Promise.all(
		roots.map(async (root) => ({ ...root, files: await Promise.all(root.files.map(paint)) }))
	);
}
