use ruby_analysis::core::SourceKind;
use ruby_fast_lsp_extension_api::{LockedGem, LockedGemSource, ProjectContext, ProjectSourceKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

const MAX_LOCKED_GEMS: usize = 4_096;
const MAX_GEM_NAME_BYTES: usize = 128;
const MAX_GEM_VERSION_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LockedGemSnapshot {
    pub lockfile_present: bool,
    pub complete: bool,
    pub gems: Vec<LockedGem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectContextSeed {
    pub project_uri: String,
    pub workspace_trusted: bool,
    pub ruby_version: Option<String>,
    snapshot: LockedGemSnapshot,
    applicability_fingerprint: ExtensionApplicabilityFingerprint,
    #[cfg(test)]
    applicability_fingerprint_computations: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExtensionApplicabilityFingerprint([u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectContextSnapshot {
    pub context: ProjectContext,
    pub applicability_fingerprint: ExtensionApplicabilityFingerprint,
}

impl ProjectContextSeed {
    pub fn detect(
        project_uri: String,
        project_root: &Path,
        workspace_trusted: bool,
        ruby_version: Option<String>,
    ) -> Self {
        let snapshot = locked_gem_snapshot(project_root);
        Self {
            project_uri,
            workspace_trusted,
            ruby_version,
            applicability_fingerprint: ExtensionApplicabilityFingerprint::from_snapshot(Some(
                &snapshot,
            )),
            snapshot,
            #[cfg(test)]
            applicability_fingerprint_computations: 1,
        }
    }

    pub fn refresh_dependencies(&mut self, project_root: &Path) {
        self.snapshot = locked_gem_snapshot(project_root);
        self.applicability_fingerprint =
            ExtensionApplicabilityFingerprint::from_snapshot(Some(&self.snapshot));
        #[cfg(test)]
        {
            self.applicability_fingerprint_computations += 1;
        }
    }

    #[cfg(test)]
    pub fn context(&self, source_uri: String, source_kind: SourceKind) -> ProjectContext {
        self.context_snapshot(source_uri, source_kind).context
    }

    pub fn context_snapshot(
        &self,
        source_uri: String,
        source_kind: SourceKind,
    ) -> ProjectContextSnapshot {
        ProjectContextSnapshot {
            context: ProjectContext {
                project_uri: self.project_uri.clone(),
                source_uri,
                source_kind: match source_kind {
                    SourceKind::Project => ProjectSourceKind::Project,
                    SourceKind::Gem => ProjectSourceKind::Gem,
                    SourceKind::Stdlib => ProjectSourceKind::Stdlib,
                    SourceKind::Stub => ProjectSourceKind::Stub,
                    SourceKind::Signature => ProjectSourceKind::Signature,
                    SourceKind::External => ProjectSourceKind::Signature,
                    SourceKind::Excluded => ProjectSourceKind::Excluded,
                },
                workspace_trusted: self.workspace_trusted,
                ruby_version: self.ruby_version.clone(),
                lockfile_present: self.snapshot.lockfile_present,
                locked_gems_complete: self.snapshot.complete,
                locked_gems: self.snapshot.gems.clone(),
            },
            applicability_fingerprint: self.applicability_fingerprint,
        }
    }

    #[cfg(test)]
    pub fn applicability_fingerprint(&self) -> ExtensionApplicabilityFingerprint {
        self.applicability_fingerprint
    }

    #[cfg(test)]
    fn applicability_fingerprint_computations(&self) -> u64 {
        self.applicability_fingerprint_computations
    }
}

impl ExtensionApplicabilityFingerprint {
    pub fn from_project_context(project: Option<&ProjectContext>) -> Self {
        Self::from_parts(project.map(|project| {
            (
                project.lockfile_present,
                project.locked_gems_complete,
                project.locked_gems.as_slice(),
            )
        }))
    }

    fn from_snapshot(snapshot: Option<&LockedGemSnapshot>) -> Self {
        Self::from_parts(snapshot.map(|snapshot| {
            (
                snapshot.lockfile_present,
                snapshot.complete,
                snapshot.gems.as_slice(),
            )
        }))
    }

    fn from_parts(parts: Option<(bool, bool, &[LockedGem])>) -> Self {
        #[derive(Serialize)]
        struct ApplicabilityInput<'a> {
            lockfile_present: bool,
            locked_gems_complete: bool,
            locked_gems: &'a [LockedGem],
        }

        let input =
            parts.map(
                |(lockfile_present, locked_gems_complete, locked_gems)| ApplicabilityInput {
                    lockfile_present,
                    locked_gems_complete,
                    locked_gems,
                },
            );
        let encoded = serde_json::to_vec(&input).expect(
            "INVARIANT VIOLATED: extension applicability input failed serialization. This is a host ABI bug because locked gem applicability types derive Serialize. Fix: keep the semantic seed fingerprint limited to the exact fields used by extension applicability.",
        );
        Self(Sha256::digest(encoded).into())
    }
}

pub(crate) fn locked_gem_snapshot(project_root: &Path) -> LockedGemSnapshot {
    let lockfile = project_root.join("Gemfile.lock");
    let Ok(content) = std::fs::read_to_string(&lockfile) else {
        return LockedGemSnapshot {
            lockfile_present: lockfile.exists(),
            complete: false,
            gems: Vec::new(),
        };
    };
    parse_lockfile(&content)
}

fn parse_lockfile(content: &str) -> LockedGemSnapshot {
    let mut source = None;
    let mut inside_specs = false;
    let mut complete = true;
    let mut gems = BTreeSet::new();

    for line in content.lines() {
        if !line.starts_with(' ') {
            source = match line {
                "GEM" => Some(LockedGemSource::Registry),
                "GIT" => Some(LockedGemSource::Git),
                "PATH" => Some(LockedGemSource::Path),
                _ => None,
            };
            inside_specs = false;
            continue;
        }
        if source.is_none() {
            continue;
        }
        if line == "  specs:" {
            inside_specs = true;
            continue;
        }
        if !inside_specs || !line.starts_with("    ") || line.starts_with("      ") {
            continue;
        }

        let spec = line.trim();
        let Some((name, version)) = spec.split_once(" (") else {
            complete = false;
            continue;
        };
        let Some(version) = version.strip_suffix(')') else {
            complete = false;
            continue;
        };
        if name.is_empty()
            || version.is_empty()
            || name.len() > MAX_GEM_NAME_BYTES
            || version.len() > MAX_GEM_VERSION_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            complete = false;
            continue;
        }
        if gems.len() == MAX_LOCKED_GEMS {
            complete = false;
            continue;
        }
        gems.insert(LockedGem {
            name: name.to_string(),
            version: version.to_string(),
            source: source.expect(
                "INVARIANT VIOLATED: Bundler spec lost its source section. This is a parser bug because specs are accepted only while a source is active. Fix: keep section and spec parsing in one state machine.",
            ),
        });
    }

    LockedGemSnapshot {
        lockfile_present: true,
        complete,
        gems: gems.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_registry_git_and_path_specs_deterministically() {
        let snapshot = parse_lockfile(
            r#"GIT
  remote: https://example.test/acme.git
  revision: abcdef123456
  specs:
    acme-kit (1.2.3)
      rack

PATH
  remote: components/local
  specs:
    local-kit (0.4.0)

GEM
  remote: https://rubygems.org/
  specs:
    rack (3.1.0)
      base64
    rspec-core (3.13.1)
"#,
        );

        assert!(snapshot.lockfile_present);
        assert!(snapshot.complete);
        assert_eq!(
            snapshot.gems,
            vec![
                LockedGem {
                    name: "acme-kit".to_string(),
                    version: "1.2.3".to_string(),
                    source: LockedGemSource::Git,
                },
                LockedGem {
                    name: "local-kit".to_string(),
                    version: "0.4.0".to_string(),
                    source: LockedGemSource::Path,
                },
                LockedGem {
                    name: "rack".to_string(),
                    version: "3.1.0".to_string(),
                    source: LockedGemSource::Registry,
                },
                LockedGem {
                    name: "rspec-core".to_string(),
                    version: "3.13.1".to_string(),
                    source: LockedGemSource::Registry,
                },
            ]
        );
    }

    #[test]
    fn malformed_spec_marks_snapshot_incomplete_without_guessing() {
        let snapshot = parse_lockfile("GEM\n  specs:\n    broken\n    rack (3.1.0)\n");
        assert!(!snapshot.complete);
        assert_eq!(snapshot.gems.len(), 1);
    }

    #[test]
    fn isolated_project_seeds_keep_locked_versions_and_source_kinds_separate() {
        let first = TempDir::new().expect("first project temp directory must be created");
        let second = TempDir::new().expect("second project temp directory must be created");
        std::fs::write(
            first.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    rspec-core (3.12.0)\n",
        )
        .expect("first lockfile must be written");
        std::fs::write(
            second.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    rspec-core (3.13.1)\n",
        )
        .expect("second lockfile must be written");

        let first_seed = ProjectContextSeed::detect(
            "file:///umbrella/first".to_string(),
            first.path(),
            true,
            Some("3.2".to_string()),
        );
        let second_seed = ProjectContextSeed::detect(
            "file:///umbrella/second".to_string(),
            second.path(),
            false,
            Some("3.3".to_string()),
        );
        let first_context = first_seed.context(
            "file:///umbrella/first/spec/a.rb".to_string(),
            SourceKind::Project,
        );
        let second_context = second_seed.context(
            "file:///gems/rspec-core/lib/rspec.rb".to_string(),
            SourceKind::Gem,
        );

        assert_eq!(first_context.project_uri, "file:///umbrella/first");
        assert_eq!(first_context.source_kind, ProjectSourceKind::Project);
        assert!(first_context.workspace_trusted);
        assert_eq!(first_context.locked_gems[0].version, "3.12.0");
        assert_eq!(second_context.project_uri, "file:///umbrella/second");
        assert_eq!(second_context.source_kind, ProjectSourceKind::Gem);
        assert!(!second_context.workspace_trusted);
        assert_eq!(second_context.locked_gems[0].version, "3.13.1");
    }

    #[test]
    fn applicability_fingerprint_is_computed_once_per_dependency_snapshot() {
        let project = TempDir::new().expect("project temp directory must be created");
        std::fs::write(
            project.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    rspec-core (3.13.1)\n",
        )
        .expect("initial lockfile must be written");

        let mut seed = ProjectContextSeed::detect(
            "file:///umbrella/app".to_string(),
            project.path(),
            true,
            Some("3.3".to_string()),
        );
        let initial_fingerprint = seed.applicability_fingerprint();
        assert_eq!(seed.applicability_fingerprint_computations(), 1);

        for source_uri in [
            "file:///umbrella/app/lib/a.rb",
            "file:///umbrella/app/lib/b.rb",
            "file:///umbrella/app/spec/a_spec.rb",
        ] {
            let snapshot = seed.context_snapshot(source_uri.to_string(), SourceKind::Project);
            assert_eq!(snapshot.applicability_fingerprint, initial_fingerprint);
        }
        assert_eq!(
            seed.applicability_fingerprint_computations(),
            1,
            "per-file contexts must reuse the dependency snapshot fingerprint"
        );

        std::fs::write(
            project.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    rspec-core (3.14.0)\n",
        )
        .expect("updated lockfile must be written");
        seed.refresh_dependencies(project.path());

        assert_ne!(seed.applicability_fingerprint(), initial_fingerprint);
        assert_eq!(
            seed.applicability_fingerprint_computations(),
            2,
            "one dependency refresh must compute exactly one replacement fingerprint"
        );
    }
}
