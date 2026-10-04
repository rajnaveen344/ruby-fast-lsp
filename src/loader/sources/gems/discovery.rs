//! Installed, Bundler, global, and navigation gem discovery through the selected runtime.

use super::discovery_cache::BundlerDiscoveryCache;
use super::lockfile::locked_version_for;
use super::GemDiscoveryRecord;
use super::GemDiscoveryStage;
use super::GemInfo;
use super::GemSource;
use super::IndexerGem;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

impl IndexerGem {
    pub(crate) fn discover_gems_blocking(&mut self) -> Result<usize> {
        debug!("Starting gem discovery process");
        let total_started = Instant::now();

        self.discovered_gems.clear();
        self.locked_gems.clear();
        self.discovery_stage = GemDiscoveryStage::NotStarted;

        if self.ruby_executable.is_none() {
            info!(
                "No exact Ruby runtime selected for {:?}; gem dependencies remain unavailable",
                self.workspace_root
            );
            self.discovery_stage = GemDiscoveryStage::Complete;
            return Ok(0);
        }

        if self
            .workspace_root
            .as_ref()
            .is_some_and(|root| !root.join("Gemfile").is_file())
        {
            if self.explicitly_included_gems.is_empty() {
                info!(
                    "No Gemfile at Ruby project root; skipping gem discovery for standalone project"
                );
                self.discovery_stage = GemDiscoveryStage::Complete;
                return Ok(0);
            }
            self.discover_global_gems()?;
            self.resolve_gem_lib_paths();
            info!(
                "[PERF][gem discovery] project={} total={:?} source=explicit-global unique_gems={}",
                self.workspace_root
                    .as_deref()
                    .map(Path::display)
                    .map(|path| path.to_string())
                    .unwrap_or_else(|| "<none>".to_string()),
                total_started.elapsed(),
                self.discovered_gems.len()
            );
            self.discovery_stage = GemDiscoveryStage::Complete;
            return Ok(self.discovered_gems.len());
        }

        let lockfile_started = Instant::now();
        self.load_locked_gems()?;
        let lockfile_wall = lockfile_started.elapsed();
        let installed_started = Instant::now();
        self.discover_auto_gems()?;
        let installed_wall = installed_started.elapsed();
        let git_started = Instant::now();
        self.discover_cached_git_gems()?;
        let git_wall = git_started.elapsed();
        let archives_started = Instant::now();
        self.discover_cached_gem_archives()?;
        let archives_wall = archives_started.elapsed();
        let resolution_started = Instant::now();
        self.resolve_gem_lib_paths();
        let resolution_wall = resolution_started.elapsed();

        info!(
            "[PERF][gem discovery] project={} total={:?} lockfile={:?} \
             installed={:?} vendor_git={:?} vendor_archives={:?} \
             resolve_paths={:?} unique_gems={}",
            self.workspace_root
                .as_deref()
                .map(Path::display)
                .map(|path| path.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            total_started.elapsed(),
            lockfile_wall,
            installed_wall,
            git_wall,
            archives_wall,
            resolution_wall,
            self.discovered_gems.len()
        );
        self.discovery_stage = GemDiscoveryStage::Complete;
        Ok(self.discovered_gems.len())
    }

    pub(crate) fn discover_navigation_gems_blocking(
        &mut self,
        priority_keys: &HashSet<String>,
    ) -> Result<usize> {
        invariant!(
            !priority_keys.is_empty(),
            what = "navigation gem discovery received no active-document keys",
            why = "an empty frontier cannot identify bounded dependency work",
            fix = "use complete discovery when no active dependency key exists",
        );
        invariant!(
            self.workspace_root
                .as_ref()
                .is_some_and(|root| root.join("Gemfile").is_file()),
            what = "navigation gem discovery has no owning-project Gemfile",
            why = "exact locked source precedence exists only for a Ruby project",
            fix = "keep standalone explicit-global discovery on the complete discovery path",
        );
        let total_started = Instant::now();
        self.discovered_gems.clear();
        self.locked_gems.clear();
        self.discovery_stage = GemDiscoveryStage::NotStarted;

        if self.ruby_executable.is_none() {
            info!(
                "No exact Ruby runtime selected for {:?}; gem dependencies remain unavailable",
                self.workspace_root
            );
            self.discovery_stage = GemDiscoveryStage::NavigationInputs;
            return Ok(0);
        }

        let lockfile_started = Instant::now();
        self.load_locked_gems()?;
        let lockfile_wall = lockfile_started.elapsed();
        let installed_started = Instant::now();
        self.discover_auto_gems()?;
        let installed_wall = installed_started.elapsed();
        let git_started = Instant::now();
        self.discover_cached_git_gems()?;
        let git_wall = git_started.elapsed();
        let archives_started = Instant::now();
        self.discover_priority_cached_gem_archives(priority_keys)?;
        let archives_wall = archives_started.elapsed();
        let resolution_started = Instant::now();
        self.resolve_gem_lib_paths();
        let resolution_wall = resolution_started.elapsed();
        self.discovery_stage = GemDiscoveryStage::NavigationInputs;

        info!(
            "[PERF][priority gem discovery] project={} total={:?} lockfile={:?} \
             installed={:?} vendor_git={:?} priority_vendor_archives={:?} \
             resolve_paths={:?} unique_gems={}",
            self.workspace_root
                .as_deref()
                .map(Path::display)
                .map(|path| path.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            total_started.elapsed(),
            lockfile_wall,
            installed_wall,
            git_wall,
            archives_wall,
            resolution_wall,
            self.discovered_gems.len()
        );
        Ok(self.discovered_gems.len())
    }

    pub(crate) fn complete_navigation_gem_discovery_blocking(&mut self) -> Result<usize> {
        invariant_eq!(
            self.discovery_stage,
            GemDiscoveryStage::NavigationInputs,
            what = "exhaustive gem discovery did not follow the bounded navigation phase",
            why = "completing an uninitialized or already-complete catalog could hide stale candidates",
            fix = "call this exactly once after successful discover_navigation_gems_blocking",
        );
        let started = Instant::now();
        if self.ruby_executable.is_some() {
            self.discover_cached_gem_archives()?;
        }
        self.resolve_gem_lib_paths();
        self.discovery_stage = GemDiscoveryStage::Complete;
        info!(
            "[PERF][exhaustive vendor archive discovery] project={} total={:?} unique_gems={}",
            self.workspace_root
                .as_deref()
                .map(Path::display)
                .map(|path| path.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            started.elapsed(),
            self.discovered_gems.len()
        );
        Ok(self.discovered_gems.len())
    }

    pub(crate) fn priority_locked_gem_names(&self, priority_keys: &HashSet<String>) -> Vec<String> {
        let mut names = self
            .locked_gems
            .keys()
            .filter(|name| {
                priority_keys.contains(&crate::loader::coordinator::dependency_priority_key(name))
            })
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub(crate) fn needs_unlocked_explicit_discovery(&self) -> bool {
        self.ruby_executable.is_some()
            && self.discovered_gems.is_empty()
            && !self.explicitly_included_gems.is_empty()
            && self
                .workspace_root
                .as_ref()
                .is_some_and(|root| !root.join("Gemfile").is_file())
    }

    /// Discover the exact Bundler environment when available, otherwise the
    /// selected runtime's global RubyGems installation. Both branches execute
    /// in one selected-runtime process so a missing Bundler does not pay a
    /// second runtime startup before fallback.
    pub(super) fn discover_auto_gems(&mut self) -> Result<()> {
        let gemfile = self.find_gemfile()?;
        let saved = self.bundler_discovery_cache(&gemfile);
        if let Some(gems) = saved.as_ref().and_then(BundlerDiscoveryCache::load) {
            debug!("Reusing saved Bundler gems for unchanged inputs");
            return self.process_gem_json(&gems, "Bundler", GemSource::BundlerInstalled);
        }
        let script = r#"
            require 'json'
            project = lambda do |specs|
              specs.map do |spec|
                next if spec.name.nil? || spec.version.nil?
                {
                  name: spec.name,
                  version: spec.version.to_s,
                  platform: spec.platform.to_s,
                  gem_dir: spec.gem_dir,
                  lib_dirs: spec.require_paths.map { |p| File.join(spec.gem_dir, p) },
                  dependencies: spec.runtime_dependencies.map(&:name),
                  default_gem: spec.default_gem?
                }
              end.compact
            end
            begin
              require 'bundler'
              Bundler.root
              gems = project.call(Bundler.load.specs)
              source = 'bundler'
            rescue Exception
              require 'rubygems'
              gems = project.call(Gem::Specification.to_a)
              source = 'global'
            end
            puts "RUBY_FAST_LSP_GEM_DISCOVERY=#{JSON.generate({ source: source, gems: gems })}"
        "#;
        let output = self
            .ruby_command()?
            .env("BUNDLE_GEMFILE", &gemfile)
            .args(["-e", script])
            .output()
            .map_err(|error| anyhow!("Failed to execute automatic gem discovery: {error}"))?;
        if !output.status.success() {
            return Err(anyhow!(
                "Bundler and global gem discovery failed in the selected runtime: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        const DISCOVERY_PREFIX: &[u8] = b"RUBY_FAST_LSP_GEM_DISCOVERY=";
        let payload = output
            .stdout
            .split(|byte| *byte == b'\n')
            .rev()
            .find_map(|line| line.strip_prefix(DISCOVERY_PREFIX))
            .ok_or_else(|| {
                anyhow!(
                    "automatic gem discovery returned no framed result (stdout={:?}, stderr={:?})",
                    String::from_utf8_lossy(&output.stdout)
                        .chars()
                        .take(512)
                        .collect::<String>(),
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(512)
                        .collect::<String>()
                )
            })?;
        let value = serde_json::from_slice::<serde_json::Value>(payload)
            .context("automatic gem discovery returned invalid framed JSON")?;
        let source = value
            .get("source")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("automatic gem discovery JSON has no string `source`"))?;
        let gems = value
            .get("gems")
            .ok_or_else(|| anyhow!("automatic gem discovery JSON has no `gems` array"))?;
        let encoded = serde_json::to_vec(gems)
            .context("automatic gem discovery gem array could not be re-encoded")?;
        match source {
            "bundler" => {
                debug!("Using Bundler gems from Gemfile");
                if let Some(saved) = &saved {
                    if let Err(error) = saved.store(&encoded) {
                        warn!("Bundler discovery result was not saved for reuse: {error:#}");
                    }
                }
                self.process_gem_json(&encoded, "Bundler", GemSource::BundlerInstalled)
            }
            "global" => {
                debug!("Bundler unavailable; using selected runtime global gems");
                self.process_gem_json(&encoded, "Global", GemSource::GlobalInstalled)
            }
            other => Err(anyhow!(
                "automatic gem discovery returned unknown source `{other}`"
            )),
        }
    }

    /// The saved-result slot for this project, when reuse is enabled and the
    /// project's inputs can be read.
    fn bundler_discovery_cache(&self, gemfile: &Path) -> Option<BundlerDiscoveryCache> {
        let cache_root = self.discovery_cache_root.as_deref()?;
        let project_root = self.workspace_root.as_deref()?;
        let ruby_executable = self.ruby_executable.as_deref()?;
        BundlerDiscoveryCache::open(
            cache_root,
            project_root,
            gemfile,
            ruby_executable,
            self.java_home.as_deref(),
        )
        .unwrap_or_else(|error| {
            warn!("Bundler discovery reuse is unavailable for this project: {error:#}");
            None
        })
    }

    /// Discover all global gems
    fn discover_global_gems(&mut self) -> Result<()> {
        let script = r#"
            require 'rubygems'
            require 'json'
            gems = Gem::Specification.map do |spec|
              next if spec.name.nil? || spec.version.nil?
              {
                name: spec.name,
                version: spec.version.to_s,
                platform: spec.platform.to_s,
                gem_dir: spec.gem_dir,
                lib_dirs: spec.require_paths.map { |p| File.join(spec.gem_dir, p) },
                dependencies: spec.runtime_dependencies.map(&:name),
                default_gem: spec.default_gem?
              }
            end.compact
            puts JSON.generate(gems)
        "#;

        let output = self
            .ruby_command()?
            .args(["-e", script])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby gem discovery: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby gem discovery failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.process_gem_json(&output.stdout, "Global", GemSource::GlobalInstalled)
    }

    fn ruby_command(&self) -> Result<Command> {
        let executable = self.ruby_executable.as_ref().ok_or_else(|| {
            anyhow!(
                "Gem discovery requires an exact Ruby runtime for {:?}",
                self.workspace_root
            )
        })?;
        let mut command = Command::new(executable);
        command.env_remove("GEM_HOME");
        command.env_remove("GEM_PATH");
        command.env_remove("RUBY_VERSION");
        if let Some(java_home) = &self.java_home {
            command.env("JAVA_HOME", java_home);
        }
        if let Some(root) = &self.workspace_root {
            command.current_dir(root);
        }
        Ok(command)
    }

    /// Process gem data from JSON output
    fn process_gem_json(
        &mut self,
        data: &[u8],
        source_label: &str,
        source: GemSource,
    ) -> Result<()> {
        let gems: Vec<GemDiscoveryRecord> =
            serde_json::from_slice(data).context("failed to parse gem discovery JSON")?;

        for gem in gems {
            if gem.name.is_empty()
                || gem.version.is_empty()
                || gem.platform.is_empty()
                || gem.gem_dir.as_os_str().is_empty()
            {
                return Err(anyhow!(
                    "{source_label} gem discovery returned an incomplete identity: {gem:?}"
                ));
            }
            let locked_version = locked_version_for(&gem.version, &gem.platform);

            let gem_info = GemInfo {
                name: gem.name.clone(),
                version: gem.version,
                platform: gem.platform,
                locked_version,
                source,
                path: gem.gem_dir,
                lib_paths: gem.lib_dirs,
                dependencies: gem.dependencies,
                is_default: gem.default_gem,
            };

            self.discovered_gems
                .entry(gem.name)
                .or_default()
                .push(gem_info);
        }

        debug!(
            "Processed {} gems from {} source",
            self.discovered_gems.len(),
            source_label
        );
        Ok(())
    }

    /// Resolve and validate gem library paths
    fn resolve_gem_lib_paths(&mut self) {
        for versions in self.discovered_gems.values_mut() {
            for gem in versions.iter_mut() {
                // Filter out non-existent lib paths
                gem.lib_paths.retain(|p| p.exists() && p.is_dir());

                // Try default lib path if none exist
                if gem.lib_paths.is_empty() {
                    let default_lib = gem.path.join("lib");
                    if default_lib.exists() && default_lib.is_dir() {
                        gem.lib_paths.push(default_lib);
                    }
                }
            }
        }
    }
}
