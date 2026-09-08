<script>
	// The mark alone, without the black plate the icon and favicon sit on: in the
	// header it has to read on both themes, so it takes `currentColor` instead.
	// The paths are the ones in `public/logo.svg`, with its `translate`/`scale`
	// dropped in favour of a viewBox cropped to them.
	let { size = 18 } = $props();
</script>

<svg viewBox="0 0 404 393" width={size} height={size} fill="currentColor" aria-hidden="true">
	<path d="M79 98 404 0 316 185Z" />
	<path d="M162 166 365 261 0 393Z" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}
</style>
