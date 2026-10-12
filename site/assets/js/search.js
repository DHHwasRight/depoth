/*
 * Search, client side. No build step, no dependency, one file.
 *
 * It queries the index written by `crates/depoth/src/search.rs`. The scoring
 * below is that module's formula, reimplemented:
 *
 *     score(section, query) = SUM over t in query of
 *                                 idf(t) * w(t, section) * (1 + ln(tf(t, section)))
 *
 *         idf(t) = ln(1 + N / df(t))      N = number of sections
 *         w(t, s) = 2^rank(field)         highest-ranked field t occurs in
 *         rank: body 0, description 1, heading 2, title 3
 *
 * The two implementations have to agree on tokenising or a query silently
 * misses, which is why there is no stemmer on either side: a stemmer is the
 * part that drifts. The last word of a query also matches as a prefix, at
 * PREFIX_FACTOR of its weight, which is what makes it feel like type-ahead.
 *
 * DEGRADING
 * ---------
 * The trigger must ship with the `hidden` attribute:
 *
 *     <button type="button" data-search hidden …>
 *
 * This script removes it. With JavaScript off the button is absent, which is
 * better than present and dead. The keyboard shortcuts work either way, so a
 * site that ships no trigger at all still has search.
 *
 * WIRING
 * ------
 *     <script defer src="/assets/js/search.js" data-index="/search-index.json"></script>
 *
 * `data-index` is optional: the default is `search-index.json` two directories
 * above this file, which is right when the script sits in `assets/js/` and
 * keeps working under a deployment sub-path.
 *
 * KEYBOARD CONTRACT
 * -----------------
 *     Cmd/Ctrl+K    open, from anywhere
 *     /             open, unless a field already has focus
 *     Escape        close, focus returns where it was
 *     Down / Up     move through results, wrapping at each end
 *     Home / End    first / last result
 *     Enter         open the selected result
 *     Cmd/Ctrl+Enter  open it in a new tab
 *     Tab / Shift+Tab cycle inside the dialog, never out of it
 */

