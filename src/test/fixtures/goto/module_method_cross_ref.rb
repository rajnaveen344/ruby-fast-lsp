module ModuleA
  def method_from_a
    puts "Method from Module A"
    method_from_b  # This should go to definition in ModuleB
  end

  def another_method_a
    puts "Another method from A"
  end
end

module ModuleB
  def method_from_b
    puts "Method from Module B"
  end

  def helper_method
    puts "Helper method from B"
  end
end

class TestClass
  include ModuleA
  include ModuleB

  def test_method
    method_from_a  # This should go to definition in ModuleA
    method_from_b  # This should go to definition in ModuleB
  end

  def another_test
    helper_method  # This should go to definition in ModuleB
  end
end

# Usage examples
test_instance = TestClass.new
test_instance.test_method
test_instance.method_from_a
test_instance.method_from_b