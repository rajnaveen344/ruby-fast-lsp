module Outer
  OUTER_CONSTANT = "Outer constant"
  
  module Inner
    INNER_CONSTANT = "Inner constant"
    
    class Klass
      CLASS_CONSTANT = "Class constant"
      
      def method_using_constants
        # Reference to constants at different levels
        puts OUTER_CONSTANT
        puts INNER_CONSTANT
        puts CLASS_CONSTANT
        puts ::GLOBAL_CONSTANT
      end
    end
    
    # Reference to class constant from outside the class
    puts Klass::CLASS_CONSTANT
  end
  
  # Reference to inner constant using path
  puts Inner::INNER_CONSTANT
end

# Global constant
GLOBAL_CONSTANT = "Global constant"

# Reference to constants using full paths
puts Outer::OUTER_CONSTANT
puts Outer::Inner::INNER_CONSTANT
puts Outer::Inner::Klass::CLASS_CONSTANT
