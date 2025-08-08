module Outer
  module ModuleA
    def method_from_a
      puts "Method from Module A"
      method_from_b  # This should resolve to Outer::ModuleB
    end
  end

  module ModuleB
    def method_from_b
      puts "Method from Module B"
    end
  end

  class TestClass
    include ModuleA  # This is partially qualified - should resolve to Outer::ModuleA
    include ModuleB  # This is partially qualified - should resolve to Outer::ModuleB

    def test_method
      method_from_a  # Should work
      method_from_b  # Should work via cross-module resolution
    end
  end
end

# Another namespace with same module names
module Other
  module ModuleA
    def other_method_a
      puts "Other Module A"
    end
  end

  module ModuleB
    def other_method_b
      puts "Other Module B"
    end
  end
end

# Test instance
test_instance = Outer::TestClass.new
test_instance.test_method