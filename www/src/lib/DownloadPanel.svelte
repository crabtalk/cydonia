<script>
	import { onMount } from 'svelte';
	import { Download, Copy, Check } from 'lucide-static';
	import { cargo, dmgFor, install, installWindows, latestAsset } from '$lib/meta.js';

	let { version } = $props();
	const platforms = ['macOS', 'Linux', 'Windows'];
	let platform = $state('macOS');
	let architecture = $state('x86_64');
	let copied = $state('');
	let message = $state('');
	let timer;
	const file = $derived(platform === 'macOS'
		? `cydonia-${version}-arm64.dmg`
		: platform === 'Linux'
			? `cydonia-linux-${architecture}.tar.gz`
			: 'cydonia-windows-x86_64.zip');
	const href = $derived(platform === 'macOS' ? dmgFor(version) : latestAsset(file));
	const command = $derived(platform === 'Windows' ? installWindows : install);

	onMount(() => {
		const agent = navigator.userAgent;
		if (/Windows/i.test(agent)) platform = 'Windows';
		else if (/Linux/i.test(agent) && !/Android/i.test(agent)) platform = 'Linux';
		if (/aarch64|arm64/i.test(agent)) architecture = 'aarch64';
		navigator.userAgentData?.getHighEntropyValues(['architecture'])
			.then(({ architecture: arch }) => {
				if (arch === 'arm') architecture = 'aarch64';
				else if (arch === 'x86') architecture = 'x86_64';
			})
			.catch(() => {});
		return () => clearTimeout(timer);
	});

	function select(value) {
		platform = value;
		copied = '';
		message = '';
	}

	function navigate(event, index) {
		let next;
		if (event.key === 'ArrowRight') next = (index + 1) % platforms.length;
		else if (event.key === 'ArrowLeft') next = (index + platforms.length - 1) % platforms.length;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = platforms.length - 1;
		else return;
		event.preventDefault();
		select(platforms[next]);
		document.getElementById(`download-tab-${next}`).focus();
	}

	async function copy(value) {
		clearTimeout(timer);
		try {
			await navigator.clipboard.writeText(value);
			copied = value;
			message = '';
			timer = setTimeout(() => { copied = ''; message = ''; }, 2000);
		} catch {
			copied = '';
			message = 'Could not copy. Select and copy the command below.';
		}
	}
</script>

