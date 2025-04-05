module TopLevel
  class FirstClass
    def method1
      puts "Method in FirstClass"
    end
  end
  
  module Nested
    class SecondClass
      def method2
        puts "Method in SecondClass"
      end
    end
  end
end

# References to classes and modules
top_level = TopLevel
first_class = TopLevel::FirstClass.new
nested = TopLevel::Nested
second_class = TopLevel::Nested::SecondClass.new

# Reopening modules and classes
module TopLevel
  # Adding a new method to FirstClass
  class FirstClass
    def another_method
      puts "Another method in FirstClass"
    end
  end
  
  # Adding a new class to Nested module
  module Nested
    class ThirdClass
      def method3
        puts "Method in ThirdClass"
      end
    end
  end
end

# References to the reopened classes
third_class = TopLevel::Nested::ThirdClass.new
