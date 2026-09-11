import { useMemo } from "react";
import { highlightCached, escapeHtml } from "../highlight.js";

/**
 * Static syntax-highlighted code block for docs.
 * lang: 'ruby' highlights tokens; anything else renders escaped plain text.
 */
export default function CodeBlock({ lang = "ruby", children }) {
  const text = useMemo(
    () => String(children).replace(/^\n+/, "").replace(/\s+$/, ""),
    [children],
  );

  const body = useMemo(() => {
    if (lang !== "ruby") {
      return escapeHtml(text);
    }
    return text
      .split("\n")
      .map((line) =>
        highlightCached(line)
          .map((tok) =>
            tok.c
              ? `<span class="tok-${tok.c}">${escapeHtml(tok.t)}</span>`
              : escapeHtml(tok.t),
          )
          .join(""),
      )
      .join("\n");
  }, [text, lang]);

  return (
    <figure className="codeblock" style={{ margin: "14px 0 20px" }}>
      <span className="lang-tag">{lang}</span>
      <pre>
        <code dangerouslySetInnerHTML={{ __html: body }} />
      </pre>
    </figure>
  );
}