{#snippet commandLine(value)}
	<div class="command">
		<code>{value}</code>
		<button class="control copy-command" type="button" onclick={() => copy(value)} aria-label={copied === value ? 'Copied' : 'Copy command'}>
			<span aria-hidden="true">{@html copied === value ? Check : Copy}</span>
			<span>{copied === value ? 'Copied' : 'Copy'}</span>
		</button>
	</div>
{/snippet}

<div class="download-panel">
	<div class="panel-header">
		<div class="tabs" role="tablist" aria-label="Operating system">
			{#each platforms as name, index}
				<button type="button" role="tab" id="download-tab-{index}" aria-selected={platform === name}
					aria-controls="download-platform" tabindex={platform === name ? 0 : -1}
					onclick={() => select(name)} onkeydown={(event) => navigate(event, index)}>{name}</button>
			{/each}
		</div>
		<div class="download-row">
			{#if platform === 'Linux'}
				<fieldset class="architecture" aria-label="Architecture">
					<div class="segments">
						{#each [{ value: 'x86_64', label: 'x64', name: 'x64 (Intel / AMD)' }, { value: 'aarch64', label: 'ARM64', name: 'ARM64' }] as option}
							<label>
								<input type="radio" name="architecture" value={option.value} bind:group={architecture} aria-label={option.name} />
								<span>{option.label}</span>
							</label>
						{/each}
					</div>
				</fieldset>
			{:else if platform === 'macOS'}
				<span class="detail">Apple Silicon</span>
			{/if}
			<a class="control download" {href} aria-label="Download for {platform}"><span aria-hidden="true">{@html Download}</span>Download</a>
		</div>
	</div>
	<div class="platform" id="download-platform" role="tabpanel" aria-labelledby="download-tab-{platforms.indexOf(platform)}" tabindex="0">
		{@render commandLine(command)}
		{#if platform !== 'macOS'}
			<p class="notice">{platform} support is new and may have bugs. See <a href="https://github.com/crabtalk/cydonia/issues/53">issue #53</a>.</p>
		{/if}
	</div>
	<details>
		<summary>Build from source</summary>
		<p class="label">Compile and install with the Rust toolchain.</p>
		{@render commandLine(cargo)}
	</details>
	<p class="status" role="status">{message}</p>
</div>

<style>
	.download-panel {
		container-type: inline-size;
		min-width: 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--panel);
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0 24px;
		padding: 0 24px;
		border-bottom: 1px solid var(--line);
	}

	.tabs {
		display: flex;
		gap: 24px;
	}

	button {
		font-family: inherit;
	}

	.tabs button {
		height: var(--control-height);
		margin: 8px 0;
		padding: 0;
		line-height: 1;
		border: 0;
		border-bottom: 2px solid transparent;
		background: transparent;
		color: var(--muted);
		font-size: var(--control-font-size);
		cursor: pointer;
	}

	.tabs button[aria-selected='true'] {
		color: var(--text);
		border-bottom-color: var(--text);
	}

	.platform {
		padding: 24px;
	}

	.download-row {
		margin-left: auto;
		padding: 8px 0;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
	}

	.detail {
		color: var(--muted);
		font-size: 13px;
	}

	.download {
		display: inline-flex;
		justify-content: center;
		align-items: center;
		gap: 8px;
		border-radius: var(--radius);
		background: var(--accent);
		color: var(--accent-ink);
		font-weight: 500;
		white-space: nowrap;
	}

	.download:hover {
		background: var(--accent-hover);
		text-decoration: none;
	}

	.download span, .copy-command span {
		display: inline-flex;
	}

	:global(.download-panel svg) {
		width: 16px;
		height: 16px;
	}

	.architecture {
		display: flex;
		min-width: 0;
		margin: 0;
		padding: 0;
		border: 0;
	}

	.segments {
		display: inline-flex;
		gap: 3px;
		padding: 2px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg);
	}

	.segments label {
		position: relative;
		cursor: pointer;
	}

	.segments input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
	}

	.segments span {
		display: grid;
		place-items: center;
		min-width: 60px;
		height: calc(var(--control-height) - 6px);
		padding: 0 var(--control-padding);
		line-height: 1;
		border-radius: var(--radius-sm);
		color: var(--muted);
		font-size: 13px;
	}

	.segments input:checked + span {
		background: var(--panel-high);
		color: var(--text);
	}

	.segments label:hover span {
		color: var(--text);
	}

	.segments input:focus-visible + span {
		outline: 2px solid var(--text);
		outline-offset: 1px;
	}

	.notice {
		margin: 12px 0 0;
		color: var(--muted);
		font-size: 13px;
	}

	.label {
		margin: 0 0 10px;
		color: var(--muted);
		font-size: 13px;
	}

	.command {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 6px 4px 12px;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg);
	}

	.command code {
		flex: 1;
		min-width: 0;
		padding: 4px 0;
		line-height: 1.4;
		overflow-x: auto;
		white-space: nowrap;
		background: none;
		font-size: 13px;
	}

	.copy-command {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		flex: none;
		min-width: 76px;
		border: 0;
		border-radius: var(--radius);
		background: transparent;
		color: var(--muted);
		cursor: pointer;
	}

	.copy-command:hover {
		background: var(--panel-high);
		color: var(--text);
	}

	details {
		padding: 16px 24px;
		border-top: 1px solid var(--line);
	}

	summary {
		cursor: pointer;
		color: var(--muted);
		font-size: 13px;
	}

	details[open] summary {
		margin-bottom: 16px;
	}

	.status {
		margin: 0;
		padding: 0 24px;
		color: var(--muted);
		font-size: 12px;
	}

	.status:not(:empty) {
		padding-bottom: 16px;
	}

	button:focus-visible, a:focus-visible, summary:focus-visible, .platform:focus-visible {
		outline: 2px solid var(--text);
		outline-offset: 3px;
	}

	@container (max-width: 580px) {
		.panel-header {
			padding: 0 20px;
		}

		.tabs {
			flex-basis: 100%;
			gap: 0;
		}

		.tabs button {
			flex: 1;
		}

		.platform {
			padding: 20px;
		}

		details {
			padding: 16px 20px;
		}

		.copy-command {
			min-width: var(--control-height);
			padding: 0;
		}

		.copy-command > span:last-child {
			display: none;
		}
	}
</style>
