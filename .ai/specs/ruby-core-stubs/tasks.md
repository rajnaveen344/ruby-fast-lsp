# Ruby Core Class Stubs Implementation Tasks

## Overview

This document outlines the implementation tasks for adding pre-packaged Ruby core class stubs to the Ruby Fast LSP. The system uses a maintained version array and build-time generation to provide offline, fast, and accurate core class completion.

## 🎯 Key Approach
- **Pre-packaged Stubs**: All stubs generated at build-time and included in extension
- **Version Array**: Maintained `SUPPORTED_RUBY_VERSIONS` array for supported versions
- **Core Classes Only**: Standard library excluded (available as actual Ruby files)
- **Offline Operation**: No runtime network dependencies

## Phase 1: Core Infrastructure 🏗️

### Task 1.1: Stub Data Structures
**Priority**: High | **Effort**: 3 days | **Dependencies**: None

#### Subtasks:
- [ ] Create `MinorVersion` struct with parsing and comparison
- [ ] Create `ClassStub` struct for core class representation  
- [ ] Create `MethodStub` and `ConstantStub` structs
- [ ] Add version constants array `SUPPORTED_RUBY_VERSIONS`
- [ ] Implement version matching and fallback logic

#### Acceptance Criteria:
- [ ] Parse Ruby versions (2.7.6 → MinorVersion{2, 7})
- [ ] Find closest supported version for fallback
- [ ] Serialize/deserialize stub data efficiently
- [ ] Handle all 15 supported Ruby versions (1.9-3.4)

#### Files to Create:
- `src/stubs/mod.rs`
- `src/stubs/types.rs` 
- `src/stubs/version.rs`

### Task 1.2: Stub Loader Module
**Priority**: High | **Effort**: 4 days | **Dependencies**: Task 1.1

#### Subtasks:
- [ ] Create `StubLoader` struct with version management
- [ ] Implement `load_version()` and `unload_version()`
- [ ] Implement `switch_version()` for runtime switching
- [ ] Add `find_closest_version()` for fallback logic
- [ ] Implement lazy loading for individual core classes
- [ ] Add compression/decompression for stub files

#### Acceptance Criteria:
- [ ] Load stubs from `vsix/stubs/{version}/core/` structure
- [ ] Memory usage < 30MB for loaded version
- [ ] Version switching < 500ms
- [ ] Graceful fallback for missing versions
- [ ] Thread-safe operations

#### Files to Create:
- `src/stubs/loader.rs`
- `src/stubs/compression.rs`

### Task 1.3: Package Structure Setup
**Priority**: Medium | **Effort**: 2 days | **Dependencies**: None

#### Subtasks:
- [ ] Create `vsix/stubs/` directory structure
- [ ] Add metadata.json template for version info
- [ ] Create sample stub files for testing (Ruby 3.0)
- [ ] Set up compression utilities for packaging

#### Acceptance Criteria:
- [ ] Directory structure matches design specification
- [ ] Sample stubs are syntactically valid Ruby
- [ ] Metadata includes version and generation info
- [ ] Compression reduces file size by >60%

#### Files to Create:
- `vsix/stubs/` (directory structure)
- `vsix/stubs/metadata.json`
- `vsix/stubs/3.0/core/object.rb` (sample)

## Phase 2: Build-Time Generation 🔨

### Task 2.1: Documentation Fetcher
**Priority**: High | **Effort**: 5 days | **Dependencies**: Task 1.1

#### Subtasks:
- [ ] Create `RubyDocFetcher` for ruby-doc.org API
- [ ] Implement RDoc parser for local Ruby installations
- [ ] Add Ruby source code parser as fallback
- [ ] Create documentation source priority system
- [ ] Add rate limiting and caching for API calls

#### Acceptance Criteria:
- [ ] Fetch class documentation from ruby-doc.org
- [ ] Parse RDoc from local Ruby installations
- [ ] Extract method signatures and documentation
- [ ] Handle network failures gracefully
- [ ] Cache responses to minimize API calls

#### Files to Create:
- `src/build/fetcher.rs`
- `src/build/rdoc_parser.rs`
- `src/build/source_parser.rs`

### Task 2.2: Stub Generator
**Priority**: High | **Effort**: 6 days | **Dependencies**: Task 2.1

#### Subtasks:
- [ ] Create `StubGenerator` for core class processing
- [ ] Implement method signature extraction
- [ ] Add constant and module extraction
- [ ] Generate proper Ruby syntax with documentation
- [ ] Handle version-specific features and differences
- [ ] Add quality validation for generated stubs

#### Acceptance Criteria:
- [ ] Generate syntactically valid Ruby stubs
- [ ] Include accurate method signatures
- [ ] Preserve documentation and RDoc links
- [ ] Handle version-specific differences
- [ ] Validate generated stub completeness

#### Files to Create:
- `src/build/generator.rs`
- `src/build/validator.rs`
- `src/build/templates.rs`

### Task 2.3: Build Pipeline
**Priority**: High | **Effort**: 4 days | **Dependencies**: Task 2.2

#### Subtasks:
- [ ] Create build script for stub generation
- [ ] Add CLI commands for generating specific versions
- [ ] Implement quality validation checks
- [ ] Add packaging and compression steps
- [ ] Integrate with existing build system

#### Acceptance Criteria:
- [ ] Generate stubs for all supported versions
- [ ] CLI supports individual version generation
- [ ] Quality checks prevent invalid stubs
- [ ] Integration with `cargo build` process
- [ ] Build time < 5 minutes for all versions

#### Files to Create:
- `src/build/cli.rs`
- `src/build/pipeline.rs`
- `build_stubs.sh`

## Phase 3: LSP Integration 🔌

