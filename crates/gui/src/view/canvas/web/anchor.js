(input => {
  const url = location.href.split('#')[0];
  const quote = element => (element.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 240);
  const unique = selector => {
    const matches = document.querySelectorAll(selector);
    return matches.length === 1 ? matches[0] : null;
  };
  if (input.point) {
    const element = document.elementFromPoint(input.point[0] * innerWidth, input.point[1] * innerHeight);
    if (!element) return null;
    const path = [];
    for (let part = element; part; part = part.parentElement) {
      if (part.id) {
        const id = '#' + CSS.escape(part.id);
        if (unique(id) === part) {
          path.unshift(id);
          break;
        }
      }
      const tag = part.localName;
      const siblings = [...(part.parentElement?.children || [])].filter(sibling => sibling.localName === tag);
      path.unshift(CSS.escape(tag) + (siblings.length > 1 ? ':nth-of-type(' + (siblings.indexOf(part) + 1) + ')' : ''));
    }
    const selector = path.join(' > ');
    return unique(selector) === element ? { url, selector, quote: quote(element) } : null;
  }
  return input.anchors.map(anchor => {
    if (anchor.url !== url) return { attached: false };
    let element;
    try { element = unique(anchor.selector); }
    catch { return { attached: false }; }
    if (!element) return { attached: false };
    const hasStableId = element.id && '#' + CSS.escape(element.id) === anchor.selector;
    if (!hasStableId && quote(element) !== anchor.quote) return { attached: false };
    const box = element.getBoundingClientRect();
    const left = Math.max(0, box.left);
    const right = Math.min(innerWidth, box.right);
    const top = Math.max(0, box.top);
    const bottom = Math.min(innerHeight, box.bottom);
    if (left >= right || top >= bottom || getComputedStyle(element).visibility !== 'visible') {
      return { attached: true };
    }
    return { attached: true, point: [right / innerWidth, (top + bottom) / 2 / innerHeight] };
  });
})
