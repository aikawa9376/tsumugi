// Searching the page's text for the bar under the toolbar. The page keeps the
// match selected, so scrolling to it and drawing it are the page's own.
(() => {
  const skipped = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEXTAREA', 'TEMPLATE']);
  // Every place the query is in the text the page shows, in reading order.
  const matches = (query) => {
    const found = [];
    const needle = query.toLowerCase();
    const walker = document.createTreeWalker(document.body || document, NodeFilter.SHOW_TEXT, {
      acceptNode: (node) => {
        const parent = node.parentElement;
        if (!parent || skipped.has(parent.tagName)) return NodeFilter.FILTER_REJECT;
        if (parent.checkVisibility && !parent.checkVisibility()) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.data.toLowerCase();
      for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + needle.length)) {
        const range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + needle.length);
        found.push(range);
      }
    }
    return found;
  };
  window.tsumugiFind = {
    // Show the next match, or the previous when `backwards`; `fresh` starts
    // from the top of the page rather than after the match shown.
    run(query, backwards, fresh) {
      const selection = getSelection();
      if (!query) {
        selection.removeAllRanges();
        return { found: false, total: 0, ordinal: 0 };
      }
      const ranges = matches(query);
      if (fresh) selection.removeAllRanges();
      const found = ranges.length > 0 && window.find(query, false, backwards, true, false, false, false);
      let ordinal = 0;
      if (found && selection.rangeCount > 0) {
        const shown = selection.getRangeAt(0);
        ordinal = ranges.findIndex((r) => r.compareBoundaryPoints(Range.START_TO_START, shown) === 0) + 1;
      }
      return { found: !!found, total: ranges.length, ordinal };
    },
    stop() {
      getSelection().removeAllRanges();
    },
  };
})();
