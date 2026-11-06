// TcTidy settings page: edits the `.tctidy.toml` that `tctidy --settings`
// serves. The list sets values; the panel beside it shows what the selected
// setting does to a sample (the default against the current value), and under
// it formats a function block, or the user's own code, with every value on the
// page. Talks only to its own loopback server, with the one-time token from the
// URL's fragment.
"use strict";

(() => {
  const TOKEN_KEY = "tctidy-settings-token";
  const TOKEN_SHAPE = /^[0-9a-f]{32}$/;
  const PING_MS = 30000;
  const REOPEN = "Open it again from Tools › TcTidy › Formatting Settings… in Visual Studio, or run tctidy --settings.";

  // ---- token ------------------------------------------------------------------------

  // The fragment carries the token (never sent to a server). It is kept for
  // reloads of this tab, and the fragment leaves the address bar.
  const token = (() => {
    let t = location.hash.slice(1).split("&")[0];
    try {
      if (TOKEN_SHAPE.test(t)) sessionStorage.setItem(TOKEN_KEY, t);
      else t = sessionStorage.getItem(TOKEN_KEY) || "";
    } catch (e) {
      // Storage blocked: the fragment still serves this load.
    }
    if (location.hash) history.replaceState(null, "", location.pathname);
    return TOKEN_SHAPE.test(t) ? t : "";
  })();

  // ---- theme --------------------------------------------------------------------

  // The system's light or dark mode, followed live, unless the top bar's
  // switch picked one. The pick is kept by the browser for this page's
  // address, so it lasts while this settings server runs.
  const THEME_KEY = "tctidy-settings-theme";
  const isTheme = (t) => t === "light" || t === "dark";
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

  function pickedTheme() {
    try {
      const t = localStorage.getItem(THEME_KEY);
      if (isTheme(t)) return t;
    } catch (e) {
      // Storage blocked: follow the system.
    }
    return null;
  }

  function applyTheme(theme) {
    if (theme) document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
    const dark = theme ? theme === "dark" : systemDark.matches;
    document.getElementById("theme-light").setAttribute("aria-pressed", String(!dark));
    document.getElementById("theme-dark").setAttribute("aria-pressed", String(dark));
  }

  function pickTheme(theme) {
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch (e) {
      // Storage blocked: the pick holds until the page closes.
    }
    applyTheme(theme);
  }

  applyTheme(pickedTheme());
  systemDark.addEventListener("change", () => applyTheme(pickedTheme()));
  document.getElementById("theme-light").addEventListener("click", () => pickTheme("light"));
  document.getElementById("theme-dark").addEventListener("click", () => pickTheme("dark"));

  // ---- small helpers ------------------------------------------------------------

  const $ = (id) => document.getElementById(id);

  function el(tag, cls, text) {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text !== undefined && text !== null) node.textContent = text;
    return node;
  }

  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  const copy = (o) => JSON.parse(JSON.stringify(o));

  function debounce(fn, ms) {
    let timer = null;
    return (...args) => {
      clearTimeout(timer);
      timer = setTimeout(() => fn(...args), ms);
    };
  }

  class ApiError extends Error {
    constructor(status, message, data) {
      super(message);
      this.status = status;
      this.data = data;
    }
  }

  async function api(method, path, body) {
    let res;
    try {
      res = await fetch(path, {
        method,
        cache: "no-store",
        headers: Object.assign(
          { "X-TcTidy-Token": token },
          body === undefined ? {} : { "Content-Type": "application/json" },
        ),
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch (e) {
      throw new ApiError(0, "The settings server is not running.");
    }
    const text = await res.text();
    let data = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch (e) {
      data = null;
    }
    if (!res.ok) throw new ApiError(res.status, (data && data.error) || res.statusText, data);
    return data;
  }

  // ---- state --------------------------------------------------------------------

  const state = {
    path: "",
    exists: false,
    version: "",
    groups: [], // [{ id, label, settings, section, link, item, count }]
    settings: new Map(), // key -> catalog entry
    rows: new Map(), // key -> row parts
    values: {}, // key -> value shown on the page
    saved: {}, // key -> value in the file ({} when the file is invalid)
    previews: {}, // key -> { before, after } for the values shown
    example: null, // { declaration, implementation } "Your code" starts from
    errors: {}, // key -> message
    selected: null, // key shown in the preview panel
    online: true,
    saving: false,
    saves: 0, // completed saves, so a ping that crossed one is ignored
    conflict: false,
    flash: null, // { text, until }: a passing status message
  };

  // ---- value display ------------------------------------------------------------

  const CHOICE_LABELS = {
    upper: "UPPER",
    lower: "lower",
    preserve: "As written",
    same_line: "Same line",
    own_line: "Own line",
    when_broken: "When multi-line",
    per_block: "Per block",
  };

  function choiceLabel(choice) {
    if (CHOICE_LABELS[choice]) return CHOICE_LABELS[choice];
    const s = String(choice).replace(/_/g, " ");
    return s.charAt(0).toUpperCase() + s.slice(1);
  }

  function display(setting, value) {
    switch (setting.kind.type) {
      case "bool":
        return value ? "On" : "Off";
      case "choice":
        return choiceLabel(value);
      default:
        return String(value);
    }
  }

  const isChanged = (key) => !same(state.values[key], state.settings.get(key).default);

  // ---- line diff ------------------------------------------------------------------

  // Which lines of `a` were removed and which of `b` were added (LCS). Each
  // changed line maps to the ranges of its words that changed, when it was
  // replaced by a similar line, or to null when it was added or removed whole.
  // `pairs` lists, in order, the lines of `a` kept or replaced in `b`.
  function diff(a, b) {
    const n = a.length;
    const m = b.length;
    const lcs = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
      }
    }
    const before = new Map(); // line index in a -> [[start, end], ...] or null
    const after = new Map();
    const pairs = []; // [line index in a, line index in b]
    let i = 0;
    let j = 0;
    let dels = [];
    let adds = [];
    const pair = (di, aj) => {
      const words = wordDiff(a[di], b[aj]);
      before.set(di, words && words.before);
      after.set(aj, words && words.after);
      pairs.push([di, aj]);
    };
    // In order, pair the removed and added lines one for one.
    const pairInOrder = (ds, as) => {
      ds.forEach((di, k) => (k < as.length ? pair(di, as[k]) : before.set(di, null)));
      as.slice(ds.length).forEach((aj) => after.set(aj, null));
    };
    // A removed line and an added one that differ only in spacing or case
    // are the same line changed (re-indented, say): pair those first, then
    // the rest in order between them.
    const flush = () => {
      const bare = (line) => line.replace(/[ \t]+/g, "").toLowerCase();
      const { keptA, keptB } = common(
        dels.map((di) => bare(a[di])),
        adds.map((aj) => bare(b[aj])),
        (x, y) => x === y && x !== "",
      );
      let restD = [];
      let restA = [];
      let k = 0;
      dels.forEach((di, x) => {
        if (!keptA[x]) {
          restD.push(di);
          return;
        }
        while (!keptB[k]) restA.push(adds[k++]);
        pairInOrder(restD, restA);
        restD = [];
        restA = [];
        pair(di, adds[k++]);
      });
      pairInOrder(restD, restA.concat(adds.slice(k)));
      dels = [];
      adds = [];
    };
    while (i < n || j < m) {
      if (i < n && j < m && a[i] === b[j]) {
        flush();
        pairs.push([i, j]);
        i++;
        j++;
      } else if (j >= m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1])) {
        dels.push(i++);
      } else {
        adds.push(j++);
      }
    }
    flush();
    return { before, after, pairs };
  }

  // A line as words, runs of spaces and tabs, and single symbols, with their offsets.
  function tokens(line) {
    const out = [];
    const re = /[A-Za-z0-9_]+|[ \t]+|[^A-Za-z0-9_ \t]/g;
    let t;
    while ((t = re.exec(line))) out.push({ text: t[0], start: t.index, end: t.index + t[0].length });
    return out;
  }

  // Which tokens of `a` and `b` belong to their longest common subsequence
  // under `same`.
  function common(a, b, same) {
    const n = a.length;
    const m = b.length;
    const lcs = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        lcs[i][j] = same(a[i], b[j]) ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
      }
    }
    const keptA = new Array(n).fill(false);
    const keptB = new Array(m).fill(false);
    for (let i = 0, j = 0; i < n && j < m; ) {
      if (same(a[i], b[j])) {
        keptA[i] = keptB[j] = true;
        i++;
        j++;
      } else if (lcs[i + 1][j] >= lcs[i][j + 1]) i++;
      else j++;
    }
    return { keptA, keptB };
  }

  // The ranges of the words that differ between two versions of a line, as
  // GitHub marks them: only what changed, so `IF bStart` against `if bStart`
  // marks IF and if, not bStart. Null when the lines have too little in common
  // (ignoring case) for word marks to help, as when a long line wraps.
  function wordDiff(x, y) {
    const a = tokens(x);
    const b = tokens(y);
    const visible = (toks, kept) =>
      toks.reduce((sum, t, k) => sum + (kept && !kept[k] ? 0 : t.text.trim().length), 0);
    const alike = common(a, b, (p, q) => p.text.toLowerCase() === q.text.toLowerCase());
    const shared = visible(a, alike.keptA) + visible(b, alike.keptB);
    if (shared < 0.5 * (visible(a) + visible(b))) return null;
    const { keptA, keptB } = common(a, b, (p, q) => p.text === q.text);
    const ranges = (toks, kept) => {
      const out = [];
      toks.forEach((t, k) => {
        if (kept[k]) return;
        const last = out[out.length - 1];
        if (last && last[1] === t.start) last[1] = t.end;
        else out.push([t.start, t.end]);
      });
      return out;
    };
    return { before: ranges(a, keptA), after: ranges(b, keptB) };
  }

  // ---- code blocks --------------------------------------------------------------------

  // One line of code: its number, a change sign, and the text. `ranges` mark
  // the parts that changed. Leading whitespace is drawn faintly (· space,
  // → tab) so indentation shows, and changed whitespace is drawn the same way
  // inside its mark, so an added or removed space is visible.
  function codeLine(number, text, kind, ranges) {
    const row = el("div", kind ? "line " + kind : "line");
    const sign = kind === "del" ? "−" : kind === "add" ? "+" : "";
    row.append(el("span", "line-no", String(number)), el("span", "line-sign", sign));
    const code = el("span", "line-code");
    const lead = /^[ \t]*/.exec(text)[0].length;
    const marks = ranges || [];
    const cuts = new Set([0, lead, text.length]);
    tokens(text).forEach((t) => cuts.add(t.start).add(t.end));
    marks.forEach(([s, e]) => cuts.add(s).add(e));
    const points = [...cuts].sort((p, q) => p - q);
    for (let k = 0; k + 1 < points.length; k++) {
      const [s, e] = [points[k], points[k + 1]];
      const piece = text.slice(s, e);
      const marked = marks.some(([ms, me]) => s >= ms && e <= me);
      let node =
        /^[ \t]+$/.test(piece) && (e <= lead || marked)
          ? el("span", "ws", piece.replace(/\t/g, "→\t").replace(/ /g, "·"))
          : document.createTextNode(piece);
      if (marked) {
        const m = el("mark");
        m.append(node);
        node = m;
      }
      code.append(node);
    }
    row.append(code);
    return row;
  }

  function renderLines(box, text, marks, kind) {
    box.replaceChildren(
      ...text
        .split("\n")
        .map((line, i) => (marks && marks.has(i) ? codeLine(i + 1, line, kind, marks.get(i)) : codeLine(i + 1, line))),
    );
  }

  // A titled block of code: `role` (Default, Current, ...) and the value it shows.
  function codeBlock(role, value, aside, text, marks, kind, tabSize) {
    const fig = el("figure", "code-block");
    const head = el("figcaption", "code-head");
    head.append(el("span", "code-role", role), el("span", "code-value", value));
    if (aside) {
      const a = el("span", "code-aside", aside);
      a.title = aside;
      head.append(a);
    }
    const code = el("div", "code");
    code.style.tabSize = tabSize;
    const lines = el("div", "code-lines");
    renderLines(lines, text, marks, kind);
    code.append(lines);
    fig.append(head, code);
    return fig;
  }

  // ---- rows -----------------------------------------------------------------------------

  function buildControl(setting, row) {
    const key = setting.key;
    const kind = setting.kind;
    const labelled = (node) => node.setAttribute("aria-labelledby", "label-" + key);

    if (kind.type === "bool") {
      const box = el("span", "switch-box");
      const text = el("span", "switch-text");
      text.setAttribute("aria-hidden", "true");
      const sw = el("button", "switch");
      sw.type = "button";
      sw.setAttribute("role", "switch");
      labelled(sw);
      sw.addEventListener("click", () => setValue(key, !state.values[key]));
      box.append(text, sw);
      row.controls = [sw];
      row.sync = (v) => {
        sw.setAttribute("aria-checked", String(Boolean(v)));
        text.textContent = v ? "On" : "Off";
      };
      return box;
    }

    if (kind.type === "choice") {
      const box = el("span", "select");
      const select = el("select");
      labelled(select);
      kind.choices.forEach((choice) => {
        const option = el("option", null, choiceLabel(choice));
        option.value = choice;
        select.append(option);
      });
      select.addEventListener("change", () => setValue(key, select.value));
      box.append(select);
      row.controls = [select];
      row.sync = (v) => {
        select.value = v;
      };
      return box;
    }

    const input = el("input", "input number");
    input.type = "number";
    input.inputMode = "numeric";
    input.min = kind.min;
    input.max = kind.max;
    input.step = 1;
    input.title = kind.min + " to " + kind.max;
    labelled(input);
    const parse = () => {
      const n = Number(input.value);
      return input.value.trim() !== "" && Number.isInteger(n) ? n : null;
    };
    // Preview while typing once the number is valid; settle it when done.
    input.addEventListener("input", () => {
      const n = parse();
      if (n !== null && n >= kind.min && n <= kind.max) setValue(key, n);
    });
    input.addEventListener("change", () => {
      const n = parse();
      const v = n === null ? state.values[key] : Math.min(kind.max, Math.max(kind.min, n));
      input.value = v;
      setValue(key, v);
    });
    input.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") input.blur();
    });
    row.controls = [input];
    row.sync = (v) => {
      if (document.activeElement !== input) input.value = v;
    };
    return input;
  }

  function buildRow(setting) {
    const key = setting.key;
    const row = { setting };
    const root = el("div", "setting");
    root.id = "setting-" + key;
    row.root = root;

    const text = el("div", "setting-text");
    const label = el("div", "setting-label", setting.label);
    label.id = "label-" + key;
    row.error = el("p", "setting-error");
    row.error.hidden = true;
    text.append(label, el("p", "setting-desc", setting.description), row.error);

    const control = el("div", "setting-control");
    row.reset = el("button", "reset", "Reset");
    row.reset.type = "button";
    row.reset.title = "Back to the default: " + display(setting, setting.default);
    row.reset.setAttribute("aria-label", "Reset " + setting.label);
    row.reset.addEventListener("click", () => setValue(key, setting.default));
    control.append(row.reset, buildControl(setting, row));

    root.append(text, control);
    root.addEventListener("click", () => select(key));
    root.addEventListener("focusin", () => select(key));
    state.rows.set(key, row);
    return root;
  }

  function refreshRow(key) {
    const row = state.rows.get(key);
    const changed = isChanged(key);
    row.sync(state.values[key]);
    row.root.classList.toggle("modified", changed);
    row.reset.classList.toggle("unused", !changed);
    row.reset.disabled = !changed || !state.online;
    row.controls.forEach((c) => {
      c.disabled = !state.online;
    });
    const err = state.errors[key];
    row.error.textContent = err || "";
    row.error.hidden = !err;
  }

  // ---- preview: the selected setting -------------------------------------------------------

  function select(key) {
    if (state.selected === key) return;
    if (state.selected) state.rows.get(state.selected).root.classList.remove("selected");
    state.selected = key;
    state.rows.get(key).root.classList.add("selected");
    renderPreview();
  }

  // A tab is drawn as wide as the indent width it stands for, so that setting
  // shows even where the text itself is the same.
  const tabFor = (key, v) => (key === "indent_width" ? v : state.values.indent_width);
  const looksDifferent = (key, x, y, vx, vy) => x !== y || (key === "indent_width" && y.includes("\t") && vx !== vy);

  // The selected setting's sample with its default value beside the value
  // shown on the page. Always the same two blocks, with any note in the
  // second's caption, so selecting or changing a setting moves nothing.
  function renderPreview() {
    const key = state.selected;
    if (!key) return;
    const s = state.settings.get(key);
    const value = state.values[key];

    $("pv-title").textContent = s.label;
    $("pv-key").textContent = s.key;
    $("pv-meta").textContent =
      "Default: " +
      display(s, s.default) +
      (s.kind.type === "integer" ? " · Allowed: " + s.kind.min + " to " + s.kind.max : "");

    const p = state.previews[key] || { before: "", after: "" };
    let note = "";
    let d = { before: null, after: null };
    if (key === "indent_width" && state.values.indent_style === "tabs") {
      note = "With tabs, the indent width sets how wide a tab is shown.";
    }
    if (!isChanged(key)) {
      note = note || "At its default: choose another value to compare the two.";
    } else if (!looksDifferent(key, p.before, p.after, s.default, value)) {
      note = note || "Same result as the default on this sample.";
    } else {
      d = compare(p.before, p.after);
    }
    const error = state.errors[key];
    const tab = (v) => tabFor(key, v);
    const current = codeBlock("Current", display(s, value), error || note, p.after, d.after, "add", tab(value));
    if (error) current.querySelector(".code-aside").classList.add("error");
    $("pv-blocks").replaceChildren(
      codeBlock("Default", display(s, s.default), null, p.before, d.before, "del", tab(s.default)),
      current,
    );
  }

  // The changed lines of two renderings of a sample. The same text can still
  // differ in how wide its tabs are drawn: then the tab indentation is marked.
  function compare(a, b) {
    if (a !== b) return diff(a.split("\n"), b.split("\n"));
    const tabs = (text) => {
      const marks = new Map();
      text.split("\n").forEach((line, i) => {
        const lead = /^\t+/.exec(line);
        if (lead) marks.set(i, [[0, lead[0].length]]);
      });
      return marks;
    };
    return { before: tabs(a), after: tabs(b) };
  }

  // ---- preview: your code -------------------------------------------------------------------

  // The code under the sample, in two parts as a POU has them: each typed in
  // a text area and shown beside it formatted with the values on the page,
  // what formatting changed marked.
  const PARTS = [
    ["decl", "declaration"],
    ["impl", "implementation"],
  ].map(([id, name]) => ({
    name, // the part's key in the example
    input: $(id + "-input"),
    numbers: $(id + "-numbers"),
    output: $(id + "-output"),
    note: $(id + "-note"),
    row: $("row-" + id),
    map: null, // where each input line falls in the formatted code (lineMap)
    lines: 0, // input lines numbered
  }));

  let codeSeq = 0;
  const formatCode = debounce(async () => {
    if (!state.online || !state.example) return;
    const seq = ++codeSeq;
    try {
      const results = await Promise.all(
        PARTS.map((part) =>
          part.input.value.trim()
            ? api("POST", "/api/format", {
                values: state.values,
                source: part.input.value,
                declaration: part.name === "declaration",
              })
            : null,
        ),
      );
      if (seq !== codeSeq) return;
      PARTS.forEach((part, k) => renderPart(part, results[k]));
    } catch (e) {
      lost(e);
    }
  }, 150);

  // Show a part formatted, the lines formatting changed marked.
  function renderPart(part, result) {
    const box = part.output.parentElement;
    part.input.style.tabSize = box.style.tabSize = state.values.indent_width;
    part.note.className = "code-aside";
    part.note.replaceChildren();
    part.note.title = "";
    if (!result) {
      part.output.replaceChildren();
      part.map = null;
      numberLines(part);
      return;
    }
    // Keep the last output shown, with what stops this one.
    if (result.error) {
      part.note.textContent = part.note.title = result.error;
      part.note.classList.add("error");
      return;
    }
    const input = part.input.value.replace(/\r\n/g, "\n").split("\n");
    const output = result.output.replace(/\r\n/g, "\n");
    const lines = output.split("\n");
    const d = diff(input, lines);
    renderLines(part.output, output, d.after, "add");
    part.map = lineMap(d.pairs, input.length, lines.length);
    numberLines(part);
    const changed = d.after.size;
    part.note.textContent = !changed
      ? "Already formatted"
      : changed === 1
        ? "1 line changed"
        : changed + " lines changed";
    follow(part.input, box, part.map, false);
  }

  // The input's line numbers, and the width of both panes' number columns.
  function numberLines(part) {
    const count = part.input.value.split("\n").length;
    if (count !== part.lines) {
      part.lines = count;
      part.numbers.textContent = Array.from({ length: count }, (_, i) => i + 1).join("\n");
    }
    const widest = Math.max(count, part.output.childElementCount);
    part.row.style.setProperty("--digits", String(widest).length);
  }

  // The two panes of a part scroll together. The point kept level slides from
  // the top of the view to its bottom as a pane scrolls down, so both reach
  // their top and their end together.
  const LINE_H = 20; // px, in both panes
  const PAD_TOP = 8; // px above the first line, in both panes
  const SMOOTH = 12; // lines either side the line map is averaged over
  const moved = new Map(); // pane -> [top, left] it was scrolled to by the other

  // Where each line of the input, and its end, falls in the formatted code:
  // through the lines the diff paired, stretched in between, then averaged
  // over a few lines either side. Paired lines alone would make the other
  // pane stop on a removed blank line and rush through a statement broken
  // over four; averaged, it changes pace gradually and stays level within a
  // few lines. What is averaged is the gap to an even spread of the formatted
  // lines over the input's, mirrored past both ends with its sign flipped,
  // so the ends stay where they are without the window narrowing there.
  function lineMap(pairs, n, m) {
    const points = [[0, 0], ...pairs.filter(([i]) => i > 0), [n, m]];
    const even = (i) => (n ? (i * m) / n : 0);
    const gap = new Float64Array(n + 1);
    for (let i = 0, k = 0; i <= n; i++) {
      while (points[k + 1][0] < i) k++;
      const [x0, y0] = points[k];
      const [x1, y1] = points[k + 1];
      gap[i] = (x1 > x0 ? y0 + ((i - x0) * (y1 - y0)) / (x1 - x0) : y1) - even(i);
    }
    const mirrored = (i) => (i < 0 ? -gap[Math.min(-i, n)] : i > n ? -gap[Math.max(2 * n - i, 0)] : gap[i]);
    const map = new Float64Array(n + 1);
    for (let i = 0; i <= n; i++) {
      let sum = 0;
      for (let j = i - SMOOTH; j <= i + SMOOTH; j++) sum += mirrored(j);
      map[i] = Math.max(i ? map[i - 1] : 0, even(i) + sum / (2 * SMOOTH + 1));
    }
    return map;
  }

  // Line `line` of the input in the formatted code, or with `back` the other
  // way. Past either end, both go line for line.
  function across(map, line, back) {
    const n = map.length - 1;
    const [from, to] = back ? [map[n], n] : [n, map[n]];
    if (line <= 0) return line;
    if (line >= from) return to + line - from;
    if (!back) {
      const i = Math.floor(line);
      return map[i] + (line - i) * (map[i + 1] - map[i]);
    }
    let lo = 0;
    let hi = n;
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (map[mid] <= line) lo = mid;
      else hi = mid;
    }
    const span = map[hi] - map[lo];
    return lo + (span > 0 ? (line - map[lo]) / span : 0);
  }

  function follow(from, to, map, back) {
    if (!map) return;
    const range = from.scrollHeight - from.clientHeight;
    const r = range > 0 ? from.scrollTop / range : 0;
    const line = (from.scrollTop + r * from.clientHeight - PAD_TOP) / LINE_H;
    to.scrollTop = PAD_TOP + across(map, line, back) * LINE_H - r * to.clientHeight;
    to.scrollLeft = from.scrollLeft;
    moved.set(to, [to.scrollTop, to.scrollLeft]);
  }

  // A pane moved by the other fires a scroll event too: that one is not
  // followed back.
  function linkPanes(part) {
    const input = part.input;
    const output = part.output.parentElement;
    [
      [input, output, 0],
      [output, input, 1],
    ].forEach(([from, to, side]) =>
      from.addEventListener(
        "scroll",
        () => {
          const was = moved.get(from);
          moved.delete(from);
          if (was && Math.abs(from.scrollTop - was[0]) < 1 && Math.abs(from.scrollLeft - was[1]) < 1) return;
          follow(from, to, part.map, side === 1);
        },
        { passive: true },
      ),
    );
    input.addEventListener("scroll", () => {
      part.numbers.style.transform = "translateY(" + -input.scrollTop + "px)";
    });
  }

  // Put the example back in both parts.
  function restoreExample() {
    PARTS.forEach((part) => {
      part.input.value = state.example[part.name];
      part.input.scrollTop = part.input.scrollLeft = 0;
      numberLines(part);
    });
    refreshRestore();
    formatCode();
  }

  function refreshRestore() {
    $("restore").disabled =
      !state.online || !state.example || PARTS.every((part) => part.input.value === state.example[part.name]);
  }

  // ---- page state ----------------------------------------------------------------------

  function changedCount() {
    let n = 0;
    state.settings.forEach((_, key) => {
      if (!same(state.values[key], state.saved[key])) n++;
    });
    return n;
  }

  const isDirty = () => changedCount() > 0;

  let flashTimer = null;
  function flash(text) {
    state.flash = text;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      state.flash = null;
      refreshStatus();
    }, 4000);
    refreshStatus();
  }

  function refreshStatus() {
    const pending = changedCount();
    const status = $("status");
    status.className = "status";
    if (!state.online) {
      status.textContent = "Disconnected";
      status.classList.add("offline");
    } else if (state.saving) {
      status.textContent = "Saving…";
    } else if (pending) {
      status.textContent = pending === 1 ? "1 unsaved change" : pending + " unsaved changes";
      status.classList.add("dirty");
    } else if (state.flash) {
      status.textContent = state.flash;
    } else if (state.exists) {
      status.textContent = "Saved";
    } else {
      status.textContent = "Defaults, no file yet";
    }
    const hasErrors = Object.keys(state.errors).length > 0;
    $("save").disabled = !state.online || state.saving || !pending || hasErrors;
    $("discard").disabled = !state.online || state.saving || !pending;
    const path = el("bdi", null, state.path);
    path.dir = "ltr";
    $("file").replaceChildren(path);
    $("file").title = state.path;
    $("file-tag").hidden = state.exists || !state.path;
  }

  function refreshNav() {
    state.groups.forEach((g) => {
      const n = g.settings.filter((s) => isChanged(s.key)).length;
      g.count.textContent = n;
      g.count.hidden = n === 0;
      g.count.title = n === 1 ? "1 setting modified" : n + " settings modified";
    });
  }

  function refreshAll() {
    state.settings.forEach((_, key) => refreshRow(key));
    refreshNav();
    refreshStatus();
    applyFilter();
    renderPreview();
  }

  function setValue(key, value) {
    if (!state.online || same(value, state.values[key])) return;
    state.values[key] = value;
    delete state.errors[key];
    refreshRow(key);
    refreshNav();
    refreshStatus();
    if ($("only-changed").checked) applyFilter();
    select(key);
    updatePreviews();
    formatCode();
  }

  let previewSeq = 0;
  const updatePreviews = debounce(async () => {
    const seq = ++previewSeq;
    try {
      const res = await api("POST", "/api/preview", { values: state.values });
      if (seq !== previewSeq) return;
      state.previews = res.previews;
      state.errors = {};
      res.errors.forEach((e) => {
        state.errors[e.key] = e.message;
      });
      refreshAll();
    } catch (e) {
      lost(e);
    }
  }, 120);

  // ---- loading, saving ----------------------------------------------------------------

  function load(data) {
    state.path = data.path;
    state.exists = data.exists;
    state.version = data.version;
    state.previews = data.previews;
    // The sample blocks are as tall as the tallest sample at its default, so
    // they keep their height whatever setting is selected.
    const tallest = Math.max(1, ...Object.values(data.previews).map((p) => p.before.split("\n").length));
    $("pv-blocks").style.setProperty("--sample-lines", tallest);
    state.errors = {};
    state.values = {};
    data.catalog.groups.forEach((g) =>
      g.settings.forEach((s) => {
        state.values[s.key] = s.value;
      }),
    );
    // An invalid file starts from the defaults: everything shown is unsaved.
    state.saved = data.problems.length ? {} : copy(state.values);
    if (!state.groups.length) buildPage(data.catalog);
    clearBanners();
    if (data.problems.length) {
      banner(
        "warn",
        "This file has problems, so the page starts from the defaults. Saving replaces the file.",
        data.problems,
      );
    }
    $("footer").textContent =
      "TcTidy " + data.tctidy_version + ". Settings left at their default are not written to the file.";
    if (!state.selected) select(state.settings.keys().next().value);
    refreshAll();
    // A reload keeps the code the user typed.
    if (!state.example) {
      state.example = data.example;
      restoreExample();
    }
    formatCode();
  }

  function buildPage(catalog) {
    const groupsRoot = $("groups");
    const nav = $("nav");
    catalog.groups.forEach((g) => {
      const section = el("section", "group");
      section.id = "group-" + g.id;
      const title = el("h2", "group-title", g.label);
      title.id = "group-title-" + g.id;
      section.setAttribute("aria-labelledby", title.id);
      section.append(title);
      g.settings.forEach((s) => {
        state.settings.set(s.key, s);
        section.append(buildRow(s));
      });
      groupsRoot.append(section);

      const group = {};
      const item = el("li");
      const link = el("a");
      link.href = "#" + section.id;
      link.addEventListener("click", (ev) => {
        // Scroll without touching the address bar, where only the token goes.
        ev.preventDefault();
        pinned = group;
        section.scrollIntoView({ block: "start" });
        markActiveGroup();
      });
      const count = el("span", "nav-count");
      count.hidden = true;
      link.append(el("span", null, g.label), count);
      item.append(link);
      nav.append(item);
      Object.assign(group, { id: g.id, settings: g.settings, section, link, item, count });
      state.groups.push(group);
    });
    watchSections();
  }

  async function reload() {
    try {
      load(await api("GET", "/api/state"));
    } catch (e) {
      lost(e);
    }
  }

  async function save(force) {
    if (state.saving) return;
    state.saving = true;
    refreshStatus();
    try {
      const res = await api("POST", "/api/save", {
        values: state.values,
        version: state.version,
        force: Boolean(force),
      });
      state.saved = copy(state.values);
      state.version = res.version;
      state.exists = true;
      state.saves++;
      state.conflict = false;
      clearBanners();
      state.saving = false;
      // Saved, but not safe from a power loss yet: keep that in view.
      if (res.warning) banner("warn", res.warning);
      flash(res.warning ? "Saved, not yet safe on disk" : "Saved");
    } catch (e) {
      if (e.status === 409) showConflict();
      else if (e.status === 422 && e.data && e.data.errors) {
        e.data.errors.forEach((x) => {
          state.errors[x.key] = x.message;
        });
        refreshAll();
      } else lost(e);
    } finally {
      state.saving = false;
      refreshStatus();
    }
  }

  function discard() {
    // An invalid file has nothing to go back to but the defaults.
    state.settings.forEach((s, key) => {
      state.values[key] = key in state.saved ? state.saved[key] : s.default;
    });
    state.errors = {};
    refreshAll();
    updatePreviews();
    formatCode();
  }

  function showConflict() {
    if (state.conflict) return;
    state.conflict = true;
    const b = banner(
      "warn",
      "The file changed on disk since this page loaded it. Reload it (your unsaved changes here are dropped), or overwrite it with the values on this page.",
    );
    const actions = el("div", "banner-actions");
    const reloadBtn = el("button", "btn btn-small", "Reload");
    reloadBtn.type = "button";
    reloadBtn.addEventListener("click", () => {
      state.conflict = false;
      reload();
    });
    const overwrite = el("button", "btn btn-small", "Overwrite");
    overwrite.type = "button";
    overwrite.addEventListener("click", () => {
      b.remove();
      state.conflict = false;
      save(true);
    });
    actions.append(reloadBtn, overwrite);
    b.append(actions);
  }

  // Notice edits made to the file elsewhere (by hand, or a checkout): reload
  // quietly when nothing here is unsaved, otherwise ask.
  async function ping() {
    if (!state.online) return;
    const saves = state.saves;
    try {
      const res = await api("POST", "/api/ping");
      if (res.version === state.version || state.saving || saves !== state.saves) return;
      if (isDirty()) showConflict();
      else {
        await reload();
        flash("Reloaded, the file changed on disk");
      }
    } catch (e) {
      lost(e);
    }
  }

  function lost(e) {
    if (!state.online) return;
    if (e && e.status && e.status !== 401) {
      banner("error", "Something went wrong: " + e.message);
      return;
    }
    state.online = false;
    clearBanners();
    banner(
      "error",
      (e && e.status === 401
        ? "This page has no access to the settings server. "
        : "The settings server has stopped, so changes can no longer be previewed or saved. ") + REOPEN,
    );
    document.querySelectorAll("main button, main input, main select, aside button, aside textarea").forEach((c) => {
      c.disabled = true;
    });
    if (state.groups.length) refreshAll();
    else refreshStatus();
  }

  // ---- banners ----------------------------------------------------------------------------

  function banner(kind, message, items) {
    const b = el("div", "banner " + kind);
    b.setAttribute("role", kind === "error" ? "alert" : "status");
    const icon = el("span", "banner-icon", "!");
    icon.setAttribute("aria-hidden", "true");
    const text = el("div", "banner-text", message);
    if (items && items.length) {
      const ul = el("ul");
      items.forEach((i) => ul.append(el("li", null, i)));
      text.append(ul);
    }
    b.append(icon, text);
    $("banners").append(b);
    return b;
  }

  function clearBanners() {
    $("banners").replaceChildren();
  }

  // ---- search, filter, navigation ----------------------------------------------------------

  function applyFilter() {
    const q = $("search").value.trim().toLowerCase();
    const onlyChanged = $("only-changed").checked;
    let any = false;
    state.groups.forEach((g) => {
      let visible = 0;
      g.settings.forEach((s) => {
        const text = (g.label + " " + s.label + " " + s.key + " " + s.description).toLowerCase();
        const show = (!q || text.includes(q)) && (!onlyChanged || isChanged(s.key));
        state.rows.get(s.key).root.hidden = !show;
        if (show) visible++;
      });
      g.section.hidden = visible === 0;
      g.item.hidden = visible === 0;
      any = any || visible > 0;
    });
    $("no-match").hidden = any;
    markActiveGroup();
    $("no-match").textContent =
      onlyChanged && !q ? "Every setting is at its default." : "No setting matches your search.";
  }

  // The group under the toolbar is the active one in the navigation; a group
  // picked there stays active even when the list cannot scroll it to the top.
  let pinned = null;

  function markActiveGroup() {
    const list = $("settings");
    const line = list.getBoundingClientRect().top + list.querySelector(".toolbar").offsetHeight + 8;
    const shown = state.groups.filter((g) => !g.section.hidden);
    let active = shown[0] || null;
    shown.forEach((g) => {
      if (g.section.getBoundingClientRect().top <= line) active = g;
    });
    if (pinned && !pinned.section.hidden) active = pinned;
    state.groups.forEach((g) => g.link.classList.toggle("active", g === active));
  }

  function watchSections() {
    const list = $("settings");
    let queued = false;
    list.addEventListener(
      "scroll",
      () => {
        if (queued) return;
        queued = true;
        requestAnimationFrame(() => {
          queued = false;
          markActiveGroup();
        });
      },
      { passive: true },
    );
    ["wheel", "touchstart", "pointerdown", "keydown"].forEach((type) =>
      list.addEventListener(
        type,
        () => {
          pinned = null;
        },
        { passive: true },
      ),
    );
    markActiveGroup();
  }

  // ---- start ----------------------------------------------------------------------------------

  $("save").addEventListener("click", () => save(false));
  $("discard").addEventListener("click", discard);
  $("search").addEventListener("input", applyFilter);
  $("only-changed").addEventListener("change", applyFilter);
  $("restore").addEventListener("click", restoreExample);
  PARTS.forEach((part) => {
    part.input.addEventListener("input", () => {
      numberLines(part);
      refreshRestore();
      formatCode();
    });
    linkPanes(part);
    numberLines(part);
  });
  document.addEventListener("keydown", (ev) => {
    if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "s") {
      ev.preventDefault();
      if (!$("save").disabled) save(false);
    }
  });
  window.addEventListener("beforeunload", (ev) => {
    if (state.online && isDirty()) {
      ev.preventDefault();
      ev.returnValue = "";
    }
  });
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") ping();
  });

  (async () => {
    let data;
    try {
      data = await api("GET", "/api/state");
    } catch (e) {
      state.path = "";
      lost(e);
      return;
    }
    load(data);
    setInterval(ping, PING_MS);
  })();
})();
