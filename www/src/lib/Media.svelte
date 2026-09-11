<script>
	// A clip or still for one release. Video never autoplays and never preloads:
	// the hero already plays one on this page, and a changelog with a dozen
	// releases would otherwise cost tens of megabytes to open. The poster is
	// what loads; the file itself waits for a click.
	// Height is the fixed axis and width follows from the ratio. Capping the
	// width instead would let every release sit at a different height, since
	// that height would come from whatever shape the file happened to be.
	let { media, height = 440 } = $props();

	const box = $derived(
		[
			`--media-h: ${height}px`,
			// A video with `preload="none"` has no size until someone plays it, so
			// its ratio has to be declared. An image carries its own, and trusting
			// that over the numbers in the file means a wrong `w`/`h` can no
			// longer letterbox the thing it was meant to describe.
			media.video && media.w && media.h ? `aspect-ratio: ${media.w} / ${media.h}` : null
		]
			.filter(Boolean)
			.join('; ')
	);
</script>

{#if media.video}
	<video
		class="media"
		src={media.src}
		poster={media.poster}
		width={media.w}
		height={media.h}
		style={box}
		preload="none"
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
		width={media.w}
		height={media.h}
		style={box}
		draggable="false"
		loading="lazy"
		decoding="async"
		alt={media.alt}
	/>
{/if}

<style>
	.media {
		display: block;
		width: auto;
		height: var(--media-h);
		max-width: 100%;
		margin: 20px 0 0;
		border: 1px solid var(--line);
		border-radius: 12px;
		/* Safari ignores the attribute on its own. */
		-webkit-user-drag: none;
	}

	/* Narrow enough that a fixed height could only letterbox: take the column
	   and be as tall as the ratio makes it. */
	@media (max-width: 640px) {
		.media {
			width: 100%;
			height: auto;
		}
	}
</style>
