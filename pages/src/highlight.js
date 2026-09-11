const TOKEN_RE =
  /(#.*)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|\b(class|def|end|module|if|elsif|else|unless|while|until|begin|rescue|ensure|return|yield|self|nil|true|false|and|or|not|do|then|require|require_relative|attr_accessor|attr_reader|attr_writer)\b|(:[A-Za-z_]\w*)|(@[A-Za-z_]\w*)|\b([A-Z][A-Za-z0-9_]*)\b|\b(\d+)\b|([a-z_][A-Za-z0-9_]*[?!=]?)(?=\s*\()/g;

const CLASSES = ["cm", "st", "kw", "sy", "iv", "co", "nu", "fn"];

export function highlight(line) {
  const out = [];
  let last = 0;
  line.replace(TOKEN_RE, (m, ...groups) => {
    const offset = groups[groups.length - 2];
    if (offset > last) out.push({ t: line.slice(last, offset), c: "" });
    let cls = "";
    for (let i = 0; i < CLASSES.length; i++) {
      if (groups[i] !== undefined) {
        cls = CLASSES[i];
        break;
      }
    }
    out.push({ t: m, c: cls });
    last = offset + m.length;
    return m;
  });
  if (last < line.length) out.push({ t: line.slice(last), c: "" });
  return out;
}

const cache = new Map();

export function highlightCached(line) {
  let tokens = cache.get(line);
  if (!tokens) {
    tokens = highlight(line);
    if (cache.size > 600) cache.clear();
    cache.set(line, tokens);
  }
  return tokens;
}

export function escapeHtml(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
