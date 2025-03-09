class Calculator
  def add(a, b)
    a + b
  end

  def subtract(a, b)
    a - b
  end

  def multiply(a, b)
    a * b
  end

  def divide(a, b)
    a / b if b != 0
  end
end

calc = Calculator.new
puts calc.add(5, 3)
puts calc.subtract(10, 4)
puts calc.multiply(2, 6)
puts calc.divide(8, 2)
