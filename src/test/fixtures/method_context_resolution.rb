# Test fixture for method context resolution with no receiver
# This tests the improved handle_no_receiver function

class TestClass
  # Class body - bare calls should be instance methods
  puts "In class body"
  helper_method  # Line 6 - should resolve as instance method
  
  def instance_method
    # Inside instance method - bare calls should be instance methods  
    some_method    # Line 10 - should resolve as instance method
    helper_method  # Line 11 - should resolve as instance method
  end
  
  def self.class_method
    # Inside class method - bare calls should be class methods
    some_method    # Line 16 - should resolve as class method
    helper_method  # Line 17 - should resolve as class method
  end
  
  class << self
    # Inside singleton context - bare calls should be class methods
    some_method    # Line 22 - should resolve as class method
    helper_method  # Line 23 - should resolve as class method
    
    def singleton_method
      # Inside singleton method - bare calls should be class methods
      some_method    # Line 27 - should resolve as class method
      helper_method  # Line 28 - should resolve as class method
    end
    
    def another_singleton
      nested_call    # Line 32 - should resolve as class method
    end
  end
  
  # Define the methods being called
  def helper_method
    "instance helper"
  end
  
  def self.helper_method
    "class helper"
  end
  
  def some_method
    "instance some_method"
  end
  
  def self.some_method
    "class some_method"
  end
end

# Top-level context - bare calls should be instance methods
top_level_call   # Line 53 - should resolve as instance method
helper_method    # Line 54 - should resolve as instance method

def top_level_method
  # Top-level method - bare calls should be instance methods
  some_method    # Line 58 - should resolve as instance method
  helper_method  # Line 59 - should resolve as instance method
end

# Nested class scenario
class OuterClass
  inner_call     # Line 64 - should resolve as instance method
  
  class InnerClass
    nested_call  # Line 67 - should resolve as instance method
    
    def inner_instance_method
      method_call  # Line 70 - should resolve as instance method
    end
    
    def self.inner_class_method
      method_call  # Line 74 - should resolve as class method
    end
    
    class << self
      singleton_call  # Line 78 - should resolve as class method
      
      def inner_singleton_method
        deep_call     # Line 81 - should resolve as class method
      end
    end
  end
end

# Module with singleton context
module TestModule
  module_call    # Line 88 - should resolve as instance method
  
  def self.module_method
    module_helper  # Line 91 - should resolve as class method
  end
  
  class << self
    singleton_module_call  # Line 95 - should resolve as class method
    
    def module_singleton_method
      deep_module_call     # Line 98 - should resolve as class method
    end
  end
end