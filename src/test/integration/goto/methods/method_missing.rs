//! Goto definition around `method_missing` fallbacks.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn goto_missing_method_does_not_resolve_method_missing() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "actions.rb",
            r#"module App
  module Actions
    def run
      virtual_total
    end
  end
end
"#,
        )
        .await;
    editor
        .open(
            "app_base.rb",
            r#"class AppBase
  include App::Actions
end
"#,
        )
        .await;
    editor
        .open(
            "basic_object.rb",
            r#"class BasicObject
  def method_missing(name, *args)
  end
end

class Object < BasicObject
end
"#,
        )
        .await;

    let defs = editor.goto_def_at("actions.rb", 3, 10).await;
    assert!(
        defs.is_empty(),
        "missing method goto must not navigate to method_missing, got {defs:?}"
    );
}

#[tokio::test]
async fn goto_direct_method_missing_call_resolves_method_missing_definition() {
    check(
        r#"
class DynamicRecord
  <def>def method_missing(name, *args)
    "dynamic"
  end</def>
end

DynamicRecord.new.method_missing$0(:virtual_total)
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_included_module_method_does_not_include_method_missing_fallback() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "orders.rb",
            r#"module App
  module API
    module Orders
      def build_summary
        result = {}
        result[:summary] = shared_helper([result[:id]])
      end
    end
  end
end
"#,
        )
        .await;
    editor
        .open(
            "shared_helpers.rb",
            r#"module App
  module API
    module SharedHelpers
      def shared_helper(items)
        {}
      end
    end
  end
end
"#,
        )
        .await;
    editor
        .open(
            "api.rb",
            r#"module App
  module API
    include App::API::Orders
    include App::API::SharedHelpers
  end
end

class ApiBase
  include App::API
end

class OrdersOnly
  include App::API::Orders
end
"#,
        )
        .await;
    editor
        .open(
            "basic_object.rb",
            r#"class BasicObject
  def method_missing(name, *args)
  end
end

class Object < BasicObject
end
"#,
        )
        .await;

    let defs = editor.goto_def_at("orders.rb", 5, 37).await;
    assert_eq!(
        defs.len(),
        1,
        "included module method should resolve only to its real definition, got {defs:?}"
    );
    assert!(
        defs.iter()
            .all(|loc| loc.uri.path().ends_with("/shared_helpers.rb")),
        "included module method must not include BasicObject#method_missing fallback, got {defs:?}"
    );
}
