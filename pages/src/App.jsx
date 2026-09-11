import { useEffect, useState } from "react";
import Sidebar, { GemIcon } from "./components/Sidebar.jsx";
import ThemeToggle from "./components/ThemeToggle.jsx";
import Demo from "./components/Demo.jsx";
import CodeBlock from "./components/CodeBlock.jsx";
import { GROUPS, PAGES, STORIES, REPOSITORY } from "./content/catalog.js";

function currentPage() {
  return (
    window.location.hash.replace(/^#\/?/, "").replace(/\/$/, "") || "overview"
  );
}

function Overview() {
  return (
    <>
      <header className="hero">
        <p className="eyebrow">RUBY FAST LSP · DOCUMENTATION</p>
        <h1>
          See the types flowing
          <br />
          through your <em>Ruby.</em>
        </h1>
        <p className="lead">
          Follow values through methods, collections, and Hashes. Get useful
          types, navigation, and diagnostics while you work—with clear
          explanations when a type remains unknown.
        </p>
        <div className="hero-actions">
          <a className="btn btn-primary" href="#/install">
            Get started ↗
          </a>
          <a className="btn" href="#/types/hash-shapes">
            Explore Hash shapes
          </a>
        </div>
      </header>
      <Demo id="types/hash-shapes" title="A closer look inside your Hashes" />
      <section className="overview-section" aria-labelledby="stories-title">
        <p className="eyebrow">FROM THE VALUE TO THE EDITOR</p>
        <h2 id="stories-title">More context. Less guessing.</h2>
        <div className="story-grid">
          {STORIES.map((id, n) => {
            const p = PAGES[id];
            return (
              <a className="story" href={"#/" + id} key={id}>
                <span className="story-number">0{n + 1}</span>
                <h3>
                  {p.title}
                  <span aria-hidden="true">↗</span>
                </h3>
                <p>{p.summary}</p>
                <img
                  src={
                    import.meta.env.BASE_URL + "demos/" + p.demo + "/poster.png"
                  }
                  alt={p.title + " in VS Code"}
                  width="820"
                  height="420"
                  loading="lazy"
                />
              </a>
            );
          })}
        </div>
      </section>
      <section className="overview-section" aria-labelledby="features-title">
        <p className="eyebrow">THE EVERYDAY TOOLKIT</p>
        <h2 id="features-title">Explore the features</h2>
        <div className="feature-grid">
          {GROUPS.filter((g) => g.feature).map((g) => (
            <section className="feature-group" key={g.label}>
              <h3>{g.label}</h3>
              <p>{g.summary}</p>
              <ul>
                {g.items.map((id) => (
                  <li key={id}>
                    <a href={"#/" + id}>
                      {PAGES[id].title}
                      <span aria-hidden="true">↗</span>
                    </a>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </section>
      <section className="comparison overview-section">
        <p className="eyebrow">FIND YOUR FIT</p>
        <h2>How it compares</h2>
        <p>
          Ruby Fast LSP focuses on inferred value flow, structural Hash
          contents, and readable type information across the editor.
        </p>
        <div className="comparison-grid">
          <div>
            <h3>Ruby LSP</h3>
            <p>
              Shopify’s Ruby LSP provides a broad editor toolkit, type-aware
              completion for known receivers, and guessed types. Explore our
              Hash and collection examples to see the behavior we focus on.
            </p>
            <a href="https://shopify.github.io/ruby-lsp/#completion">
              Read Ruby LSP’s feature guide ↗
            </a>
          </div>
          <div>
            <h3>Solargraph</h3>
            <p>
              Solargraph also supports type inference, YARD annotations, and
              type checking. Our examples make specific shape, propagation, and
              presentation behavior visible, rather than claiming inference is
              exclusive.
            </p>
            <a href="https://solargraph.org/guides/type-checking">
              Read Solargraph’s type-checking guide ↗
            </a>
          </div>
        </div>
        <p className="fine-print">
          Documentation reviewed September 2026. These are differences in
          emphasis, not a comparative benchmark. Inference has{" "}
          <a href="#/types/unknown">documented limits</a>.
        </p>
      </section>
    </>
  );
}

function FeaturePage({ page }) {
  return (
    <article className="feature-page">
      <header className="page-header">
        <p className="eyebrow">{page.group}</p>
        <h1>{page.title}</h1>
        <p className="lead">{page.summary}</p>
      </header>
      {page.demo && (
        <Demo
          id={page.demo}
          title={page.demoTitle || page.title + " in practice"}
        />
      )}
      {page.intro && <p>{page.intro}</p>}
      {page.steps?.length > 0 && (
        <section>
          <h2>Try it in your editor</h2>
          <ol className="steps">
            {page.steps.map((s) => (
              <li key={s}>{s}</li>
            ))}
          </ol>
        </section>
      )}
      {page.code && (
        <section>
          <h2>A small example</h2>
          <CodeBlock lang={page.lang || "ruby"}>{page.code}</CodeBlock>
        </section>
      )}
      {page.sections?.map((s) => (
        <section key={s.title}>
          <h2>{s.title}</h2>
          {s.body && <p>{s.body}</p>}
          {s.items && (
            <ul>
              {s.items.map((i) => (
                <li key={i}>{i}</li>
              ))}
            </ul>
          )}
          {s.code && <CodeBlock lang={s.lang || "ruby"}>{s.code}</CodeBlock>}
          {s.links?.map((l) => (
            <p key={l.href}>
              <a href={l.href}>{l.label} ↗</a>
            </p>
          ))}
        </section>
      ))}
      {page.requirements && (
        <section className="note">
          <h2>Before you start</h2>
          <p>{page.requirements}</p>
        </section>
      )}
      {page.limits && (
        <section>
          <h2>What to expect</h2>
          <p>{page.limits}</p>
        </section>
      )}
      {page.contract && (
        <p className="contract-link">
          <a href={REPOSITORY + "/blob/main/" + page.contract}>
            Read the detailed support contract ↗
          </a>
        </p>
      )}
      {page.related?.length > 0 && (
        <section className="related">
          <h2>Keep exploring</h2>
          {page.related.map((id) => (
            <a className="btn" href={"#/" + id} key={id}>
              {PAGES[id].title} ↗
            </a>
          ))}
        </section>
      )}
    </article>
  );
}

export default function App() {
  const [active, setActive] = useState(currentPage);
  const [menuOpen, setMenuOpen] = useState(false);
  const page = PAGES[active];
  useEffect(() => {
    const change = () => {
      setActive(currentPage());
      setMenuOpen(false);
      window.scrollTo(0, 0);
    };
    window.addEventListener("hashchange", change);
    return () => window.removeEventListener("hashchange", change);
  }, []);
  useEffect(() => {
    document.title =
      (active === "overview"
        ? "See the types flowing through your Ruby"
        : page?.title || "Page not found") + " · Ruby Fast LSP";
    const meta = document.querySelector('meta[name="description"]');
    if (meta)
      meta.content =
        page?.summary ||
        "Ruby Fast LSP documentation: type inference, navigation, diagnostics, and project tools.";
  }, [active, page]);
  useEffect(() => {
    const close = (e) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, []);
  return (
    <>
      <a
        className="skip-link"
        href="#main"
        onClick={(e) => {
          e.preventDefault();
          document.getElementById("main").focus();
        }}
      >
        Skip to content
      </a>
      <header className="topbar">
        <button
          className="icon-btn menu-btn"
          aria-label="Toggle documentation menu"
          aria-expanded={menuOpen}
          onClick={() => setMenuOpen(!menuOpen)}
        >
          ☰
        </button>
        <a className="brand" href="#/">
          <GemIcon />
          Ruby Fast <span>LSP</span>
        </a>
        <span className="topbar-label">Docs</span>
        <div className="topbar-spacer" />
        <a className="topbar-link" href={REPOSITORY}>
          GitHub ↗
        </a>
        <ThemeToggle />
        <a className="btn btn-primary topbar-install" href="#/install">
          Install
        </a>
      </header>
      <div className="layout">
        <Sidebar
          active={active}
          open={menuOpen}
          onClose={() => setMenuOpen(false)}
        />
        <main className="content" id="main" tabIndex="-1">
          {active === "overview" ? (
            <Overview />
          ) : page ? (
            <FeaturePage key={active} page={page} />
          ) : (
            <section className="page-header">
              <h1>Page not found</h1>
              <p>This documentation page does not exist.</p>
              <a className="btn" href="#/">
                Back to the overview
              </a>
            </section>
          )}
          <footer className="site-footer">
            <a className="brand" href="#/">
              <GemIcon size={16} />
              Ruby Fast LSP
            </a>
            <p>A Ruby language server written in Rust.</p>
            <div>
              <a href="#/support">Support & limits</a>
              <a href={REPOSITORY + "/blob/main/AGENTS.md"}>Contribute</a>
              <a href={REPOSITORY + "/blob/main/editors/vscode/vsix/LICENSE"}>
                MIT License
              </a>
            </div>
          </footer>
        </main>
      </div>
    </>
  );
}
