import { useEffect, useState } from "react";
import Sidebar, { GemIcon } from "./components/Sidebar.jsx";
import ThemeToggle from "./components/ThemeToggle.jsx";
import Demo from "./components/Demo.jsx";
import CodeBlock from "./components/CodeBlock.jsx";
import { GROUPS, PAGES, TYPE_FEATURES, REPOSITORY } from "./content/catalog.js";

function currentPage() {
  return (
    window.location.hash.replace(/^#\/?/, "").replace(/\/$/, "") || "overview"
  );
}

function FeatureGroups({ section }) {
  return (
    <div className="feature-grid">
      {GROUPS.filter((group) => group.overview === section).map((group) => (
        <section className="feature-group" key={group.label}>
          <h3>{group.label}</h3>
          <p>{group.summary}</p>
          <ul>
            {group.items.map((id) => (
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
  );
}

function Overview() {
  return (
    <>
      <header className="hero">
        <p className="eyebrow">RUBY FAST LSP · DOCUMENTATION</p>
        <h1>
          Ruby tools for
          <br />
          <em>everyday coding.</em>
        </h1>
        <p className="lead">
          Jump to definitions, complete method calls, rename symbols, and catch
          errors as you edit. Ruby Fast LSP brings navigation, code assistance,
          and diagnostics to your Ruby workspace.
        </p>
        <div className="hero-actions">
          <a className="btn btn-primary" href="#/install">
            Get started ↗
          </a>
          <a className="btn" href="#/navigate/definition">
            Explore navigation
          </a>
        </div>
      </header>
      <section
        className="overview-section overview-intro"
        aria-labelledby="features-title"
      >
        <h2 id="features-title">Everyday editor tools</h2>
        <FeatureGroups section="editor" />
      </section>
      <Demo
        id="editing/completion"
        title="Complete variables and method calls"
      />
      <section className="overview-section" aria-labelledby="stories-title">
        <p className="eyebrow">TYPE INFERENCE</p>
        <h2 id="stories-title">A closer look at your values</h2>
        <p className="section-intro">
          Explore the types of method results and collection elements without
          adding annotations. For Hashes, inspect individual keys and their
          value types. Add{" "}
          <a href="#/types/signatures">YARD or RBS signatures</a> where you need
          more detail, and learn what an{" "}
          <a href="#/types/unknown">Unknown type</a> means.
        </p>
        <div className="story-grid">
          {TYPE_FEATURES.map((id) => {
            const p = PAGES[id];
            return (
              <a className="story" href={"#/" + id} key={id}>
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
      <section className="overview-section" aria-labelledby="projects-title">
        <h2 id="projects-title">Work across your Ruby projects</h2>
        <FeatureGroups section="projects" />
      </section>
      <section className="comparison overview-section">
        <p className="eyebrow">FIND YOUR FIT</p>
        <h2>How it compares</h2>
        <p>
          Alongside familiar editor features, Ruby Fast LSP offers inferred
          types and a detailed view of Hash contents. Explore the guides to see
          how these features fit your workflow.
        </p>
        <div className="comparison-grid">
          <div>
            <h3>Ruby LSP</h3>
            <p>
              Shopify’s Ruby LSP provides a broad editor toolkit, type-aware
              completion for known receivers, and guessed types.
            </p>
            <a href="https://shopify.github.io/ruby-lsp/#completion">
              Read Ruby LSP’s feature guide ↗
            </a>
          </div>
          <div>
            <h3>Solargraph</h3>
            <p>
              Solargraph supports type inference, YARD annotations, and type
              checking alongside its editor features.
            </p>
            <a href="https://solargraph.org/guides/type-checking">
              Read Solargraph’s type-checking guide ↗
            </a>
          </div>
        </div>
        <p className="fine-print">
          Features vary by tool and configuration. See each project’s guide for
          details and our <a href="#/support">support guide</a> for current
          limitations.
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
            Read the detailed guide ↗
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
        ? "Ruby tools for everyday coding"
        : page?.title || "Page not found") + " · Ruby Fast LSP";
    const meta = document.querySelector('meta[name="description"]');
    if (meta)
      meta.content =
        page?.summary ||
        "Ruby Fast LSP documentation: navigation, completion, diagnostics, and type inference for Ruby projects.";
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
