# Test fixture for basic method references

# Top-level method definition
def global_helper
  puts "Global helper method"
end

# Top-level method call
global_helper

class Calculator
  def initialize
    # Call instance method from constructor
    reset
  end

  def add(a, b)
    a + b
  end

  def multiply(x, y)
    # Call another instance method
    result = add(x * y, 0)
    log_operation("multiply", x, y, result)
    result
  end

  def self.version
    "1.0.0"
  end

  def self.create_default
    # Call class method from class method
    puts "Version: #{version}"
    new
  end

  def reset
    @value = 0
  end

  private

  def log_operation(op, x, y, result)
    puts "#{op}(#{x}, #{y}) = #{result}"
  end
end

# Class method calls
Calculator.version
calc = Calculator.create_default

# Instance method calls
result1 = calc.add(5, 3)
result2 = calc.multiply(4, 6)

module MathUtils
  def self.square(n)
    n * n
  end

  def self.cube(n)
    # Call another module method
    square(n) * n
  end

  module Constants
    PI = 3.14159

    def self.circle_area(radius)
      # Reference to parent module method
      MathUtils.square(radius) * PI
    end
  end
end

# Module method calls
squared = MathUtils.square(5)
cubed = MathUtils.cube(3)
area = MathUtils::Constants.circle_area(2)

class Shape
  def area
    raise "Not implemented"
  end

  def perimeter
    raise "Not implemented"
  end
end

class Rectangle < Shape
  def initialize(width, height)
    @width = width
    @height = height
  end

  def area
    @width * @height
  end

  def perimeter
    2 * (@width + @height)
  end

  def describe
    # Call inherited methods
    puts "Rectangle: area=#{area}, perimeter=#{perimeter}"
  end
end

# Inheritance method calls
rect = Rectangle.new(10, 5)
rect.describe

# Call top-level method again
global_helper