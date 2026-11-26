(input => {
  const address = location.href.split('#')[0];
  const url = input.base && address.startsWith(input.base) ? address.slice(input.base.length) : address;
  const quote = element => (element.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 240);
  const unique = selector => {
    const matches = document.querySelectorAll(selector);
    return matches.length === 1 ? matches[0] : null;
  };
  const view = visualViewport || { offsetLeft: 0, offsetTop: 0, width: innerWidth, height: innerHeight };
  if (input.point) {
    const element = document.elementFromPoint(
      view.offsetLeft + input.point[0] * view.width,
      view.offsetTop + input.point[1] * view.height,
    );
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
    const left = Math.max(view.offsetLeft, box.left);
    const right = Math.min(view.offsetLeft + view.width, box.right);
    const top = Math.max(view.offsetTop, box.top);
    const bottom = Math.min(view.offsetTop + view.height, box.bottom);
    if (left >= right || top >= bottom || getComputedStyle(element).visibility !== 'visible') {
      return { attached: true };
    }
    return {
      attached: true,
      point: [(right - view.offsetLeft) / view.width, ((top + bottom) / 2 - view.offsetTop) / view.height],
    };
  });
})