(function () {
  'use strict';

  var FORMAT_VERSION = 1;
  var PREFIX_FACTOR = 0.5;
  var SNIPPET_CHARS = 220;
  var MAX_RESULTS = 20;
  var TOKEN_SPLIT = /[^\p{L}\p{N}]+/u;

  var script = (typeof document !== 'undefined' && document.currentScript) || null;
  var indexUrl =
    (script && script.dataset.index) ||
    (script && script.src ? new URL('../../search-index.json', script.src).href : '/search-index.json');

  // --------------------------------------------------------------- the index

  var index = null; // {docs, sections, terms, words: sorted term list}
  var loading = null;
  var loadError = null;

  function load() {
    if (index) return Promise.resolve(index);
    if (loading) return loading;
    loading = fetch(indexUrl, { credentials: 'same-origin' })
      .then(function (r) {
        if (!r.ok) throw new Error('HTTP ' + r.status);
        return r.json();
      })
      .then(function (data) {
        if (data.v !== FORMAT_VERSION) {
          throw new Error('index format v' + data.v + ', expected v' + FORMAT_VERSION);
        }
        // Object key order is not sorted order in JavaScript -- a key that
        // looks like an integer is hoisted to the front -- so the prefix scan
        // gets its own sorted array rather than trusting the JSON's order.
        data.words = Object.keys(data.terms).sort();
        index = data;
        return index;
      })
      .catch(function (e) {
        loadError = e;
        loading = null;
        throw e;
      });
    return loading;
  }

  // ------------------------------------------------------------- the scoring

  function tokenize(text) {
    var out = [];
    var parts = String(text).toLowerCase().split(TOKEN_SPLIT);
    for (var i = 0; i < parts.length; i++) {
      if (parts[i] && Array.from(parts[i]).length <= 48) out.push(parts[i]);
    }
    return out;
  }

  /* 2^rank, where the rank is the position of the highest set field bit. */
  function fieldWeight(fields) {
    if (!fields) return 0;
    return 1 << (31 - Math.clz32(fields));
  }

  /* First index in the sorted word list at or after `term`. */
  function lowerBound(words, term) {
    var lo = 0;
    var hi = words.length;
    while (lo < hi) {
      var mid = (lo + hi) >> 1;
      if (words[mid] < term) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  /* section id -> {fields, count, factor} for one query term. */
  function postingsFor(term, allowPrefix) {
    var found = new Map();
    function take(postings, factor) {
      for (var i = 0; i < postings.length; i++) {
        var p = postings[i];
        var slot = found.get(p[0]);
        if (!slot) {
          slot = { fields: 0, count: 0, factor: 0 };
          found.set(p[0], slot);
        }
        slot.fields |= p[1];
        slot.count += p[2];
        if (factor > slot.factor) slot.factor = factor;
      }
    }
    var exact = index.terms[term];
    if (exact) take(exact, 1);
    if (allowPrefix) {
      for (var i = lowerBound(index.words, term); i < index.words.length; i++) {
        var word = index.words[i];
        if (word.lastIndexOf(term, 0) !== 0) break;
        if (word !== term) take(index.terms[word], PREFIX_FACTOR);
      }
    }
    return found;
  }

  function search(query) {
    var terms = tokenize(query);
    if (!terms.length || !index.sections.length) return { terms: terms, hits: [] };
    var total = index.sections.length;
    var scores = null;

    for (var n = 0; n < terms.length; n++) {
      var found = postingsFor(terms[n], n === terms.length - 1);
      if (!found.size) return { terms: terms, hits: [] };
      var idf = Math.log(1 + total / found.size);

      if (scores === null) {
        scores = new Map();
        found.forEach(function (m, section) {
          scores.set(section, contribution(idf, m));
        });
      } else {
        // Every term must appear, so this narrows on each pass.
        var next = new Map();
        scores.forEach(function (score, section) {
          var m = found.get(section);
          if (m) next.set(section, score + contribution(idf, m));
        });
        scores = next;
      }
      if (!scores.size) return { terms: terms, hits: [] };
    }

    var ranked = Array.from(scores.entries());
    // Score first, section order second: equal scores stay in reading order
    // and the list does not reshuffle between builds.
    ranked.sort(function (a, b) {
      return b[1] - a[1] || a[0] - b[0];
    });
    ranked.length = Math.min(ranked.length, MAX_RESULTS);

    return {
      terms: terms,
      hits: ranked.map(function (entry) {
        var section = index.sections[entry[0]];
        var doc = index.docs[section[0]];
        var anchor = section[1];
        return {
          url: anchor ? doc[0] + '#' + anchor : doc[0],
          title: doc[1],
          heading: section[2] || doc[1],
          text: section[3] || doc[2],
          score: entry[1]
        };
      })
    };
  }

  function contribution(idf, m) {
    return idf * fieldWeight(m.fields) * (1 + Math.log(m.count)) * m.factor;
  }

  // ------------------------------------------------------------- the snippet

  /* A window of `text` around the first query term in it. */
  function snippet(text, terms) {
    var chars = Array.from(text);
    if (chars.length <= SNIPPET_CHARS) return text;
    var lower = text.toLowerCase();
    var at = -1;
    for (var i = 0; i < terms.length; i++) {
      var found = lower.indexOf(terms[i]);
      if (found !== -1 && (at === -1 || found < at)) at = found;
    }
    if (at === -1) at = 0;
    // indexOf is in UTF-16 units; the slice below is in code points.
    at = Array.from(text.slice(0, at)).length;

    var start = Math.max(0, at - Math.floor(SNIPPET_CHARS / 2));
    while (start > 0 && !/\s/.test(chars[start - 1])) start--;
    var end = Math.min(chars.length, start + SNIPPET_CHARS);
    return (
      (start > 0 ? '…' : '') + chars.slice(start, end).join('') + (end < chars.length ? '…' : '')
    );
  }

  /* Appends `text` to `node`, wrapping each query term in a <mark>.
   * Built from text nodes rather than innerHTML, so nothing in the index can
   * become markup. */
  function appendHighlighted(node, text, terms) {
    if (!terms.length) {
      node.appendChild(document.createTextNode(text));
      return;
    }
    var sorted = terms.slice().sort(function (a, b) {
      return b.length - a.length;
    });
    var pattern = new RegExp(
      '(' + sorted.map(escapeRegExp).join('|') + ')',
      'giu'
    );
    var at = 0;
    var match;
    pattern.lastIndex = 0;
    while ((match = pattern.exec(text)) !== null) {
      if (match.index > at) {
        node.appendChild(document.createTextNode(text.slice(at, match.index)));
      }
      var mark = document.createElement('mark');
      mark.textContent = match[0];
      node.appendChild(mark);
      at = match.index + match[0].length;
      if (match[0].length === 0) pattern.lastIndex++;
    }
    if (at < text.length) node.appendChild(document.createTextNode(text.slice(at)));
  }

  function escapeRegExp(s) {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }

  // ----------------------------------------------------------------- styling
  //
  // The dialog brings its own stylesheet so the module drops into a site that
  // knows nothing about it. Every colour is a theme token with a plain
  // fallback, so it follows the page in light and dark and does not need one.

  var CSS = [
    '.dsx-dialog{width:min(640px,calc(100vw - 2rem));max-height:min(560px,calc(100dvh - 6rem));',
    'padding:0;border:1px solid var(--color-line,#d8d8d8);border-radius:12px;',
    'background:var(--color-ground,#fff);color:var(--color-ink,#111);overflow:hidden;',
    'font:14px/1.5 ui-sans-serif,system-ui,sans-serif;box-shadow:0 24px 60px rgb(0 0 0 / .22)}',
    '.dsx-dialog::backdrop{background:rgb(0 0 0 / .4)}',
    '.dsx-dialog[open]{display:flex;flex-direction:column}',
    '.dsx-top{display:flex;align-items:center;gap:.5rem;padding:.75rem .875rem;',
    'border-bottom:1px solid var(--color-line,#d8d8d8)}',
    '.dsx-input{flex:1;min-width:0;border:0;background:transparent;color:inherit;',
    'font:inherit;font-size:16px;outline:none}',
    '.dsx-close{flex:none;border:1px solid var(--color-line,#d8d8d8);border-radius:6px;',
    'background:transparent;color:var(--color-ink-soft,#666);font:inherit;font-size:11px;',
    'padding:.15rem .4rem;cursor:pointer}',
    '.dsx-close:focus-visible,.dsx-input:focus-visible{outline:2px solid var(--color-accent,#36c);outline-offset:2px}',
    '.dsx-list{flex:1;margin:0;padding:.375rem;list-style:none;overflow-y:auto}',
    '.dsx-option{border-radius:8px}',
    '.dsx-option a{display:block;padding:.5rem .625rem;color:inherit;text-decoration:none}',
    '.dsx-option[aria-selected="true"]{background:var(--color-sunken,#eee)}',
    '.dsx-option[aria-selected="true"] .dsx-heading{color:var(--color-accent,#36c)}',
    '.dsx-crumb{display:block;font-size:11px;letter-spacing:.04em;text-transform:uppercase;',
    'color:var(--color-ink-soft,#666)}',
    '.dsx-heading{display:block;font-weight:600;margin-top:.1rem}',
    '.dsx-snippet{display:block;margin-top:.15rem;color:var(--color-ink-soft,#666);',
    'overflow:hidden;display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:2}',
    '.dsx-snippet mark,.dsx-heading mark{background:var(--color-accent-fill,#ffe9a8);color:inherit;',
    'border-radius:2px;padding:0 1px}',
    '.dsx-foot{display:flex;justify-content:space-between;gap:1rem;padding:.5rem .875rem;',
    'border-top:1px solid var(--color-line,#d8d8d8);color:var(--color-ink-soft,#666);font-size:12px}',
    '.dsx-foot kbd{font:inherit;border:1px solid var(--color-line,#d8d8d8);border-radius:4px;padding:0 .25rem}',
    '.dsx-empty{padding:1.5rem .875rem;color:var(--color-ink-soft,#666);text-align:center}',
    '@media (prefers-reduced-motion:no-preference){.dsx-dialog[open]{animation:dsx-in .12s ease-out}',
    '@keyframes dsx-in{from{opacity:0;transform:translateY(-4px)}}}'
  ].join('');

  // ------------------------------------------------------------------- the UI

  var dialog, input, list, status, closeButton;
  var options = [];
  var selected = -1;
  var lastQuery = '';
  var returnFocusTo = null;

  function buildUi() {
    var style = document.createElement('style');
    style.textContent = CSS;
    document.head.appendChild(style);

    dialog = document.createElement('dialog');
    dialog.className = 'dsx-dialog';
    dialog.setAttribute('aria-label', 'Search the documentation');

    var top = document.createElement('div');
    top.className = 'dsx-top';

    input = document.createElement('input');
    input.className = 'dsx-input';
    input.type = 'text';
    input.placeholder = 'Search the documentation…';
    input.autocomplete = 'off';
    input.spellcheck = false;
    input.setAttribute('role', 'combobox');
    input.setAttribute('aria-autocomplete', 'list');
    input.setAttribute('aria-expanded', 'false');
    input.setAttribute('aria-controls', 'dsx-listbox');
    input.setAttribute('aria-label', 'Search the documentation');

    closeButton = document.createElement('button');
    closeButton.type = 'button';
    closeButton.className = 'dsx-close';
    closeButton.textContent = 'Esc';
    closeButton.setAttribute('aria-label', 'Close search');

    top.appendChild(input);
    top.appendChild(closeButton);

    list = document.createElement('ul');
    list.className = 'dsx-list';
    list.id = 'dsx-listbox';
    list.setAttribute('role', 'listbox');
    list.setAttribute('aria-label', 'Search results');

    var foot = document.createElement('div');
    foot.className = 'dsx-foot';
    status = document.createElement('span');
    // A live region, so the count reaches a screen reader as it changes and
    // not only the eye.
    status.setAttribute('role', 'status');
    status.setAttribute('aria-live', 'polite');
    var hint = document.createElement('span');
    hint.innerHTML =
      '<kbd>↑</kbd><kbd>↓</kbd> move · <kbd>↵</kbd> open · <kbd>esc</kbd> close';
    foot.appendChild(status);
    foot.appendChild(hint);

    dialog.appendChild(top);
    dialog.appendChild(list);
    dialog.appendChild(foot);
    document.body.appendChild(dialog);

    input.addEventListener('input', onInput);
    input.addEventListener('keydown', onKeyDown);
    closeButton.addEventListener('click', close);
    closeButton.addEventListener('keydown', onKeyDown);
    // `showModal` traps focus already; this keeps the cycle to the two
    // controls that are ours, in a predictable order.
    dialog.addEventListener('keydown', onDialogKeyDown);
    // Escape is the dialog's own `cancel`, so it is left alone and every exit
    // -- Escape, the button, the backdrop, a chosen result -- arrives here.
    // One place to restore focus means it cannot be forgotten on one path.
    dialog.addEventListener('close', function () {
      var target = returnFocusTo;
      returnFocusTo = null;
      if (target && target.isConnected && target.focus) {
        target.focus();
      } else if (document.activeElement && document.activeElement.blur) {
        // Otherwise focus stays on the now-hidden input, and the next
        // keystroke looks to the page like typing in a field.
        document.activeElement.blur();
      }
    });
    dialog.addEventListener('click', function (e) {
      if (e.target === dialog) close(); // the backdrop
    });
    list.addEventListener('mousemove', function (e) {
      var li = e.target.closest ? e.target.closest('.dsx-option') : null;
      if (li) selectAt(options.indexOf(li), false);
    });
    list.addEventListener('click', function (e) {
      var li = e.target.closest ? e.target.closest('.dsx-option') : null;
      if (li) close();
    });
  }

  function open(initial) {
    if (!dialog) buildUi();
    if (dialog.open) return;
    // Where focus goes on close. A shortcut is pressed with focus on nothing
    // in particular, and dumping the reader back on <body> loses their place,
    // so the trigger is the fallback.
    var active = document.activeElement;
    returnFocusTo =
      active && active !== document.body ? active : document.querySelector('[data-search]');
    dialog.showModal();
    if (typeof initial === 'string') input.value = initial;
    input.select();
    load().then(render, render);
    render();
  }

  function close() {
    if (dialog && dialog.open) dialog.close();
  }

  function onInput() {
    if (input.value === lastQuery) return;
    lastQuery = input.value;
    if (index) render();
    else load().then(render, render);
  }

  function render() {
    var query = input.value.trim();
    list.textContent = '';
    options = [];
    selected = -1;
    input.removeAttribute('aria-activedescendant');

    if (loadError) {
      input.setAttribute('aria-expanded', 'false');
      status.textContent = 'The search index could not be loaded.';
      list.appendChild(empty('Search is unavailable. Every page is also at its own URL.'));
      return;
    }
    if (!index) {
      input.setAttribute('aria-expanded', 'false');
      status.textContent = 'Loading the index…';
      return;
    }
    if (!query) {
      input.setAttribute('aria-expanded', 'false');
      status.textContent = index.docs.length + ' pages indexed';
      list.appendChild(empty('Type to search ' + index.docs.length + ' pages.'));
      return;
    }

    var result = search(query);
    input.setAttribute('aria-expanded', result.hits.length ? 'true' : 'false');
    status.textContent =
      result.hits.length === 0
        ? 'No results'
        : result.hits.length + (result.hits.length === 1 ? ' result' : ' results');

    if (!result.hits.length) {
      list.appendChild(empty('Nothing matches “' + query + '”.'));
      return;
    }
    for (var i = 0; i < result.hits.length; i++) {
      list.appendChild(optionFor(result.hits[i], i, result.terms));
    }
    selectAt(0, true);
  }

  function empty(message) {
    var li = document.createElement('li');
    li.className = 'dsx-empty';
    li.textContent = message;
    return li;
  }

  function optionFor(hit, i, terms) {
    var li = document.createElement('li');
    li.className = 'dsx-option';
    li.id = 'dsx-option-' + i;
    li.setAttribute('role', 'option');
    li.setAttribute('aria-selected', 'false');

    // A real link, so the status bar shows the target and Cmd-click opens a
    // tab. It is out of the tab order: the input keeps focus and points at
    // the selected option with aria-activedescendant, which is the combobox
    // pattern.
    var a = document.createElement('a');
    a.href = hit.url;
    a.tabIndex = -1;

    var crumb = document.createElement('span');
    crumb.className = 'dsx-crumb';
    crumb.textContent = hit.title;

    var heading = document.createElement('span');
    heading.className = 'dsx-heading';
    appendHighlighted(heading, hit.heading, terms);

    var text = document.createElement('span');
    text.className = 'dsx-snippet';
    appendHighlighted(text, snippet(hit.text || '', terms), terms);

    a.appendChild(crumb);
    a.appendChild(heading);
    a.appendChild(text);
    li.appendChild(a);
    options.push(li);
    return li;
  }

  function selectAt(i, scroll) {
    // The modulo below does the wrapping, so a negative index is wanted here
    // and must not be filtered out: Up from the first result is the last.
    if (!options.length) return;
    if (selected >= 0 && options[selected]) options[selected].setAttribute('aria-selected', 'false');
    selected = ((i % options.length) + options.length) % options.length;
    var li = options[selected];
    li.setAttribute('aria-selected', 'true');
    input.setAttribute('aria-activedescendant', li.id);
    if (scroll && li.scrollIntoView) li.scrollIntoView({ block: 'nearest' });
  }

  function activate(newTab) {
    if (selected < 0 || !options[selected]) return;
    var url = options[selected].querySelector('a').href;
    if (newTab) {
      window.open(url, '_blank', 'noopener');
      return;
    }
    close();
    // A same-page fragment does not reload, so focus has to be moved by hand
    // or a keyboard reader stays at the top of the document.
    window.location.assign(url);
    var id = url.indexOf('#') === -1 ? '' : url.slice(url.indexOf('#') + 1);
    if (id) {
      var target = document.getElementById(id);
      if (target) {
        if (!target.hasAttribute('tabindex')) target.setAttribute('tabindex', '-1');
        target.focus({ preventScroll: true });
      }
    }
  }

  function onKeyDown(e) {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        selectAt(selected + 1, true);
        break;
      case 'ArrowUp':
        e.preventDefault();
        selectAt(selected - 1, true);
        break;
      case 'Home':
        if (options.length) {
          e.preventDefault();
          selectAt(0, true);
        }
        break;
      case 'End':
        if (options.length) {
          e.preventDefault();
          selectAt(options.length - 1, true);
        }
        break;
      case 'Enter':
        if (e.isComposing) return;
        e.preventDefault();
        activate(e.metaKey || e.ctrlKey);
        break;
      default:
        break;
    }
  }

  /* The focus trap. `showModal` already confines focus to the dialog; this
   * makes the cycle the two controls we own, in order, so Tab never lands on
   * a result link and strands the reader. */
  function onDialogKeyDown(e) {
    if (e.key !== 'Tab') return;
    e.preventDefault();
    var order = [input, closeButton];
    var at = order.indexOf(document.activeElement);
    var next = (at + (e.shiftKey ? -1 : 1) + order.length) % order.length;
    order[next].focus();
  }

  // --------------------------------------------------------------- the entry

  function isTyping(el) {
    if (!el) return false;
    if (el.isContentEditable) return true;
    var tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
  }

  function onGlobalKeyDown(e) {
    if (dialog && dialog.open) return;
    if (e.defaultPrevented) return;
    if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault();
      open();
      return;
    }
    // `/` is the other half of the convention, but only when it would
    // otherwise be a literal slash in a field.
    if (e.key === '/' && !e.metaKey && !e.ctrlKey && !e.altKey && !isTyping(e.target)) {
      e.preventDefault();
      open();
    }
  }

  function reveal() {
    var triggers = document.querySelectorAll('[data-search]');
    for (var i = 0; i < triggers.length; i++) {
      var trigger = triggers[i];
      trigger.hidden = false;
      trigger.removeAttribute('aria-hidden');
      trigger.addEventListener('click', function () {
        open();
      });
      // The index is the only thing a first search waits for, so start it on
      // the first sign of intent rather than on the click.
      trigger.addEventListener('pointerenter', prefetch, { once: true });
      trigger.addEventListener('focus', prefetch, { once: true });
    }
    // Lets a stylesheet style the page differently once search exists, and is
    // how a test tells "revealed" from "was never hidden".
    document.documentElement.setAttribute('data-search-ready', '');
  }

  function prefetch() {
    load().catch(function () {});
  }

  if (typeof document !== 'undefined') {
    document.addEventListener('keydown', onGlobalKeyDown);
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', reveal);
    } else {
      reveal();
    }
  }

  // The scoring has to agree with `crates/depoth/src/search.rs` word for word,
  // so it is reachable from node and a test compares the two rankings over the
  // real index. The page never takes this branch.
  if (typeof module === 'object' && module.exports) {
    module.exports = {
      tokenize: tokenize,
      fieldWeight: fieldWeight,
      snippet: snippet,
      search: search,
      use: function (data) {
        data.words = Object.keys(data.terms).sort();
        index = data;
      }
    };
  }
})();
