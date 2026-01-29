//! Goto definition tests for YARD type annotations.

use crate::test::harness::check;

/// YARD type should resolve using namespace context.
/// Inside `GoshPosh` module, `Platform::PlatformServices` in YARD annotation
/// should resolve to `GoshPosh::Platform::PlatformServices`.
#[tokio::test]
async fn yard_type_with_namespace_resolution() {
    check(
        r#"
module GoshPosh
  module Platform
    <def>class PlatformServices
    end</def>
  end

  class Base
    # @return [Platform::PlatformServices$0]
    def services
    end
  end
end
"#,
    )
    .await;
}

/// YARD type with full namespace should work regardless of context.
#[tokio::test]
async fn yard_type_with_full_namespace() {
    check(
        r#"
module GoshPosh
  module Platform
    <def>class PlatformServices
    end</def>
  end

  class Base
    # @return [GoshPosh::Platform::PlatformServices$0]
    def services
    end
  end
end
"#,
    )
    .await;
}

/// YARD param type should resolve using namespace context.
#[tokio::test]
async fn yard_param_type_with_namespace_resolution() {
    check(
        r#"
module MyApp
  <def>class User
  end</def>

  class Service
    # @param user [User$0]
    def process(user)
    end
  end
end
"#,
    )
    .await;
}

/// YARD type at top level should resolve to top-level class.
#[tokio::test]
async fn yard_type_at_top_level() {
    check(
        r#"
<def>class TopLevelClass
end</def>

# @return [TopLevelClass$0]
def my_method
end
"#,
    )
    .await;
}

/// YARD type should fallback to top-level when not found in namespace.
#[tokio::test]
async fn yard_type_fallback_to_toplevel() {
    check(
        r#"
<def>class GlobalHelper
end</def>

module MyApp
  class Service
    # GlobalHelper is not in MyApp, should resolve to top-level
    # @return [GlobalHelper$0]
    def get_helper
    end
  end
end
"#,
    )
    .await;
}
