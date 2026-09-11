import { useEffect, useState } from "react";

export default function Demo({ id, title }) {
  const [paused, setPaused] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [failed, setFailed] = useState(false);
  const root = import.meta.env.BASE_URL + "demos/" + id + "/";
  useEffect(() => {
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updateMotion = (event) => setPaused(event.matches);
    preference.addEventListener("change", updateMotion);
    return () => preference.removeEventListener("change", updateMotion);
  }, []);
  return (
    <figure className="demo">
      <div className="demo-heading">
        <span>{title}</span>
        <span className="demo-editor">VS Code</span>
      </div>
      <img
        className="demo-image"
        src={root + (paused || failed ? "poster.png" : "demo.gif")}
        alt={title}
        loading="lazy"
        onError={() => setFailed(true)}
      />
      <figcaption>
        <span>Real editor capture · AI suggestions off</span>
        <span>
          {failed ? (
            <span>Animation unavailable.</span>
          ) : (
            <button
              className="demo-motion"
              type="button"
              onClick={() => setPaused(!paused)}
              aria-label={(paused ? "Play" : "Pause") + " animation: " + title}
            >
              {paused ? "Play loop" : "Pause loop"}
            </button>
          )}
          <a href={root + "demo.gif"}>GIF ↗</a>
          <a href={root + "README.md"}>Steps & capture notes ↗</a>
        </span>
      </figcaption>
    </figure>
  );
}
