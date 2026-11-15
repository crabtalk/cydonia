(() => {
  $FIND
  const el = $TARGET;
  if (!el) throw new Error('element not found; read the tab again or use a CSS selector');
  const bounds = (node) => {
    const rect = node.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const describe = (node) => node ? {
    tag: node.tagName.toLowerCase(), id: node.id,
    classes: [...node.classList].slice(0, 12),
    role: node.getAttribute('role')
  } : null;
  const properties = [
    'display', 'position', 'visibility', 'opacity', 'pointer-events',
    'width', 'height', 'min-width', 'max-width', 'min-height', 'max-height',
    'margin', 'padding', 'box-sizing', 'overflow-x', 'overflow-y',
    'z-index', 'transform', 'clip-path', 'content-visibility',
    'color', 'background-color', 'font-family', 'font-size', 'font-weight',
    'line-height', 'white-space', 'text-overflow', 'flex', 'flex-direction',
    'align-items', 'justify-content', 'gap', 'grid-template-columns'
  ];
  const style = getComputedStyle(el);
  const rect = bounds(el);
  const point = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  const centerInViewport = point.x >= 0 && point.y >= 0 &&
    point.x < innerWidth && point.y < innerHeight;
  const stack = centerInViewport ? document.elementsFromPoint(point.x, point.y) : [];
  const ancestors = [];
  const scrollContainers = [];
  let ancestorCount = 0;
  for (let parent = el.parentElement; parent; parent = parent.parentElement) {
    const css = getComputedStyle(parent);
    if (ancestorCount++ < 8) ancestors.push({
      ...describe(parent), bounds: bounds(parent),
      display: css.display, visibility: css.visibility, opacity: css.opacity,
      overflowX: css.overflowX, overflowY: css.overflowY,
      pointerEvents: css.pointerEvents, transform: css.transform, clipPath: css.clipPath
    });
    if (scrollContainers.length < 8 &&
        ((parent.scrollHeight > parent.clientHeight && /auto|scroll|hidden/.test(css.overflowY)) ||
         (parent.scrollWidth > parent.clientWidth && /auto|scroll|hidden/.test(css.overflowX)))) {
      scrollContainers.push({
        ...describe(parent), bounds: bounds(parent),
        scrollTop: parent.scrollTop, scrollLeft: parent.scrollLeft,
        scrollWidth: parent.scrollWidth, scrollHeight: parent.scrollHeight,
        clientWidth: parent.clientWidth, clientHeight: parent.clientHeight,
        overflowX: css.overflowX, overflowY: css.overflowY
      });
    }
  }
  return {
    element: describe(el),
    text: (el.innerText || el.textContent || '').trim().slice(0, 500),
    attributes: Object.fromEntries([...el.attributes].slice(0, 30).map((a) => [a.name, a.value.slice(0, 500)])),
    bounds: rect,
    viewport: { width: innerWidth, height: innerHeight, scrollX, scrollY },
    visibility: {
      hasBox: rect.width > 0 && rect.height > 0,
      intersectsViewport: rect.x < innerWidth && rect.y < innerHeight &&
        rect.x + rect.width > 0 && rect.y + rect.height > 0,
      checkVisibility: typeof el.checkVisibility === 'function' ?
        el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }) : null
    },
    disabled: el.matches(':disabled'),
    computedStyles: Object.fromEntries(properties.map((name) => [name, style.getPropertyValue(name)])),
    hitTest: {
      point, centerInViewport, target: describe(stack[0]),
      withinElement: stack.length > 0 && (stack[0] === el || el.contains(stack[0])),
      stack: stack.slice(0, 5).map(describe)
    },
    ancestors, ancestorsTruncated: ancestorCount > 8, scrollContainers
  };
})()
