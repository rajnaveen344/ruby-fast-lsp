//! Exact locked Java-platform gem roots for a selected JRuby runtime.

use super::*;

#[test]
fn discovers_only_exact_locked_java_platform_gem_roots_for_selected_jruby() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let rvm = fixture.path().join(".rvm");
    let runtime = rvm.join("rubies/jruby-9.2.21.0");
    let executable = runtime.join("bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        concat!(
            "GEM\n",
            "  remote: https://rubygems.org/\n",
            "  specs:\n",
            "    bson (4.14.1-java)\n",
            "    bson (4.14.1)\n",
            "    rack (3.0.0)\n",
            "PLATFORMS\n",
            "  java\n",
        ),
    )
    .unwrap();
    let exact = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
    let wrong = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.0-java");
    let ruby = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1");
    fs::create_dir_all(&exact).unwrap();
    fs::create_dir_all(&wrong).unwrap();
    fs::create_dir_all(&ruby).unwrap();

    assert_eq!(
        discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
        vec![dunce::canonicalize(exact).unwrap()]
    );
}

#[test]
fn project_local_locked_java_gem_precedes_the_selected_rvm_runtime_copy() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let rvm = fixture.path().join(".rvm");
    let executable = rvm.join("rubies/jruby-9.2.21.0/bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        concat!(
            "GEM\n",
            "  remote: https://rubygems.org/\n",
            "  specs:\n",
            "    bson (4.14.1-java)\n",
            "PLATFORMS\n",
            "  java\n",
        ),
    )
    .unwrap();
    let local = project.join("vendor/bundle/jruby/2.5.0/gems/bson-4.14.1-java");
    let global = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(&global).unwrap();

    assert_eq!(
        discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
        vec![dunce::canonicalize(local).unwrap()]
    );
}

#[test]
fn duplicate_project_local_locked_java_gem_installations_fail_closed() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        concat!(
            "GEM\n",
            "  remote: https://rubygems.org/\n",
            "  specs:\n",
            "    bson (4.14.1-java)\n",
            "PLATFORMS\n",
            "  java\n",
        ),
    )
    .unwrap();
    for compatibility in ["2.5.0", "3.1.0"] {
        fs::create_dir_all(
            project
                .join("vendor/bundle/jruby")
                .join(compatibility)
                .join("gems/bson-4.14.1-java"),
        )
        .unwrap();
    }

    let error = discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous installations in project vendor/bundle"),
        "unexpected error: {error:?}"
    );
}
