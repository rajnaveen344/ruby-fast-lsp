import { GROUPS, PAGES } from "../content/catalog.js";
export function GemIcon({ size = 20 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <path fill="var(--accent)" d="M6 3h12l4 5-10 13L2 8z" />
      <path fill="#fff" opacity=".3" d="M6 3h12l4 5H2z" />
    </svg>
  );
}
export default function Sidebar({ active, open, onClose }) {
  return (
    <>
      <button
        tabIndex={open ? 0 : -1}
        className={"sidebar-backdrop" + (open ? " show" : "")}
        onClick={onClose}
        aria-label="Close documentation menu"
      />
      <nav
        className={"sidebar" + (open ? " open" : "")}
        aria-label="Documentation navigation"
      >
        <a
          className={
            "nav-link overview-link" + (active === "overview" ? " active" : "")
          }
          href="#/"
          onClick={onClose}
          aria-current={active === "overview" ? "page" : undefined}
        >
          Overview
        </a>
        {GROUPS.map((group) => (
          <div className="nav-group" key={group.label}>
            <p className="nav-group-label">{group.label}</p>
            {group.items.map((id) => (
              <a
                key={id}
                className={"nav-link" + (active === id ? " active" : "")}
                href={"#/" + id}
                onClick={onClose}
                aria-current={active === id ? "page" : undefined}
              >
                {PAGES[id].title}
              </a>
            ))}
          </div>
        ))}
        <div className="sidebar-footer">Open source. Built for Ruby.</div>
      </nav>
    </>
  );
}
