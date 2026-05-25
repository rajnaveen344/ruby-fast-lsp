# Ruby Edge Cases for Static Analysis

This document tracks Ruby language features that affect static analysis, particularly for the inheritance graph and method resolution.

## Known Limitations (Not Supported)

### Dynamic Mixins

```ruby
class User
  include SomeModule if Rails.env.production?
  include(*MIXINS)  # Splat from array
end
```

**Why:** Requires runtime evaluation. Static analysis cannot determine which modules are included.

### Runtime Includes

```ruby
class User
  def self.enable_feature!
    include FeatureModule  # Called at runtime
  end
end
```

**Why:** `include` happens during method execution, not at parse time.

### Anonymous Modules

```ruby
class User
  include Module.new {
    def dynamic_method; end
  }
end
```

**Why:** No FQN for anonymous modules - can't be indexed or referenced.

### Dynamic `class_eval` / `module_eval`

```ruby
target = User
target.class_eval do
  def generated_method; end
end

User.class_eval("def generated_from_string; end")
```

**Why:** Non-constant receivers and string eval require runtime evaluation. Static analysis only handles direct constant receivers with block bodies.

### Dynamic `define_method`

```ruby
method_name = :"generated_#{kind}"
define_method(method_name) do
end

some_runtime_object.send(:define_method, :patched) do
end
```

**Why:** Non-literal method names and non-constant receivers require runtime evaluation. Static analysis only handles literal symbol/string method names and direct constant receivers.

### Dynamic ActiveSupport::Concern Callbacks

```ruby
module Nameable
  extend ActiveSupport::Concern

  included do
    # Arbitrary runtime DSL executed when included
  end
end
```

**Why:** Arbitrary callback bodies can call project-specific runtime DSLs. Static analysis only models direct method definitions and direct static mixin edges.

---

## Supported Features

### Prepend Ordering

```ruby
module Logging
  def save
    puts "Saving..."
    super
  end
end

class User
  prepend Logging
  def save; end
end
```

**Status:** Fully supported. The graph has separate `prepends` list, and `method_lookup_chain` processes prepends before the class itself.

### Extend Self

```ruby
module Utils
  extend self

  def helper; end  # Callable as Utils.helper AND as instance method
end
```

**Status:** Supported. `extend self` creates a singleton include edge back to the module instance namespace, so `Utils.helper` resolves to `Utils#helper`. Covered by explicit goto/refs/hover/type tests and the seeded simulator.

---

### `include` Inside Singleton Class

**Status:** Supported. `include` inside `class << self` is indexed on the singleton namespace, so module instance methods become class methods.

```ruby
class User
  class << self
    include AdminMethods
  end
end
```

---

### Static `class_eval` / `module_eval` Blocks

**Status:** Supported for direct constant receivers with block bodies. Methods inside the block are indexed on the receiver namespace, not the lexical caller namespace or top level.

```ruby
class User
end

User.class_eval do
  def generated_method
  end
end
```

### Static `define_method`

**Status:** Supported for literal symbol/string method names. Bare `define_method(:name)` is indexed in the current namespace. `Receiver.send(:define_method, :name)` is indexed on the direct constant receiver namespace.

```ruby
class User
  define_method(:generated_method) do
  end
end

User.send(:define_method, :patched_method) do
end

Net.const_get(:SMTP).send(:define_method, :tls?) do
end
```

### ActiveSupport::Concern `class_methods`

**Status:** Supported for static `class_methods do ... end` blocks. Methods defined inside the block are indexed as `ConcernModule::ClassMethods` instance methods, and the concern module gets an implicit include-hook extend edge so classes that include the concern resolve those as class methods. Covered for goto, references, hover, and type inference.

```ruby
module Searchable
  extend ActiveSupport::Concern

  class_methods do
    # @return [String]
    def find_by_term
      "ok"
    end
  end
end

class Product
  include Searchable
end

Product.find_by_term
```

### Static Constant Lookup

**Status:** Supported for literal symbol/string constant names in `Namespace.const_get(:Child)` and `Namespace.const_defined?(:Child)`. The literal selector is indexed as a constant reference, so goto, references, hover, type inference, and simulator checks track the target constant.

```ruby
SampleApp::Platform::Util.const_get(:TriggerHelpers)

class PushUnit
  TYPE = "push"

  def self.type
    self.const_defined?(:TYPE) ? self.const_get(:TYPE) : nil
  end
end
```

### Static `send` / `public_send` / `__send__`

**Status:** Supported for literal symbol/string method names. The literal selector is treated as the target method call, so goto, references, hover, diagnostics, and simulator checks use the receiver and the selected method, not `send` itself.

```ruby
User.new.send(:generated_method)
User.new.public_send("generated_method")
User.new.__send__(:generated_method)
```

