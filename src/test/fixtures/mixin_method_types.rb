# Test mixin method return type inference
# This pattern is what fails in simulation tests

module Base
  def get_string
    "hello"
  end
end

class MyClass
  include Base
end

var_1 = MyClass.new
var_2 = var_1.get_string  # Expected: String (from Base.get_string)
