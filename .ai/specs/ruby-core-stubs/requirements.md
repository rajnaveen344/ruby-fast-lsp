# Ruby Core Class Stubs Requirements Document

## Introduction

This specification defines the implementation of version-specific Ruby core class stubs for the Ruby Fast LSP. Since the LSP operates without access to the actual Ruby implementation source code, this feature will provide comprehensive stub definitions for Ruby's core classes, modules, and methods across different Ruby minor versions (1.9 through 3.4, using latest patch for each minor). The stubs will be generated from official Ruby documentation and integrated into the indexing system to enable accurate completion, navigation, and analysis of Ruby core functionality. This enhancement will significantly improve developer experience by providing precise type information and method signatures for Ruby's built-in classes regardless of the Ruby version being used.

## Requirements

### Requirement 1: Ruby Version Detection and Management

**User Story:** As a Ruby developer using different Ruby versions across projects, I want the LSP to automatically detect and use the appropriate Ruby version so that I get accurate core class information for my specific Ruby environment.

#### Acceptance Criteria

1. WHEN the extension starts THEN it SHALL detect all installed Ruby versions on the system
2. WHEN multiple Ruby version managers are present (rbenv, RVM, chruby, asdf, mise) THEN the system SHALL detect versions from all managers
3. WHEN a project has a `.ruby-version` file THEN the system SHALL use that version for the project
4. WHEN a project has a `Gemfile` with ruby version THEN the system SHALL respect that version specification
5. WHEN no version is specified THEN the system SHALL use the system default Ruby version
6. WHEN the Ruby version changes THEN the system SHALL reload the appropriate core stubs
7. WHEN version detection fails THEN the system SHALL fall back to a reasonable default (Ruby 3.0)
8. WHEN detecting versions THEN support SHALL be provided for Ruby minor versions defined in `SUPPORTED_RUBY_VERSIONS` array (1.9 through 3.4)
9. WHEN client Ruby version is detected THEN it SHALL be mapped to closest supported minor version
10. WHEN patch version differences exist THEN they SHALL be handled gracefully (e.g., 2.7.6 uses 2.7 stubs)
11. WHEN exact version match is not found THEN system SHALL fall back to the closest available minor version

### Requirement 2: Pre-Generated Core Class Stubs

**User Story:** As a maintainer of the Ruby Fast LSP, I want pre-generated Ruby core class stubs packaged with the extension so that the stubs are accurate, comprehensive, and immediately available without network dependencies.

#### Acceptance Criteria

1. WHEN packaging the extension THEN pre-generated stubs SHALL be included for all supported Ruby versions, organized by minor version
2. WHEN a Ruby version is selected THEN stubs SHALL be loaded from the packaged files for all core classes and modules using individual .rb files for each core class (object.rb, string.rb, array.rb, etc.)
3. WHEN organizing stubs THEN a single core/ folder SHALL be used for core classes (standard library excluded)
4. WHEN core classes have version-specific methods THEN only methods available in that version SHALL be included in the packaged stubs
5. WHEN methods have different signatures across versions THEN version-specific signatures SHALL be maintained in separate stub files
6. WHEN classes are introduced in specific versions THEN they SHALL only appear in appropriate version stub packages
7. WHEN classes are deprecated or removed THEN they SHALL be excluded from newer version stub packages
8. WHEN documentation is included THEN it SHALL be extracted from multiple sources (ruby-doc.org, RDoc, source code) with RDoc links preserved for enhanced documentation access
9. WHEN organizing by version THEN stubs SHALL be stored in minor version folders (1.9/, 2.0/, 2.7/, 3.4/, etc.)
10. WHEN excluding standard library THEN modules like JSON, Net::HTTP, URI, OpenSSL SHALL be excluded (available as actual Ruby files on client machines)
11. WHEN stub loading fails THEN the system SHALL provide meaningful error messages and fallback to a default stub set

### Requirement 3: Version-Specific Core Classes Coverage

**User Story:** As a Ruby developer, I want comprehensive coverage of Ruby core classes across all supported versions so that I have accurate information regardless of which Ruby version I'm using.

#### Acceptance Criteria

1. WHEN using Ruby 1.9.x THEN stubs SHALL include core classes available in that version series
2. WHEN using Ruby 2.x series THEN stubs SHALL reflect the evolution of core classes through 2.0-2.7
3. WHEN using Ruby 3.x series THEN stubs SHALL include modern Ruby features and class changes
4. WHEN core classes evolve THEN version-specific differences SHALL be accurately represented
5. WHEN new methods are added THEN they SHALL appear only in versions where they exist
6. WHEN methods are deprecated THEN deprecation information SHALL be included in appropriate versions
7. WHEN standard library classes are gemified THEN they SHALL be handled appropriately per version

### Requirement 4: Core Class and Module Coverage

