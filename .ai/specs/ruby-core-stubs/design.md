# Ruby Core Class Stubs Design Document

## Overview

This document outlines the technical design for implementing version-specific Ruby core class stubs in the Ruby Fast LSP. The system will provide comprehensive stub definitions for Ruby's built-in classes and modules across versions 1.9 through 3.5, enabling accurate completion, navigation, and analysis without requiring access to the actual Ruby implementation source code.

## Architecture

### High-Level Components

```
┌─────────────────────────────────────────────────────────────┐
│                    Ruby Fast LSP                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐  │
│  │   Extension     │  │   Ruby Index    │  │   LSP       │  │
│  │  Configuration  │  │   Integration   │  │ Capabilities│  │
│  └─────────────────┘  └─────────────────┘  └─────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐  │
│  │   Version       │  │   Stub Loader   │  │ Pre-packaged│  │
│  │   Detection     │  │   & Manager     │  │ Stub Files  │  │
│  └─────────────────┘  └─────────────────┘  └─────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐  │
│  │   Build-Time    │  │   Stub Parser   │  │   Core      │  │
│  │ Stub Generator  │  │   & Validator   │  │ Class DB    │  │
│  └─────────────────┘  └─────────────────┘  └─────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

1. **Version Detection**: Automatically detect installed Ruby versions and project-specific version requirements
2. **Pre-packaged Stub Files**: Version-specific stub files bundled with the extension
3. **Stub Loader & Manager**: Efficiently load and manage pre-packaged stub files based on detected version
4. **Ruby Index Integration**: Seamlessly integrate core class stubs into the existing indexing system
5. **Extension Configuration**: Provide user-friendly configuration options for version selection
6. **Build-Time Stub Generator**: Generate stub files during extension build process (not runtime)

## Detailed Design

### 1. Version Detection System

#### Ruby Version Manager Detection

```rust
pub struct RubyVersionDetector {
    managers: Vec<Box<dyn VersionManager>>,
}

pub trait VersionManager {
    fn name(&self) -> &str;
    fn detect_versions(&self) -> Result<Vec<RubyVersion>, VersionError>;
    fn get_current_version(&self, project_path: &Path) -> Option<RubyVersion>;
}

pub struct RbenvManager;
pub struct RvmManager;
pub struct ChrubyManager;
pub struct AsdfManager;
pub struct MiseManager;
```

#### Version Detection Algorithm

1. **Project-Specific Detection**:
   - Check for `.ruby-version` file in project root and parent directories
   - Parse `Gemfile` for `ruby` version specification
   - Check for `.rvmrc`, `.rbenv-version`, `.tool-versions` files

2. **System-Wide Detection**:
   - Scan `~/.rbenv/versions/` for rbenv installations
   - Scan `~/.rvm/rubies/` for RVM installations
   - Check `~/.rubies/` for chruby installations
   - Query `asdf list ruby` for asdf installations
   - Query `mise list ruby` for mise installations

3. **Fallback Detection**:
   - Use `ruby --version` for system Ruby
   - Default to Ruby 3.0 if all detection fails

#### Data Structures

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RubyVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
    pub pre: Option<String>, // For preview/rc versions
    pub source: VersionSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionSource {
    Rbenv(PathBuf),
    Rvm(PathBuf),
    Chruby(PathBuf),
    Asdf(PathBuf),
    Mise(PathBuf),
    System(PathBuf),
    Manual(String),
}
```

### 2. Build-Time Stub Generation System

#### Ruby Documentation Sources and Fetcher (Build-Time Only)

