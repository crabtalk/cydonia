<script>
	// crabtalk's own mark, the shape its favicon carries, without the tile
	// behind it — `currentColor` so it takes the colour of the line it sits on.
	let { size = 14 } = $props();
</script>

<svg
	role="img"
	viewBox="0 0 256 256"
	width={size}
	height={size}
	fill="currentColor"
	aria-hidden="true"
>
	<path d="M216 40H88L32 96V160L88 216H216L168 168H112L80 136V120L112 88H168Z" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}
</style>
