# Test user-defined method return type inference
# This pattern matches what the simulation tests create

class MyClass
  def get_string
    "hello"
  end
  
  def get_number
    42
  end
end

var_1 = MyClass.new
var_2 = var_1.get_string
var_3 = var_1.get_number
