// Stamps the demo's JS and wasm URLs in the built index.html with a hash of both
// files, so a browser cannot pair the glue of one build with the wasm of another.
// Runs on build/, after `vite build`; static/demo/index.html keeps bare URLs.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';

const dir = 'build/demo';
const hash = createHash('sha256')
	.update(readFileSync(`${dir}/cydonia_demo.js`))
	.update(readFileSync(`${dir}/cydonia_demo_bg.wasm`))
	.digest('hex')
	.slice(0, 16);

const path = `${dir}/index.html`;
const source = readFileSync(path, 'utf8');
const stamped = source
	.replace("from './cydonia_demo.js'", `from './cydonia_demo.js?v=${hash}'`)
	.replace('init()', `init({ module_or_path: './cydonia_demo_bg.wasm?v=${hash}' })`);
if (!stamped.includes(`cydonia_demo.js?v=${hash}`) || !stamped.includes(`cydonia_demo_bg.wasm?v=${hash}`)) {
	throw new Error(`stamp-demo: expected import or init() not found in ${path}`);
}
writeFileSync(path, stamped);