```rust
// This runs during extension build, not at runtime
pub struct BuildTimeRubyDocFetcher {
    client: reqwest::Client,
    output_dir: PathBuf,
    rdoc_parser: RDocParser,
}

impl BuildTimeRubyDocFetcher {
    pub async fn fetch_from_ruby_doc_org(&self, version: &RubyVersion, class_name: &str) -> Result<ClassInfo, FetchError>;
    pub async fn fetch_from_rdoc(&self, version: &RubyVersion, class_name: &str) -> Result<ClassInfo, FetchError>;
    pub async fn fetch_version_index(&self, version: &RubyVersion) -> Result<VersionIndex, FetchError>;
    pub fn save_to_package(&self, version: &RubyVersion, stubs: &VersionStubs) -> Result<(), IoError>;
}

// RDoc integration for local Ruby installations
pub struct RDocParser {
    rdoc_cache: HashMap<RubyVersion, PathBuf>,
}

impl RDocParser {
    pub fn parse_rdoc_files(&self, ruby_path: &Path) -> Result<Vec<ClassInfo>, RDocError>;
    pub fn extract_method_signatures(&self, rdoc_file: &Path) -> Result<Vec<MethodInfo>, RDocError>;
    pub fn parse_documentation(&self, rdoc_content: &str) -> Result<String, RDocError>;
}
```

#### Documentation Sources Priority

