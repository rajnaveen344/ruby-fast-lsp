# Test self.method type inference inside a method body
# This replicates the failing simulation test pattern

module Mod_0
  def method_1
    "from base"
  end
end

class Class_1
  include Mod_0

  def use_mixin_methods
    var_1 = self.method_1  # Should infer String
    var_1
  end
end

# Top-level test
var_2 = Class_1.new
var_3 = var_2.method_1  # Should infer String
