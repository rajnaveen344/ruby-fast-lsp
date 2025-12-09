# Test deep transitive mixin method return type inference
# This pattern matches the failing simulation test

module Mod_0
  def method_1
    "from base"
  end
end

module Mod_2
  include Mod_0
  def method_4
    101
  end
end

module Mod_5
  include Mod_2
  def method_7
    102
  end
end

module Mod_8
  include Mod_5
  def method_10
    103
  end
end

class Class_11
  include Mod_8
  def method_13
    [1, 2, 3]
  end

  def use_mixin_methods
    var_14 = self.method_1  # Should infer String (from Mod_0, 4 levels deep)
    var_14
  end
end

# Top-level test
var_16 = Class_11.new
var_18 = var_16.method_1  # Should infer String (from Mod_0)
