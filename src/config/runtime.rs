use crate::runtime::catalog::{DiscoveredRuntime, RuntimeDiscoverySource, RuntimeImplementation};
use ruby_fast_lsp_jruby_support::JrubyRuntimeIdentity;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuntimeSelectionConfig {
    pub mode: RuntimeMode,
    pub projects: Vec<ProjectRuntimeSelection>,
}

impl Default for RuntimeSelectionConfig {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::Auto,
            projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    #[default]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeSelection {
    pub root: String,
    pub selection: RuntimeSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuntimeSelection {
    Mode(RuntimeSelectionMode),
    Explicit(SelectedRuntimeDescriptor),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeSelectionMode {
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedRuntimeDescriptor {
    pub implementation: RuntimeImplementation,
    pub family: String,
    pub engine_version: String,
    pub compatibility_version: String,
    pub executable: PathBuf,
    pub discovery_source: RuntimeDiscoverySource,
    pub java_home: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveRuntimeSelection {
    Auto,
    Explicit(SelectedRuntimeDescriptor),
    LegacyMriCompatibility { major: u16, minor: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeConfigError {
    DuplicateProjectRoot(String),
    EmptyProjectRoot,
    InvalidEngineVersion(String),
    InvalidCompatibilityVersion(String),
    CompatibilityMismatch {
        engine: String,
        compatibility: String,
    },
    InvalidExecutable(PathBuf),
    InvalidJavaHome(PathBuf),
}

impl RuntimeSelectionConfig {
    pub fn validate(&self) -> Result<(), RuntimeConfigError> {
        let mut roots = HashSet::with_capacity(self.projects.len());
        for project in &self.projects {
            if project.root.trim().is_empty() {
                return Err(RuntimeConfigError::EmptyProjectRoot);
            }
            if !roots.insert(project.root.clone()) {
                return Err(RuntimeConfigError::DuplicateProjectRoot(
                    project.root.clone(),
                ));
            }
            if let RuntimeSelection::Explicit(runtime) = &project.selection {
                runtime.validate()?;
            }
        }
        Ok(())
    }

    pub fn selection_for_project(
        &self,
        project_root: &str,
        legacy_ruby_version: &str,
    ) -> EffectiveRuntimeSelection {
        if let Some(project) = self
            .projects
            .iter()
            .find(|project| same_project_root(&project.root, project_root))
        {
            return match &project.selection {
                RuntimeSelection::Mode(RuntimeSelectionMode::Auto) => {
                    EffectiveRuntimeSelection::Auto
                }
                RuntimeSelection::Explicit(runtime) => {
                    EffectiveRuntimeSelection::Explicit(runtime.clone())
                }
            };
        }
        if legacy_ruby_version == "auto" {
            return EffectiveRuntimeSelection::Auto;
        }
        let Some((major, minor)) = parse_family(legacy_ruby_version) else {
            return EffectiveRuntimeSelection::Auto;
        };
        EffectiveRuntimeSelection::LegacyMriCompatibility { major, minor }
    }
}

impl SelectedRuntimeDescriptor {
    pub fn validate(&self) -> Result<(), RuntimeConfigError> {
        if !self.executable.is_absolute() || self.executable.as_os_str().is_empty() {
            return Err(RuntimeConfigError::InvalidExecutable(
                self.executable.clone(),
            ));
        }
        if self
            .java_home
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(RuntimeConfigError::InvalidJavaHome(
                self.java_home
                    .clone()
                    .expect("INVARIANT VIOLATED: checked Java home must exist"),
            ));
        }
        let compatibility = parse_family(&self.compatibility_version).ok_or_else(|| {
            RuntimeConfigError::InvalidCompatibilityVersion(self.compatibility_version.clone())
        })?;
        match self.implementation {
            RuntimeImplementation::Jruby => {
                let identity = JrubyRuntimeIdentity::from_identifier(&self.engine_version)
                    .map_err(|_| {
                        RuntimeConfigError::InvalidEngineVersion(self.engine_version.clone())
                    })?;
                if self.family != identity.series.overlay_name()
                    || compatibility
                        != (
                            identity.ruby_compatibility.major,
                            identity.ruby_compatibility.minor,
                        )
                {
                    return Err(RuntimeConfigError::CompatibilityMismatch {
                        engine: self.engine_version.clone(),
                        compatibility: self.compatibility_version.clone(),
                    });
                }
            }
            RuntimeImplementation::Mri => {
                let engine_family = parse_family(&self.engine_version).ok_or_else(|| {
                    RuntimeConfigError::InvalidEngineVersion(self.engine_version.clone())
                })?;
                if self.family != self.compatibility_version || engine_family != compatibility {
                    return Err(RuntimeConfigError::CompatibilityMismatch {
                        engine: self.engine_version.clone(),
                        compatibility: self.compatibility_version.clone(),
                    });
                }
            }
            RuntimeImplementation::Truffleruby => {
                if parse_family(&self.engine_version).is_none() {
                    return Err(RuntimeConfigError::InvalidEngineVersion(
                        self.engine_version.clone(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl From<DiscoveredRuntime> for SelectedRuntimeDescriptor {
    fn from(runtime: DiscoveredRuntime) -> Self {
        Self {
            implementation: runtime.implementation,
            family: runtime.family,
            engine_version: runtime.engine_version,
            compatibility_version: runtime.compatibility_version,
            executable: runtime.executable,
            discovery_source: runtime.discovery_source,
            java_home: runtime.java_home,
        }
    }
}

fn parse_family(source: &str) -> Option<(u16, u16)> {
    let mut components = source.split('.');
    let major = components.next()?.parse().ok()?;
    let minor = components.next()?.parse().ok()?;
    Some((major, minor))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JrubyConfig {
    pub mode: RuntimeMode,
    pub projects: Vec<ProjectJrubyConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectJrubyConfig {
    pub root: String,
    pub additional_classpath: Vec<String>,
    pub additional_sources: Vec<String>,
}

impl JrubyConfig {
    pub fn validate(&self) -> Result<(), JrubyConfigError> {
        let mut roots = HashSet::with_capacity(self.projects.len());
        for project in &self.projects {
            if project.root.trim().is_empty() || !roots.insert(project.root.clone()) {
                return Err(JrubyConfigError::InvalidProjectRoot(project.root.clone()));
            }
            for pattern in project
                .additional_classpath
                .iter()
                .chain(&project.additional_sources)
            {
                if !is_scoped_pattern(pattern) {
                    return Err(JrubyConfigError::InvalidProjectPattern(pattern.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn project_config(&self, project_root: &str) -> ProjectJrubyConfig {
        self.projects
            .iter()
            .find(|project| same_project_root(&project.root, project_root))
            .cloned()
            .unwrap_or_else(|| ProjectJrubyConfig {
                root: project_root.to_string(),
                additional_classpath: Vec::new(),
                additional_sources: Vec::new(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JrubyConfigError {
    InvalidProjectRoot(String),
    InvalidProjectPattern(String),
}

fn is_scoped_pattern(pattern: &str) -> bool {
    let path = Path::new(pattern);
    !pattern.is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn same_project_root(configured: &str, requested: &str) -> bool {
    Path::new(configured) == Path::new(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jruby_descriptor() -> SelectedRuntimeDescriptor {
        SelectedRuntimeDescriptor {
            implementation: RuntimeImplementation::Jruby,
            family: "9.2".to_string(),
            engine_version: "9.2.21.0".to_string(),
            compatibility_version: "2.5".to_string(),
            executable: PathBuf::from("/runtimes/jruby-9.2.21.0/bin/jruby"),
            discovery_source: RuntimeDiscoverySource::Rvm,
            java_home: Some(PathBuf::from("/jdks/17")),
        }
    }

    #[test]
    fn validates_exact_jruby_identity_and_rejects_crossed_compatibility() {
        assert_eq!(jruby_descriptor().validate(), Ok(()));
        let mut invalid = jruby_descriptor();
        invalid.compatibility_version = "3.1".to_string();
        assert!(matches!(
            invalid.validate(),
            Err(RuntimeConfigError::CompatibilityMismatch { .. })
        ));
    }

    #[test]
    fn routes_per_project_and_migrates_the_legacy_flat_setting_deterministically() {
        let config = RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: "admin".to_string(),
                selection: RuntimeSelection::Explicit(jruby_descriptor()),
            }],
        };
        assert_eq!(
            config.selection_for_project("admin", "3.3"),
            EffectiveRuntimeSelection::Explicit(jruby_descriptor())
        );
        assert_eq!(
            config.selection_for_project("admin/", "3.3"),
            EffectiveRuntimeSelection::Explicit(jruby_descriptor())
        );
        assert_eq!(
            config.selection_for_project("server", "3.3"),
            EffectiveRuntimeSelection::LegacyMriCompatibility { major: 3, minor: 3 }
        );
        assert_eq!(
            config.selection_for_project("server", "auto"),
            EffectiveRuntimeSelection::Auto
        );
    }

    #[test]
    fn rejects_duplicate_projects_and_unscoped_jruby_paths() {
        let runtime = RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![
                ProjectRuntimeSelection {
                    root: "admin".to_string(),
                    selection: RuntimeSelection::Mode(RuntimeSelectionMode::Auto),
                },
                ProjectRuntimeSelection {
                    root: "admin".to_string(),
                    selection: RuntimeSelection::Mode(RuntimeSelectionMode::Auto),
                },
            ],
        };
        assert_eq!(
            runtime.validate(),
            Err(RuntimeConfigError::DuplicateProjectRoot(
                "admin".to_string()
            ))
        );
        let jruby = JrubyConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectJrubyConfig {
                root: "admin".to_string(),
                additional_classpath: vec!["../shared/*.jar".to_string()],
                additional_sources: Vec::new(),
            }],
        };
        assert_eq!(
            jruby.validate(),
            Err(JrubyConfigError::InvalidProjectPattern(
                "../shared/*.jar".to_string()
            ))
        );
    }

    #[test]
    fn returns_only_the_owning_projects_jruby_paths() {
        let config = JrubyConfig {
            mode: RuntimeMode::Auto,
            projects: vec![
                ProjectJrubyConfig {
                    root: "/workspace/admin".to_string(),
                    additional_classpath: vec!["lib/admin.jar".to_string()],
                    additional_sources: Vec::new(),
                },
                ProjectJrubyConfig {
                    root: "/workspace/server".to_string(),
                    additional_classpath: vec!["lib/server.jar".to_string()],
                    additional_sources: vec!["java-src/**/*.java".to_string()],
                },
            ],
        };
        assert_eq!(
            config.project_config("/workspace/server/"),
            config.projects[1]
        );
        assert_eq!(
            config.project_config("/workspace/unknown"),
            ProjectJrubyConfig {
                root: "/workspace/unknown".to_string(),
                additional_classpath: Vec::new(),
                additional_sources: Vec::new(),
            }
        );
    }
}