**User Story:** As a Ruby developer, I want stubs for all essential Ruby core classes and modules so that I have complete information about Ruby's built-in functionality.

#### Acceptance Criteria

1. WHEN indexing core classes THEN `Object`, `BasicObject`, `Class`, `Module` SHALL be included
2. WHEN indexing numeric types THEN `Numeric`, `Integer`, `Float`, `Rational`, `Complex` SHALL be included
3. WHEN indexing collections THEN `Array`, `Hash`, `Set`, `Range`, `Enumerator` SHALL be included
4. WHEN indexing strings THEN `String`, `Symbol`, `Regexp`, `MatchData` SHALL be included
5. WHEN indexing I/O classes THEN `IO`, `File`, `Dir`, `StringIO` SHALL be included
6. WHEN indexing time classes THEN `Time`, `Date`, `DateTime` SHALL be included
7. WHEN indexing core modules THEN `Kernel`, `Enumerable`, `Comparable` SHALL be included
8. WHEN indexing exception classes THEN `Exception` hierarchy SHALL be included
9. WHEN indexing concurrency classes THEN `Thread`, `Fiber`, `Mutex` SHALL be included
10. WHEN indexing metaprogramming classes THEN `Method`, `UnboundMethod`, `Binding`, `Proc` SHALL be included

### Requirement 5: Method Signature and Documentation

**User Story:** As a Ruby developer, I want detailed method signatures and documentation for core class methods so that I can understand how to use them correctly without external references.

#### Acceptance Criteria

1. WHEN a core method is indexed THEN its complete signature SHALL be available
2. WHEN methods have optional parameters THEN default values SHALL be documented where available
3. WHEN methods have keyword arguments THEN they SHALL be properly represented
4. WHEN methods have block parameters THEN block signatures SHALL be included
5. WHEN methods have multiple overloads THEN all variants SHALL be documented
6. WHEN methods have return types THEN they SHALL be indicated where determinable
7. WHEN methods have documentation THEN it SHALL be included in the stub
8. WHEN methods raise specific exceptions THEN this information SHALL be documented

### Requirement 6: Extension Configuration Integration

**User Story:** As a Ruby developer, I want to configure which Ruby version to use through the VS Code extension settings so that I can override automatic detection when needed.

#### Acceptance Criteria

1. WHEN opening extension settings THEN a Ruby version selector SHALL be available
2. WHEN the selector is opened THEN it SHALL list all detected Ruby versions
3. WHEN a version is manually selected THEN it SHALL override automatic detection
4. WHEN the configuration changes THEN the LSP SHALL reload with the new version stubs
5. WHEN no manual selection is made THEN automatic detection SHALL be used
6. WHEN an invalid version is configured THEN the system SHALL show an error and use fallback
7. WHEN version detection is disabled THEN manual selection SHALL be required

### Requirement 7: Stub Storage and Loading

**User Story:** As a Ruby developer, I want core class stubs to be efficiently loaded from pre-packaged files so that the extension starts quickly and works reliably offline.

#### Acceptance Criteria

1. WHEN the extension starts THEN stub files SHALL be loaded from the packaged directory structure
2. WHEN a Ruby version is selected THEN only the relevant stub files SHALL be loaded into memory
3. WHEN stub files are corrupted or missing THEN the system SHALL detect this and fall back to a default stub set
4. WHEN memory usage becomes high THEN unused stub data SHALL be unloaded automatically
5. WHEN the extension is offline THEN all packaged stubs SHALL still be available
6. WHEN switching between Ruby versions THEN stub loading SHALL be fast (<500ms)
7. WHEN multiple Ruby versions are used THEN each version's stubs SHALL be loaded independently

### Requirement 8: Integration with Existing Index

**User Story:** As a maintainer of the Ruby Fast LSP, I want core class stubs to integrate seamlessly with the existing RubyIndex so that core classes appear alongside project-defined classes in completion and navigation.

#### Acceptance Criteria

1. WHEN the RubyIndex is built THEN core class stubs SHALL be included automatically
2. WHEN core classes are indexed THEN they SHALL use the same `Entry` structures as project classes
3. WHEN core methods are indexed THEN they SHALL be available for completion and navigation
4. WHEN core classes have inheritance THEN the hierarchy SHALL be properly represented
5. WHEN core modules are mixed in THEN their methods SHALL be available on including classes
6. WHEN core constants are defined THEN they SHALL be available for constant completion
7. WHEN the index is updated THEN core stubs SHALL remain consistent with the selected Ruby version

### Requirement 9: Performance Requirements

**User Story:** As a Ruby developer, I want core class stub loading to be fast and not impact my development workflow.

#### Acceptance Criteria