### Task 3.1: Ruby Version Detection
**Priority**: High | **Effort**: 3 days | **Dependencies**: Task 1.2

#### Subtasks:
- [ ] Detect Ruby version from workspace
- [ ] Map detected version to supported minor version
- [ ] Handle multiple Ruby installations (rbenv, rvm, etc.)
- [ ] Add configuration for manual version override

#### Acceptance Criteria:
- [ ] Detect Ruby from `.ruby-version`, `Gemfile`, etc.
- [ ] Support major version managers
- [ ] Map patch versions to minor versions
- [ ] Allow manual override in settings

#### Files to Create:
- `src/ruby_version/detector.rs`
- `src/ruby_version/managers.rs`

### Task 3.2: Index Integration
**Priority**: High | **Effort**: 5 days | **Dependencies**: Task 3.1

#### Subtasks:
- [ ] Integrate `StubLoader` with existing `RubyIndex`
- [ ] Modify completion system to use core stubs
- [ ] Update definition lookup to include stub methods
- [ ] Add stub-based hover information
- [ ] Handle stub vs. user code priority

#### Acceptance Criteria:
- [ ] Core class methods appear in completion
- [ ] Go-to-definition works for core methods
- [ ] Hover shows core class documentation
- [ ] User code takes priority over stubs
- [ ] Performance impact < 10% on completion

#### Files to Modify:
- `src/indexer/index.rs`
- `src/capabilities/completion/`
- `src/capabilities/definitions/`

### Task 3.3: Performance Optimization
**Priority**: Medium | **Effort**: 3 days | **Dependencies**: Task 3.2

#### Subtasks:
- [ ] Implement memory-efficient loading
- [ ] Add caching for frequently accessed stubs
- [ ] Optimize startup time with lazy loading
- [ ] Monitor memory usage and implement limits

#### Acceptance Criteria:
- [ ] Startup time < 200ms with stubs
- [ ] Memory usage < 30MB for active version
- [ ] Lazy loading reduces initial memory
- [ ] Cache hit ratio > 90% for common classes

#### Files to Create:
- `src/stubs/cache.rs`
- `src/stubs/metrics.rs`

## Phase 4: Testing & Validation ✅

### Task 4.1: Unit Tests
**Priority**: High | **Effort**: 4 days | **Dependencies**: All previous

#### Subtasks:
- [ ] Test `MinorVersion` parsing and comparison
- [ ] Test `StubLoader` version management
- [ ] Test stub file loading and parsing
- [ ] Test version fallback logic
- [ ] Test build-time generation

#### Acceptance Criteria:
- [ ] >95% code coverage for stub system
- [ ] All edge cases covered
- [ ] Performance benchmarks included
- [ ] Mock data for testing

#### Files to Create:
- `tests/stubs/`
- `tests/build/`
- `tests/integration/`

### Task 4.2: Integration Tests
**Priority**: High | **Effort**: 3 days | **Dependencies**: Task 4.1

#### Subtasks:
- [ ] Test end-to-end completion with stubs
- [ ] Test version switching scenarios
- [ ] Test performance benchmarks
- [ ] Test error handling and recovery

#### Acceptance Criteria:
- [ ] Full completion workflow tested
- [ ] Version switching works correctly
- [ ] Performance meets requirements
- [ ] Error scenarios handled gracefully

### Task 4.3: Build Tests
**Priority**: Medium | **Effort**: 2 days | **Dependencies**: Task 2.3

#### Subtasks:
- [ ] Test stub generation for all versions
- [ ] Test documentation extraction accuracy
- [ ] Test package integrity and compression
- [ ] Test CI/CD pipeline execution

#### Acceptance Criteria:
- [ ] All versions generate successfully
- [ ] Generated stubs are valid Ruby
- [ ] Compression works correctly
- [ ] CI/CD integration functional

## Phase 5: Documentation & Deployment 📚

### Task 5.1: Documentation
**Priority**: Medium | **Effort**: 2 days | **Dependencies**: All previous

#### Subtasks:
- [ ] Update README with stub system overview
- [ ] Create developer guide for adding new versions
- [ ] Document build process and CI/CD setup
- [ ] Add troubleshooting guide

#### Acceptance Criteria:
- [ ] Clear user documentation
- [ ] Developer onboarding guide
- [ ] Build process documented
- [ ] Common issues covered

### Task 5.2: Deployment
**Priority**: High | **Effort**: 2 days | **Dependencies**: Task 5.1

#### Subtasks:
- [ ] Package stubs in extension bundle
- [ ] Test extension size and performance
- [ ] Create release process for stub updates
- [ ] Monitor production metrics

#### Acceptance Criteria:
- [ ] Extension size increase < 50MB
- [ ] Performance meets requirements
- [ ] Release process automated
- [ ] Monitoring in place

## Implementation Timeline

**Week 1**: Phase 1 (Core Infrastructure)
**Week 2**: Phase 2 (Build-Time Generation) 
**Week 3**: Phase 3 (LSP Integration)
**Week 4**: Phase 4 (Testing & Validation)
**Week 5**: Phase 5 (Documentation & Deployment)

## Success Metrics 🎯

- ✅ Extension startup < 200ms with stubs loaded
- ✅ Memory usage < 30MB for loaded stubs
- ✅ Version switching < 500ms
- ✅ 100% core class coverage for supported versions
- ✅ Accurate method signatures and documentation
- ✅ Seamless offline operation
- ✅ Extension size increase < 50MB

## Next Steps

1. **Start with Task 1.1** - Create the foundational data structures
2. **Set up development environment** for stub generation
3. **Create sample stubs** for Ruby 3.0 to validate approach
4. **Implement basic loader** to test integration points

Let's build this! 🚀