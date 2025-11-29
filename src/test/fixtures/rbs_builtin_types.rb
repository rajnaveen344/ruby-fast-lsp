# Test file for RBS built-in type resolution
# This file tests that built-in Ruby methods get their return types from RBS

# String methods
name = "hello"
length = name.length        # Should infer: Integer (from RBS String#length)
upper = name.upcase         # Should infer: String (from RBS String#upcase -> self)
reversed = name.reverse     # Should infer: String
chars = name.chars          # Should infer: Array[String]

# Integer methods
count = 42
str_count = count.to_s      # Should infer: String (from RBS Integer#to_s)
float_count = count.to_f    # Should infer: Float

# Array methods
items = [1, 2, 3]
arr_length = items.length   # Should infer: Integer
first = items.first         # Should infer: Integer? (element type)
last = items.last           # Should infer: Integer?

# Hash methods
data = { name: "John", age: 30 }
keys = data.keys            # Should infer: Array[Symbol]
values = data.values        # Should infer: Array[Object]

# Method chaining
result = "hello world".split(" ").length  # Should infer: Integer