1. WHEN the extension starts THEN core stub loading SHALL complete within 1 second
2. WHEN switching Ruby versions THEN new stubs SHALL load within 500ms
3. WHEN stubs are pre-packaged THEN loading SHALL be under 200ms for typical versions
4. WHEN the extension is first installed THEN all stubs SHALL be immediately available
5. WHEN multiple versions are loaded THEN memory usage SHALL not exceed 50MB total
6. WHEN the system is under load THEN stub operations SHALL not block the main LSP thread
7. WHEN working offline THEN stub performance SHALL be identical to online performance

### Requirement 10: Version Evolution Handling

**User Story:** As a Ruby developer working across different Ruby versions, I want the LSP to accurately reflect the evolution of Ruby core classes so that I don't encounter false positives or missing methods.

#### Acceptance Criteria

1. WHEN using Ruby 1.9 THEN `BasicObject` SHALL be available (introduced in 1.9)
2. WHEN using Ruby 2.0 THEN `Refinements` SHALL be available (introduced in 2.0)
3. WHEN using Ruby 2.1 THEN `Rational` and `Complex` literal syntax SHALL be reflected
4. WHEN using Ruby 2.2 THEN `Symbol` GC and frozen string optimizations SHALL be noted
5. WHEN using Ruby 2.3 THEN safe navigation operator support SHALL be available
6. WHEN using Ruby 2.4 THEN `Integer` unification (no separate `Fixnum`/`Bignum`) SHALL be reflected
7. WHEN using Ruby 2.5 THEN `rescue` in blocks and `yield_self` SHALL be available
8. WHEN using Ruby 2.6 THEN endless ranges and `then` method SHALL be available
9. WHEN using Ruby 2.7 THEN pattern matching preview and numbered parameters SHALL be available
10. WHEN using Ruby 3.0+ THEN positional and keyword argument separation SHALL be enforced
11. WHEN using Ruby 3.1+ THEN `MatchData#deconstruct` and other pattern matching enhancements SHALL be available
12. WHEN using Ruby 3.2+ THEN `Data` class and other new features SHALL be available

### Requirement 11: Error Handling and Fallbacks

**User Story:** As a Ruby developer, I want the core stub system to handle errors gracefully so that LSP functionality remains available even when stub generation or loading fails.

#### Acceptance Criteria

1. WHEN stub generation fails THEN the system SHALL log detailed error information
2. WHEN network access is unavailable THEN cached stubs SHALL be used if available
3. WHEN a specific Ruby version stub is missing THEN the closest available version SHALL be used
4. WHEN stub parsing fails THEN the system SHALL continue with partial data
5. WHEN version detection fails THEN a reasonable default version SHALL be assumed
6. WHEN core stub loading fails THEN project indexing SHALL continue without core stubs
7. WHEN errors occur THEN user-friendly error messages SHALL be displayed

### Requirement 12: Build-Time Stub Generation

**User Story:** As a maintainer of the Ruby Fast LSP, I want an automated build process that generates and packages core class stubs from multiple documentation sources including RDoc so that the extension always ships with up-to-date and accurate stub files for all supported Ruby versions defined in the `SUPPORTED_RUBY_VERSIONS` array.

#### Acceptance Criteria

1. WHEN building the extension THEN stubs SHALL be generated from multiple sources:
   - Primary: Official Ruby documentation (ruby-doc.org)
   - Secondary: RDoc from Ruby source repositories
   - Fallback: Ruby source code parsing
2. WHEN generating stubs THEN individual .rb files SHALL be created for each core class only
3. WHEN organizing stubs THEN they SHALL be stored in minor version folders with core/ subdirectory only
4. WHEN using RDoc integration THEN method signatures and documentation SHALL be extracted from local Ruby installations
5. WHEN Ruby documentation is updated THEN the build process SHALL detect and incorporate changes
6. WHEN generating stubs THEN the process SHALL validate syntax and completeness
7. WHEN packaging stubs THEN they SHALL be compressed and organized efficiently
8. WHEN the build completes THEN stub generation logs SHALL be available for debugging
9. WHEN stub generation fails THEN the build SHALL fail with clear error messages
10. WHEN releasing the extension THEN stub freshness SHALL be verified automatically
11. WHEN documentation sources are configured THEN priority ordering SHALL be respected
12. WHEN generating stubs THEN proper attribution, source information, and RDoc links SHALL be included
13. WHEN using multiple sources THEN cross-reference validation SHALL ensure consistency
14. WHEN processing versions THEN build process SHALL iterate through `SUPPORTED_RUBY_VERSIONS` array
15. WHEN excluding standard library THEN standard library modules SHALL be excluded from generation process

### Requirement 13: Testing and Validation

**User Story:** As a maintainer of the Ruby Fast LSP, I want comprehensive testing of the core stub system to ensure accuracy and reliability across all supported Ruby versions.

#### Acceptance Criteria

