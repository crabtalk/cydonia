<script>
	import images from './generated/og.json';
	import { site } from './meta.js';
	let { card = 'home' } = $props();
	const image = $derived(images[card]);
	const url = $derived(`${site}${image.path}`);
	const alt = $derived(image.alt);
</script>

<svelte:head>
	<meta property="og:image" content={url} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:type" content="image/png" />
	<meta property="og:image:alt" content={alt} />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:image" content={url} />
	<meta name="twitter:image:alt" content={alt} />
</svelte:head>
