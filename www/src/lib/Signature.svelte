<script>
	import Crab from '$lib/Crab.svelte';
</script>

<!-- Fixed behind the page at the top: only a pull past the top shows it. -->
<div class="signature" aria-hidden="true">
	<p>Built with <span class="heart">♥</span> by the crabtalk team</p>
	<span class="crab"><span class="step"><Crab size={12} /></span></span>
</div>

<style>
	.signature {
		position: fixed;
		inset: 0 0 auto;
		z-index: -1;
		display: flex;
		align-items: center;
		justify-content: center;
		height: var(--header);
		color: var(--faint);
		font-size: 13px;
	}

	/* A band of light across the words, then a rest. */
	.signature p {
		margin: 0;
		background: linear-gradient(
			100deg,
			var(--faint) 40%,
			var(--text) 50%,
			var(--faint) 60%
		);
		background-size: 300% 100%;
		background-clip: text;
		-webkit-background-clip: text;
		-webkit-text-fill-color: transparent;
		animation: shine 4.5s ease-in-out infinite;
	}

	@keyframes shine {
		0% {
			background-position: 100% 0;
		}
		33%,
		100% {
			background-position: 0 0;
		}
	}

	.heart {
		display: inline-block;
		color: #e5484d;
		-webkit-text-fill-color: #e5484d;
		animation: beat 1.2s ease-in-out infinite;
	}

	/* Lub-dub, then rest. */
	@keyframes beat {
		0%,
		40%,
		100% {
			transform: scale(1);
		}
		10% {
			transform: scale(1.3);
		}
		20% {
			transform: scale(1);
		}
		30% {
			transform: scale(1.15);
		}
	}

	/* Along the foot of the band and back, rocking as it goes, turning to
	   face the way it walks at each edge — and blushing the heart's red as it
	   turns. */
	.crab {
		position: absolute;
		bottom: 6px;
		left: 12px;
		animation:
			scuttle 32s ease-in-out infinite,
			blush 32s linear infinite;
	}

	.step {
		display: block;
		animation: step 0.18s linear infinite alternate;
	}

	@keyframes scuttle {
		0%,
		100% {
			left: 12px;
			transform: scaleX(1);
		}
		49.9% {
			left: calc(100% - 24px);
			transform: scaleX(1);
		}
		50% {
			left: calc(100% - 24px);
			transform: scaleX(-1);
		}
		99.9% {
			left: 12px;
			transform: scaleX(-1);
		}
	}

	@keyframes blush {
		0%,
		50%,
		100% {
			color: #e5484d;
		}
		6%,
		44%,
		56%,
		94% {
			color: var(--faint);
		}
	}

	@keyframes step {
		from {
			transform: translateY(0) rotate(-8deg);
		}
		to {
			transform: translateY(-1px) rotate(8deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.signature p,
		.heart,
		.crab,
		.step {
			animation: none;
		}
	}
</style>