1. WHEN core stubs are generated THEN they SHALL be validated against known Ruby version features
2. WHEN version-specific differences exist THEN tests SHALL verify correct version handling
3. WHEN stub integration occurs THEN tests SHALL verify proper RubyIndex integration
4. WHEN performance requirements exist THEN benchmark tests SHALL validate loading times
5. WHEN edge cases are identified THEN specific test cases SHALL prevent regressions
6. WHEN Ruby versions are updated THEN tests SHALL verify continued compatibility
7. WHEN stub generation changes THEN tests SHALL verify output consistency

## Non-Functional Requirements

### Performance
- Core stub loading SHALL complete within 200ms for any supported Ruby version
- Memory usage for core stubs SHALL not exceed 10MB per Ruby version
- Stub generation SHALL complete within 30 seconds for any Ruby version
- Index integration SHALL not degrade existing performance by more than 5%

### Reliability
- The system SHALL handle network failures gracefully with cached fallbacks
- Core stub functionality SHALL remain available during Ruby version switches
- The system SHALL maintain consistency between selected Ruby version and loaded stubs
- Stub corruption SHALL be detected and automatically resolved

### Usability
- Ruby version selection SHALL be intuitive and discoverable in extension settings
- Version detection SHALL work automatically for common Ruby version managers
- Error messages SHALL provide actionable guidance for resolution
- Core class information SHALL be indistinguishable from project classes in completion

### Maintainability
- Stub generation SHALL be automated and reproducible
- The implementation SHALL reuse existing RubyIndex infrastructure
- Version-specific logic SHALL be clearly separated and documented
- The system SHALL be extensible for future Ruby versions

## Success Criteria

### Functional Success Criteria

1. **Complete Ruby Version Support**: Successfully detect and provide stubs for Ruby versions 1.9.3 through 3.5.x
2. **Comprehensive Core Class Coverage**: Include all essential Ruby core classes and modules with accurate method signatures
3. **Seamless Integration**: Core class methods appear in completion, go-to-definition works, and hover information is available
4. **Automatic Version Detection**: 90%+ accuracy in detecting Ruby versions from popular version managers and project files
5. **Offline Functionality**: Full functionality without any network dependencies using pre-packaged stubs

### Performance Success Criteria

1. **Fast Startup**: Core stub loading adds less than 200ms to extension startup time
2. **Efficient Memory Usage**: Memory footprint under 30MB for typical usage (2-3 Ruby versions)
3. **Quick Version Switching**: Switching between Ruby versions completes in under 500ms
4. **Responsive LSP**: No noticeable impact on LSP response times for completion, hover, or navigation
5. **Instant Availability**: All stubs available immediately upon extension installation

### User Experience Success Criteria

1. **Zero Configuration**: Works out-of-the-box for 95% of Ruby projects without manual setup
2. **Reliable Operation**: No network-related failures or timeouts affecting core functionality
3. **Consistent Behavior**: Identical functionality across different operating systems and network conditions
4. **Documentation Quality**: Hover information and completion details are helpful and accurate
5. **Small Extension Size**: Total extension size remains under 50MB despite including all stub files

## Implementation Notes

### Ruby Version Detection Strategy
- Check for `.ruby-version`, `.rvmrc`, `Gemfile` in project root
- Scan common version manager directories: `~/.rbenv/versions/`, `~/.rvm/rubies/`, `~/.rubies/`
- Use `rbenv version`, `rvm current`, `chruby` commands when available
- Fall back to `ruby --version` for system Ruby

### Stub Generation Approach
- Fetch Ruby documentation from ruby-doc.org API or scrape HTML documentation
- Parse class and method information using structured data extraction
- Generate Ruby-like stub files with method signatures and documentation
- Store stubs in version-specific directories within extension cache

### RubyIndex Integration
- Extend `RubyIndex` to accept pre-generated stub entries
- Create `Entry` objects for core classes with `EntryKind::Class`, `EntryKind::Module`
- Include core method entries with proper parent relationships
- Ensure core entries are marked as read-only and version-specific

### Extension Configuration
- Add `ruby.version` setting with dropdown of detected versions
- Add `ruby.autoDetectVersion` boolean setting (default: true)
- Add `ruby.coreStubsEnabled` boolean setting (default: true)
- Provide commands for refreshing version detection and regenerating stubs

### Caching Strategy
- Store stubs in `~/.vscode/extensions/ruby-fast-lsp/stubs/{version}/`
- Use JSON format for efficient loading and parsing
- Include version metadata and generation timestamps
- Implement LRU cache cleanup for disk space management

## Future Extensibility
- Design for potential integration with RBS (Ruby Signature) files
- Consider support for custom core class extensions and monkey patches
- Plan for integration with Ruby LSP protocol extensions
- Design for potential real-time documentation updates and community contributions