### Block Parameter Type Flow

**Status:** Supported for common collection blocks and same-pass local methods that yield typed arguments. `Array<T>#each`/`map`-style blocks type their first block parameter as `T`; numbered `_1` block params receive the same type; `Hash<K,V>#each` with two block parameters types them as `K` and `V`; local `yield expr` methods type call-site block parameters from the yielded expression, including class-scoped helper methods. Anonymous block forwarding with `def wrapper(&); target(&); end` and argument/block forwarding with `def wrapper(...); target(...); end` propagate the target method's yielded parameter types to wrapper call-site blocks. For local methods known to yield or forward a yielded block, the block body's result also feeds call hover, assignment inlay hints, and method return inference.

```ruby
[User.new].each do |user|
  user.name
end

{ id: "u1" }.each do |key, value|
  key.to_s
  value.upcase
end

def with_user
  yield User.new
end

with_user do |user|
  user.name
end

def label
  with_user do |user|
    user.name
  end
end
```

### Method Object Reflection

**Status:** Supported for literal symbol/string method names in `method(:name)`, `Receiver.method(:name)`, and `Receiver.instance_method(:name)`. The symbol or string selector is indexed as a reference to the reflected method, so goto, references, hover, and simulator checks track the target method rather than `method` or `instance_method`.

```ruby
class FeatureSettings
  def self.get
  end

  def copy_data
  end
end

FeatureSettings.method(:get)
FeatureSettings.instance_method(:copy_data)
```

### Bare `module_function`

**Status:** Supported. `module_function :name` and bare `module_function` mode both add singleton-callable module methods while preserving the original method definition location for goto, references, and hover/type lookup.

```ruby
module Utils
  module_function

  def helper
  end
end

Utils.helper
```

### Forwardable Delegates

**Status:** Supported for literal symbol/string receiver and method names in `def_delegator` and `def_delegators`. Generated delegate methods are indexed for goto, references, hover/type lookup, and simulator checks. Common singleton usage inside `class << self` is supported for class-level delegates.

```ruby
require "forwardable"

class ServiceFlags
  extend Forwardable

  class << self
    def_delegators :instance, :allow?, :fetch_all
  end
end
```

### Rails `class_attribute`

**Status:** Supported for literal symbol/string names. The macro creates class and instance reader/writer method facts, so goto and references work for common static uses.

```ruby
class Worker
  class_attribute :queue_config
end

Worker.queue_config
Worker.new.queue_config
```

### Include Hook Methods

**Status:** Supported for static `def self.included(base)` hooks that call `base.extend(...)` or `base.send(:extend, ...)`, hooks that call `base.include(...)` or `base.send(:include, ...)`, and hooks that wrap `include` calls in `base.class_eval do ... end`. Classes that include the module receive the extended module's instance methods as class methods and the included module's instance methods as instance methods for goto, references, hover, and simulator checks.

```ruby
module FeatureFlags
  def self.included(base)
    base.extend(ClassMethods)
    base.send :include, SharedMethods
    base.class_eval do
      include RequestHelpers
    end
  end

  module ClassMethods
    def enabled?
      true
    end
  end

  module SharedMethods
    def get_html
      "html"
    end
  end

  module RequestHelpers
    def api_get
      "ok"
    end
  end
end

class Worker
  include FeatureFlags
end

Worker.enabled?
Worker.new.get_html
Worker.new.api_get
```

## Future Considerations

### Singleton Class Prepends

Ruby allows prepending in singleton class context:

```ruby
class User
  class << self
    prepend LoggingMethods
  end
end
```

**Status:** Supported. `prepend` inside `class << self` is indexed as a prepend edge on the singleton namespace, preserving class-method MRO.

### Method Visibility in Mixins

```ruby
module Secret
  private
  def hidden; end
end

class User
  include Secret
  # hidden is private in User too
end
```

**Status:** Supported for static visibility forms. Method facts track `public`/`protected`/`private` mode for ordinary definitions and visibility argument forms. Visibility overrides such as `private :hidden`, `protected :hidden`, and `public :hidden` apply through included/inherited method lookup for goto, references, hover, and type inference; private/protected reference filtering also respects same-family protected callers. Bare calls and `send` remain allowed.

### Pattern Matching Captures

```ruby
case {user: User.new}
in {user: user}
  user.name
end
```

**Status:** Supported for static hash/array literal patterns with local variable captures. Captures are indexed as local definitions for goto and references, and literal value types flow into hover, assignment hints, and method return hints.

### Refinements

```ruby
module StringRefinements
  refine String do
    def shout
      upcase + "!"
    end
  end
end

using StringRefinements
"hello".shout  # => "HELLO!"
```

Refinements are lexically scoped and extremely complex for static analysis. Currently not supported and likely won't be in the near future.
