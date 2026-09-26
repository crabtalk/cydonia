// wasm-bindgen's loader falls back off `instantiateStreaming` only when the
// Content-Type is wrong. WebKit — every iOS browser — can fail streaming
// compilation of a compressed response (Cloudflare Brotli-compresses the wasm)
// and that failure is re-thrown, not fallen back from. On iOS, streaming is
// skipped.
//
// Tied to the text of wasm-bindgen's template: a template that changed fails
// here rather than shipping unpatched.
import { readFileSync, writeFileSync } from 'node:fs';

const path = 'www/static/demo/cydonia_demo.js';
const target = "if (typeof WebAssembly.instantiateStreaming === 'function') {";
const isIOS =
	"(/iP(hone|od|ad)/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1))";
const replacement = `if (!${isIOS} && typeof WebAssembly.instantiateStreaming === 'function') {`;

const source = readFileSync(path, 'utf8');
if (!source.includes(target)) {
	throw new Error(
		`patch-ios-wasm-loader: expected pattern not found in ${path} — wasm-bindgen's template changed, update the patch`
	);
}
writeFileSync(path, source.replace(target, replacement));
