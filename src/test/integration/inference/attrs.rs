use crate::test::harness::check;

#[tokio::test]
async fn test_attr_reader_instance() {
    check(
        r#"
class Foo
  attr_reader <def>:bar</def>

  def method
    self.b$0ar
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_writer_instance() {
    check(
        r#"
class Foo
  attr_writer <def>:bar</def>

  def method
    self.b$0ar = 1
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_accessor_instance() {
    check(
        r#"
class Foo
  attr_accessor <def>:bar</def>

  def method
    self.b$0ar
  end
end
"#,
    )
    .await;

    check(
        r#"
class Foo
  attr_accessor <def>:bar</def>

  def method
    self.b$0ar = 1
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_class_attribute_class_and_instance_accessors() {
    check(
        r#"
class Worker
  class_attribute <def>:queue_config</def>
end

Worker.queue_con$0fig
"#,
    )
    .await;

    check(
        r#"
class Worker
  class_attribute <def>:queue_config</def>
end

Worker.queue_con$0fig = {}
"#,
    )
    .await;

    check(
        r#"
class Worker
  class_attribute <def>:queue_config</def>
end

Worker.new.queue_con$0fig
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_reader_singleton() {
    check(
        r#"
class Foo
  class << self
    attr_reader <def>:bar</def>
  end

  def self.method
    # self.bar check removed
  end
end

Foo.b$0ar
"#,
    )
    .await;
}

#[tokio::test]
async fn test_multiple_attrs() {
    check(
        r#"
class Foo
  attr_accessor <def>:a</def>, :b
end

Foo.new.a$0
"#,
    )
    .await;

    check(
        r#"
class Foo
  attr_accessor :a, <def>:b</def>
end

Foo.new.b$0
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_string_names() {
    check(
        r#"
class Foo
  attr_reader '<def>bar</def>'
end

Foo.new.b$0ar
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_writer_assignment() {
    // Test direct assignment syntax which was a specific user request
    check(
        r#"
class Foo
  attr_writer <def>:name</def>
end

f = Foo.new
f.nam$0e = 1
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_mixed_string_symbol() {
    check(
        r#"
class Foo
  attr_accessor <def>:a</def>, 'b'
end

Foo.new.a$0
"#,
    )
    .await;

    check(
        r#"
class Foo
  attr_accessor :a, '<def>b</def>'
end

Foo.new.b$0
"#,
    )
    .await;
}

#[tokio::test]
async fn test_module_attributes() {
    check(
        r#"
module M
  attr_accessor <def>:m_attr</def>
end

class C
  include M
end

C.new.m_a$0ttr
"#,
    )
    .await;
}

#[tokio::test]
async fn test_attr_non_standard_names() {
    check(
        r#"
class Foo
  attr_reader <def>:PossibleConstants</def>
end

Foo.new.PossibleConsta$0nts
"#,
    )
    .await;
}

#[tokio::test]
async fn test_private_attr() {
    use crate::test::harness::FakeEditor;

    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "private_attr.rb",
            r#"
class Foo
  private
  attr_reader :secret
end

Foo.new.secret
"#,
        )
        .await;

    let defs = editor.goto_def_at("private_attr.rb", 5, 8).await;
    assert!(
        defs.is_empty(),
        "private attr reader must not resolve through explicit receiver, got {defs:?}"
    );
}

#[tokio::test]
async fn test_attr_dynamic_argument_goto() {
    check(
        r#"
<def>def get_bar
  :bar
end</def>

class Foo
  attr_accessor get_b$0ar
end
"#,
    )
    .await;
}
