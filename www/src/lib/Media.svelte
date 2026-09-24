<script>
	let { media } = $props();
</script>

{#if media.video}
	<video
		class="media"
		src={media.src}
		poster={media.poster}
		preload="metadata"
		controls
		loop
		muted
		playsinline
		aria-label={media.alt}
	></video>
{:else}
	<img
		class="media"
		src={media.src}
		draggable="false"
		loading="lazy"
		decoding="async"
		alt={media.alt}
	/>
{/if}

<style>
	.media {
		display: block;
		width: 100%;
		height: auto;
		max-width: 100%;
		margin: 20px 0 0;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		/* Safari ignores the attribute on its own. */
		-webkit-user-drag: none;
	}

</style>
