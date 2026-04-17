#!/usr/bin/env bash
# snapshot_corpus.sh — fetch pinned OSS Ruby projects for perf benchmarks.
#
# Usage:
#   scripts/snapshot_corpus.sh <discourse|mastodon|all>
#
# Downloads the pinned revision's source tarball from GitHub, extracts
# only *.rb files, and drops them into target/perf-corpus/<name>/ with a
# .corpus-ready marker so src/perf/corpus.rs ensure_corpus() picks them
# up without further work.
#
# Pinned revisions (bump intentionally):
#   discourse  v3.3.2   (released 2024-09)
#   mastodon   v4.3.0   (released 2024-10)
#
# Why pinned: perf baselines are meaningless if the corpus drifts.
# Bumping the pin = expect to regenerate baselines.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CACHE_ROOT="$ROOT/target/perf-corpus"
mkdir -p "$CACHE_ROOT"

pin_for() {
    case "$1" in
        discourse) echo "discourse/discourse v3.3.2" ;;
        mastodon)  echo "mastodon/mastodon  v4.3.0" ;;
        *) echo "unknown corpus: $1" >&2; exit 2 ;;
    esac
}

snapshot() {
    local name="$1"
    read -r repo tag <<< "$(pin_for "$name")"

    local dest="$CACHE_ROOT/$name"
    local marker="$dest/.corpus-ready"
    if [[ -f "$marker" ]]; then
        echo "[$name] already cached at $dest — skip (remove the dir to re-fetch)"
        return 0
    fi

    local url="https://github.com/${repo}/archive/refs/tags/${tag}.tar.gz"
    local scratch="${dest}.extracting"
    rm -rf "$scratch" "$dest"
    mkdir -p "$scratch"

    echo "[$name] fetching $repo @ $tag"
    # -L follow redirects, -f fail on 404, --retry to survive flaky network
    curl -fL --retry 3 --retry-delay 2 -o "$scratch/src.tar.gz" "$url"

    echo "[$name] extracting *.rb"
    # --wildcards for GNU tar; macOS bsdtar accepts wildcards without flag
    if tar --version 2>/dev/null | grep -q 'GNU tar'; then
        tar -xzf "$scratch/src.tar.gz" -C "$scratch" --wildcards '*.rb'
    else
        tar -xzf "$scratch/src.tar.gz" -C "$scratch" '*.rb'
    fi
    rm "$scratch/src.tar.gz"

    # Collapse top-level <repo>-<tag>/ wrapper
    local inner
    inner="$(find "$scratch" -mindepth 1 -maxdepth 1 -type d | head -1)"
    if [[ -n "$inner" ]]; then
        mv "$inner"/* "$scratch"/ 2>/dev/null || true
        # preserve dotfiles if any
        shopt -s dotglob nullglob
        mv "$inner"/* "$scratch"/ 2>/dev/null || true
        shopt -u dotglob nullglob
        rmdir "$inner" 2>/dev/null || true
    fi

    # Record provenance for debugging baselines
    printf 'repo: %s\ntag: %s\nfetched: %s\n' \
        "$repo" "$tag" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        > "$scratch/.corpus-provenance"

    local count
    count="$(find "$scratch" -name '*.rb' -type f | wc -l | tr -d ' ')"
    echo "[$name] extracted $count ruby files"

    touch "$scratch/.corpus-ready"
    mv "$scratch" "$dest"
    echo "[$name] ready at $dest"
}

main() {
    if [[ $# -lt 1 ]]; then
        echo "Usage: $0 <discourse|mastodon|all>" >&2
        exit 2
    fi

    case "$1" in
        discourse|mastodon) snapshot "$1" ;;
        all)                snapshot discourse; snapshot mastodon ;;
        *) echo "unknown corpus: $1" >&2; exit 2 ;;
    esac
}

main "$@"
