module Outer
  OUTER_CONSTANT = "Outer constant"

  module Inner
    INNER_CONSTANT = "Inner constant"

    module Outer::Mod2
      MOD2_CONSTANT = "Mod2 constant"
    end

    class Outer::Klass2
      K2_CONSTANT = "Klass2 constant"
    end

    class Outer2::Inner::Klass3
      K3_CONSTANT = "Klass3 constant"
    end

    class Klass
      CLASS_CONSTANT = "Class constant"

      def method_using_constants
        # Reference to constants at different levels
        puts OUTER_CONSTANT
        puts INNER_CONSTANT
        puts CLASS_CONSTANT
        puts ::GLOBAL_CONSTANT
        puts ::Outer::OUTER_CONSTANT
        puts ::Outer::Inner::INNER_CONSTANT
        puts ::Outer::Inner::Klass::CLASS_CONSTANT
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
puts GLOBAL_CONSTANT
puts Outer::Mod2::MOD2_CONSTANT
puts Outer::Klass2::KLASS2_CONSTANT

A.new(Outer::Klass2::KLASS2_CONSTANT)
