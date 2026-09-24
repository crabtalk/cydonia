<script>
	import { latest } from './changelog.js';
	import { site } from './meta.js';
	const url = `${site}/og.png?v=${encodeURIComponent(latest.version)}`;
	const alt = `Cydonia — where agents keep their work. Screenshot from v${latest.version}.`;
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
