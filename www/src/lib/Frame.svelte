<script>
	// Empty until there is a real screenshot or recording to put in it.
	let { ratio = '16 / 10' } = $props();
</script>

<div class="frame" style="aspect-ratio: {ratio}" aria-hidden="true"></div>

<style>
	.frame {
		width: 100%;
		border: 1px solid var(--line);
		border-radius: 14px;
		background: var(--panel);
	}
</style>
