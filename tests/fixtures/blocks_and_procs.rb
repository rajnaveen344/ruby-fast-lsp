# Block with do..end syntax
[1, 2, 3].each do |num|
  puts num * 2
end

# Block with {} syntax
[1, 2, 3].map { |num| num * 2 }

# Proc
my_proc = Proc.new { |x| puts x * 3 }
[1, 2, 3].each(&my_proc)

# Lambda with -> syntax
my_lambda = ->(x) { x * 4 }
result = [1, 2, 3].map(&my_lambda)
puts result.inspect

# Lambda with lambda keyword
double = lambda { |x| x * 2 }
puts double.call(5)