1. **Official Ruby Documentation** (ruby-doc.org) - Primary source
2. **Local RDoc** - Generated from Ruby source code
3. **Ruby Source Code** - Direct parsing as fallback
4. **Community Sources** - Ruby references, stdlib docs

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassInfo {
    pub name: String,
    pub superclass: Option<String>,
    pub included_modules: Vec<String>,
    pub methods: Vec<MethodInfo>,
    pub constants: Vec<ConstantInfo>,
    pub documentation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MethodInfo {
    pub name: String,
    pub signature: String,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
    pub documentation: Option<String>,
    pub visibility: Visibility,
    pub kind: MethodKind,
}
```

#### Build-Time Stub File Generation

```rust
// This runs during extension build, not at runtime
pub struct BuildTimeStubGenerator {
    fetcher: BuildTimeRubyDocFetcher,
    template_engine: TemplateEngine,
    output_dir: PathBuf,
}

impl BuildTimeStubGenerator {
    pub async fn generate_all_version_stubs(&self) -> Result<(), GenerationError>;
    pub async fn generate_version_stubs(&self, version: &RubyVersion) -> Result<VersionStubs, GenerationError>;
    pub fn generate_class_stub(&self, class_info: &ClassInfo) -> Result<String, GenerationError>;
    pub fn package_stubs(&self, version: &RubyVersion, stubs: &VersionStubs) -> Result<(), PackagingError>;
}
```

#### Comprehensive Ruby Version Coverage

**Supported Minor Versions**
- **Ruby 1.9**: 1.9.3 (latest patch)
- **Ruby 2.0**: 2.0.0 (latest patch)
- **Ruby 2.1**: 2.1.10 (latest patch)
- **Ruby 2.2**: 2.2.10 (latest patch)
- **Ruby 2.3**: 2.3.8 (latest patch)
- **Ruby 2.4**: 2.4.10 (latest patch)
- **Ruby 2.5**: 2.5.9 (latest patch)
- **Ruby 2.6**: 2.6.10 (latest patch)
- **Ruby 2.7**: 2.7.8 (latest patch)
- **Ruby 3.0**: 3.0.7 (latest patch)
- **Ruby 3.1**: 3.1.6 (latest patch)
- **Ruby 3.2**: 3.2.5 (latest patch)
- **Ruby 3.3**: 3.3.5 (latest patch)
- **Ruby 3.4**: 3.4.1 (latest patch)

#### Core Classes to Generate

Based on research, the following core classes will be included across versions:

**Universal Core Classes (All Versions)**:
- `BasicObject`, `Object`, `Class`, `Module`
- `Kernel`, `Comparable`, `Enumerable`
- `String`, `Symbol`, `Regexp`, `MatchData`
- `Numeric`, `Integer`, `Fixnum`, `Bignum`, `Float`, `Rational`, `Complex`
- `Array`, `Hash`, `Range`, `Enumerator`
- `Time`, `Date`, `DateTime`
- `IO`, `File`, `Dir`, `StringIO`
- `Thread`, `Fiber`, `Mutex`
- `Exception` hierarchy
- `Method`, `UnboundMethod`, `Proc`, `Binding`

**Version-Specific Additions**:
- Ruby 1.9: `Fixnum`, `Bignum` (separate classes), `Fiber` (lightweight concurrency)
- Ruby 2.0: `Refinement`, `BasicObject`
- Ruby 2.1: Enhanced `Rational` and `Complex`
- Ruby 2.4: Unified `Integer` (no separate `Fixnum`/`Bignum`)
- Ruby 2.7: `Warning` module enhancements
- Ruby 3.0: `Ractor`
- Ruby 3.2: `Data` class
- Ruby 3.3: Enhanced pattern matching classes

**Note**: Standard library modules (JSON, Net::HTTP, URI, etc.) are excluded as they exist as actual Ruby files on client machines and can be indexed directly by the LSP.

### 3. Pre-Packaged Stub Structure and Loading

#### Maintained Ruby Versions Array

The extension maintains a predefined array of supported Ruby minor versions:

```rust
const SUPPORTED_RUBY_VERSIONS: &[&str] = &[
    "1.9", "2.0", "2.1", "2.2", "2.3", "2.4", "2.5", 
    "2.6", "2.7", "3.0", "3.1", "3.2", "3.3", "3.4"
];
```

Each version represents the latest patch release for that minor version (e.g., 2.7.8 for Ruby 2.7).

#### Extension Package Structure

The extension includes pre-packaged core class stub files organized by Ruby minor versions:

```
vsix/stubs/
├── 1.9/
│   ├── metadata.json
│   └── core/
│       ├── object.rb
│       ├── string.rb
│       ├── array.rb
│       ├── hash.rb
│       ├── kernel.rb
│       ├── enumerable.rb
│       └── ...
├── 2.0/
│   ├── metadata.json
│   └── core/
│       ├── object.rb
│       ├── string.rb
│       ├── refinement.rb  # New in 2.0
│       └── ...
├── 2.7/
│   ├── metadata.json
│   └── core/
│       ├── object.rb
│       ├── string.rb
│       ├── pattern_matching.rb  # New in 2.7
│       └── ...
├── 3.0/
├── 3.1/
├── 3.2/
├── 3.3/
├── 3.4/
│   ├── metadata.json
│   └── core/
│       ├── object.rb
│       ├── string.rb
│       ├── data.rb  # New in 3.2
│       └── ...
└── version_index.json
```

**Note**: Standard library modules (JSON, Net::HTTP, URI, etc.) are excluded as they exist as actual Ruby files on client machines and can be indexed directly by the LSP.

#### Stub Loader Implementation

```rust
pub struct StubLoader {
    extension_dir: PathBuf,
    loaded_versions: HashMap<MinorVersion, VersionStubs>,
    max_loaded_versions: usize,
}

impl StubLoader {
    pub fn load_stubs(&mut self, version: &RubyVersion) -> Result<&VersionStubs, LoadError>;
    pub fn load_core_class(&mut self, version: &MinorVersion, class_name: &str) -> Result<ClassStub, LoadError>;
    pub fn load_stdlib_module(&mut self, version: &MinorVersion, module_path: &str) -> Result<ModuleStub, LoadError>;
    pub fn get_closest_minor_version(&self, requested: &RubyVersion) -> Option<MinorVersion>;
    pub fn unload_version(&mut self, version: &MinorVersion);
    pub fn list_available_versions(&self) -> Result<Vec<MinorVersion>, LoadError>;
    pub fn get_supported_versions() -> &'static [&'static str] {
        &SUPPORTED_RUBY_VERSIONS
    }
    pub fn find_closest_version(&self, target: MinorVersion) -> Option<MinorVersion>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MinorVersion {
    pub major: u8,
    pub minor: u8,
}

impl MinorVersion {
    pub fn from_ruby_version(version: &RubyVersion) -> Self {
        Self {
            major: version.major,
            minor: version.minor,
        }
    }
    
    pub fn matches(&self, version: &RubyVersion) -> bool {
        self.major == version.major && self.minor == version.minor
    }
}

pub struct VersionStubs {
    pub core_classes: HashMap<String, ClassStub>,
    pub metadata: PackageMetadata,
}

pub struct ClassStub {
    pub name: String,
    pub file_path: PathBuf,
    pub methods: Vec<MethodInfo>,
    pub constants: Vec<ConstantInfo>,
    pub documentation: Option<String>,
    pub rdoc_link: Option<String>,
}
```

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub version: RubyVersion,
    pub generated_at: SystemTime,
    pub ruby_doc_version: String,
    pub checksum: String,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}
```

### 4. RubyIndex Integration

#### Core Stub Loader

```rust
pub struct CoreStubLoader {
    stub_loader: StubLoader,
    version_detector: VersionDetector,
    current_version: Option<RubyVersion>,
}

impl CoreStubLoader {
    pub fn ensure_stubs_loaded(&mut self, version: &RubyVersion) -> Result<&VersionStubs, LoadError>;
    pub fn get_stubs_for_version(&mut self, version: &RubyVersion) -> Result<&VersionStubs, LoadError>;
    pub fn switch_version(&mut self, new_version: &RubyVersion) -> Result<(), LoadError>;
    pub fn convert_to_entries(&self, stubs: &VersionStubs) -> Vec<Entry>;
}
```

#### Index Integration Points

1. **Startup Integration**: Load core stubs during RubyIndex initialization
2. **Version Change Integration**: Reload stubs when Ruby version changes
3. **Entry Creation**: Convert stub data to `Entry` objects with appropriate metadata
4. **Namespace Handling**: Ensure core classes are properly namespaced and accessible

#### Modified RubyIndex Structure

```rust
impl RubyIndex {
    pub fn with_core_stubs(mut self, version: &RubyVersion) -> Result<Self, IndexError> {
        let core_entries = self.core_stub_loader.load_core_stubs(version)?;
        for entry in core_entries {
            self.add_core_entry(entry);
        }
        Ok(self)
    }
    
    fn add_core_entry(&mut self, entry: Entry) {
        // Mark as core entry for special handling
        let mut core_entry = entry;
        core_entry.metadata.insert("core".to_string(), "true".to_string());
        core_entry.metadata.insert("readonly".to_string(), "true".to_string());
        
        self.definitions.insert(core_entry.fully_qualified_name.clone(), core_entry);
    }
}
```

### 5. Extension Configuration

#### VS Code Settings Schema

```json
{
  "ruby.version": {
    "type": "string",
    "enum": ["auto", "1.9.3", "2.0.0", "2.1.0", ...],
    "default": "auto",
    "description": "Ruby version to use for core class stubs"
  },
  "ruby.autoDetectVersion": {
    "type": "boolean",
    "default": true,
    "description": "Automatically detect Ruby version from project files"
  },
  "ruby.coreStubsEnabled": {
    "type": "boolean",
    "default": true,
    "description": "Enable Ruby core class stubs for completion and navigation"
  },
  "ruby.stubCacheSize": {
    "type": "number",
    "default": 100,
    "description": "Maximum size of stub cache in MB"
  }
}
```

#### Configuration Manager

```rust
pub struct ConfigurationManager {
    settings: RubySettings,
    version_detector: RubyVersionDetector,
}

#[derive(Debug, Clone)]
pub struct RubySettings {
    pub version: VersionSetting,
    pub auto_detect_version: bool,
    pub core_stubs_enabled: bool,
    pub stub_cache_size: u64,
}

#[derive(Debug, Clone)]
pub enum VersionSetting {
    Auto,
    Manual(RubyVersion),
}
```

### 6. Performance Optimizations

#### Pre-Packaged Stub Benefits

1. **Instant Availability**: No generation time - stubs are immediately available
2. **Offline Operation**: No network dependencies during runtime
3. **Consistent Performance**: Predictable load times across all environments
4. **Reduced Startup Time**: No waiting for stub generation or downloads

#### Loading Strategy

1. **Lazy Loading**: Load version stubs only when needed
2. **Memory Management**: Keep only active version in memory, unload unused versions
3. **Compression**: Use gzip compression for packaged stub files (60-80% size reduction)
4. **Fast Decompression**: Optimized decompression for quick access

#### Memory Optimization

1. **Shared Data Structures**: Reuse common method signatures across versions
2. **String Interning**: Intern frequently used strings (method names, types)
3. **Lazy Deserialization**: Deserialize stub data only when accessed
4. **Version Switching**: Efficient switching between Ruby versions without full reload
5. **Memory Limits**: Configurable limits on number of loaded versions (default: 3)

#### Index Integration Optimizations

1. **Batch Insertion**: Add core entries to index in batches
2. **Deferred Resolution**: Resolve core class relationships after all entries are loaded
3. **Memory Pooling**: Reuse memory allocations for similar core class structures

## Build-Time Generation Process

### Version Array Management

The build process operates on the predefined `SUPPORTED_RUBY_VERSIONS` array:

1. **Adding New Versions**:
   - Add new minor version to the `SUPPORTED_RUBY_VERSIONS` array
   - Build process automatically generates stubs for the new version
   - No client-side updates required - stubs are pre-packaged

2. **Version Selection Logic**:
   - Client detects installed Ruby version (e.g., 2.7.6)
   - Maps to closest minor version in supported array (2.7)
   - Loads corresponding pre-packaged stubs from `vsix/stubs/2.7/core/`

### CI/CD Integration

1. **Automated Generation**: Run stub generation during extension build process
2. **Documentation Sources**: 
   - Primary: Official Ruby documentation (ruby-doc.org)
   - Secondary: RDoc from Ruby source repositories
   - Fallback: Ruby source code parsing
3. **Version Coverage**: Generate stubs for all supported minor versions (1.9 - 3.4)
4. **Quality Validation**: Validate generated stubs for completeness and accuracy
5. **Individual File Generation**: Create separate .rb files for each class/module
6. **Packaging**: Organize into minor version folders within extension bundle

### Build Pipeline

```bash
# Build-time stub generation for all versions
cargo run --bin generate-stubs --release -- --all-versions

# Generate specific version (for development)
cargo run --bin generate-stubs --release -- --version 3.4

# Generate with RDoc integration
cargo run --bin generate-stubs --release -- --use-rdoc --ruby-source-path /path/to/ruby

# Validates all generated stubs
cargo test --test stub_validation

# Validates specific version stubs
cargo test --test stub_validation -- --version 2.7

# Packages stubs into extension
npm run package-stubs
```

### RDoc Integration Process

1. **RDoc Discovery**: Locate RDoc files in Ruby installations
2. **Documentation Parsing**: Extract method signatures and documentation
3. **Cross-Reference**: Validate against official documentation
4. **Merge Strategy**: Combine multiple sources for comprehensive coverage

## Error Handling Strategy

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum CoreStubError {
    #[error("Version detection failed: {0}")]
    VersionDetection(#[from] VersionError),
    
    #[error("Stub generation failed: {0}")]
    StubGeneration(#[from] GenerationError),
    
    #[error("Cache operation failed: {0}")]
    Cache(#[from] CacheError),
    
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("Index integration failed: {0}")]
    IndexIntegration(String),
}
```

### Build-Time Errors

1. **Network Failures**: Retry with exponential backoff, fail build if critical
2. **Parsing Errors**: Log warnings, use fallback templates for malformed docs
3. **Validation Failures**: Fail build if generated stubs don't meet quality standards
4. **Resource Limits**: Optimize generation process for CI/CD constraints

### Runtime Errors

1. **Missing Stubs**: Fallback to closest available version or basic Ruby information
2. **Corrupted Package**: Validate checksums, report corruption to telemetry
3. **Version Detection Failures**: Use default Ruby version as fallback
4. **Loading Errors**: Graceful degradation of completion features with user notification

### Fallback Strategies

1. **Version Detection Failure**: Fall back to Ruby 3.0 stubs
2. **Network Failure**: Use cached stubs if available
3. **Generation Failure**: Continue with partial stubs
4. **Cache Corruption**: Regenerate stubs automatically

## Testing Strategy

### Build-Time Tests

1. **Stub Generation**: Validate generated stub accuracy against known Ruby documentation
2. **Documentation Parsing**: Test parsing of Ruby documentation from various sources
3. **Packaging**: Verify compression, checksums, and package integrity
4. **Version Coverage**: Ensure all supported Ruby versions are properly generated

### Unit Tests

1. **Version Detection**: Test all version manager integrations
2. **Stub Loading**: Test loading of pre-packaged stubs from extension bundle
3. **Memory Management**: Test loading/unloading of version stubs
4. **Index Integration**: Verify proper entry creation and integration

### Integration Tests

1. **End-to-End Workflows**: Test complete stub loading and usage
2. **Version Switching**: Verify behavior when changing Ruby versions
3. **Performance Tests**: Validate loading times and memory usage for pre-packaged stubs
4. **Error Scenarios**: Test graceful handling of missing or corrupted packages
5. **Offline Operation**: Verify functionality without network access

### Validation Tests

1. **Stub Accuracy**: Compare packaged stubs with actual Ruby behavior
2. **Version Compatibility**: Verify version-specific differences are correct
3. **Documentation Quality**: Ensure stub documentation is helpful and accurate
4. **Package Integrity**: Validate checksums and compression integrity

## Migration and Deployment

### Rollout Strategy

1. **Feature Flag**: Implement behind feature flag for gradual rollout
2. **Fallback Mode**: Maintain existing completion behavior as fallback
3. **User Feedback**: Collect telemetry on performance and accuracy
4. **Incremental Enablement**: Enable for specific Ruby versions first
5. **Extension Size**: Monitor impact on extension download size

### Backward Compatibility

1. **Existing Index**: Ensure compatibility with current RubyIndex implementation
2. **Configuration**: Maintain existing configuration options
3. **API Stability**: Preserve existing LSP completion behavior
4. **Performance**: Ensure no regression in completion performance
5. **Offline Support**: Graceful handling when pre-packaged stubs are unavailable

### Build and Release Process

1. **Automated Builds**: Integrate stub generation into CI/CD pipeline
2. **Quality Gates**: Validate stub quality before release
3. **Version Updates**: Process for updating stubs when new Ruby versions are released
4. **Extension Packaging**: Include compressed stubs in VSIX package
5. **Release Notes**: Document stub coverage and improvements

### Monitoring and Metrics

1. **Performance Metrics**: Track stub loading times and memory usage
2. **Accuracy Metrics**: Monitor completion accuracy improvements
3. **Error Rates**: Track stub loading failures and fallback usage
4. **User Adoption**: Monitor feature usage and user satisfaction
5. **Package Integrity**: Monitor for corrupted or missing stub packages

## Future Enhancements

### Potential Extensions

1. **RBS Integration**: Support for Ruby Signature files
2. **Custom Extensions**: Allow users to add custom core class extensions
3. **Real-time Updates**: Automatically update stubs when Ruby documentation changes
4. **Community Contributions**: Enable community-driven stub improvements
5. **Type Inference**: Use stubs to improve type inference accuracy
6. **Documentation Integration**: Provide inline documentation from stubs

### Advanced Features

1. **Gem Stubs**: Extend to popular gems (Rails, RSpec, etc.) with pre-packaged approach
2. **Custom Stubs**: Allow users to supplement pre-packaged stubs with custom definitions
3. **Incremental Updates**: Delta updates for stub packages between extension versions
4. **Documentation Integration**: Rich hover information from pre-packaged documentation

### Performance Improvements

1. **Streaming Decompression**: Stream decompression for faster loading of large stub files
2. **Selective Loading**: Load only specific classes/modules on demand
3. **Memory Optimization**: Further reduce memory footprint with better data structures
4. **Startup Optimization**: Preload metadata for instant version detection

### User Experience

1. **Configuration UI**: Visual interface for stub management and version selection
2. **Stub Browser**: Browse available core classes and methods by Ruby version
3. **Diagnostics**: Help users troubleshoot stub loading issues
4. **Version Recommendations**: Suggest optimal Ruby versions based on project analysis

### Build-Time Enhancements

1. **Automated Updates**: Automatically update stubs when new Ruby versions are released
2. **Quality Metrics**: Track and improve stub generation quality over time
3. **Differential Packaging**: Only include changed stubs in updates
4. **Multi-Source Generation**: Combine multiple documentation sources for better coverage