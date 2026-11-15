const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

const source = fs.readFileSync(path.join(__dirname, '../src/view/component/browse/mod.rs'), 'utf8');
const evaluate = source.match(/const EVALUATE: &str = r#"([\s\S]*?)"#;/)[1];
const find = source.match(/const FIND: &str = r#"([\s\S]*?)"#;/)[1].replaceAll('$MAX_ELEMENTS', '200');
const inspect = fs.readFileSync(path.join(__dirname, '../src/view/component/browse/inspect.js'), 'utf8');

function run(expression) {
  return vm.runInNewContext(evaluate.replace('$SCRIPT', expression), {
    location: { href: 'https://example.com' },
  });
}

test('evaluation awaits promises and preserves JSON values', async () => {
  const reply = await run('Promise.resolve({count: 3, ready: false})');
  assert.deepEqual(JSON.parse(JSON.stringify(reply)), {
    url: 'https://example.com', result: { count: 3, ready: false },
  });
  assert.equal((await run('undefined')).result, null);
});

test('evaluation reports rejected promises, unserializable values and oversized results', async () => {
  await assert.rejects(run('Promise.reject(new Error("request failed"))'), /request failed/);
  await assert.rejects(run('(() => { const x = {}; x.self = x; return x; })()'), /circular/i);
  await assert.rejects(run('() => 1'), /no JSON representation/);
  await assert.rejects(run('"x".repeat(16001)'), /return a smaller selection/);
});

function inspected({ hidden = false, covered = false, offscreen = false } = {}) {
  const style = {
    display: 'block', visibility: 'visible', opacity: hidden ? '0' : '1',
    overflowX: 'visible', overflowY: 'visible',
    getPropertyValue(name) { return name === 'opacity' ? this.opacity : ''; },
  };
  const element = {
    tagName: 'BUTTON', id: 'submit', classList: [], attributes: [], parentElement: null,
    disabled: true, innerText: 'Submit',
    getBoundingClientRect: () => ({ x: offscreen ? 2000 : 10, y: 10, width: 100, height: 30 }),
    getAttribute: () => null, checkVisibility: () => !hidden,
    matches: () => true, contains: () => false,
  };
  const overlay = { tagName: 'DIV', id: 'overlay', classList: [], getAttribute: () => null };
  const context = {
    document: {
      querySelectorAll: () => [], querySelector: () => element,
      elementsFromPoint: () => covered ? [overlay, element] : [element],
    },
    getComputedStyle: () => style,
    innerWidth: 800, innerHeight: 600, scrollX: 0, scrollY: 0,
  };
  const script = inspect.replace('$FIND', find).replace('$TARGET', 'document.querySelector("#submit")');
  return vm.runInNewContext(script, context);
}

test('selector inspection reaches disabled elements and identifies center overlays', () => {
  const result = inspected({ covered: true, hidden: true });
  assert.equal(result.disabled, true);
  assert.equal(result.visibility.checkVisibility, false);
  assert.equal(result.computedStyles.opacity, '0');
  assert.equal(result.hitTest.target.id, 'overlay');
  assert.equal(result.hitTest.withinElement, false);
});

test('inspection distinguishes an offscreen center from an overlapping element', () => {
  const result = inspected({ offscreen: true });
  assert.equal(result.visibility.intersectsViewport, false);
  assert.equal(result.hitTest.centerInViewport, false);
  assert.equal(result.hitTest.target, null);
  assert.equal(result.hitTest.stack.length, 0);
});
