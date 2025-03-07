begin
  # This will raise an error
  1 / 0
rescue ZeroDivisionError => e
  puts "Error: #{e.message}"
rescue StandardError => e
  puts "Some other error: #{e.message}"
else
  puts "No errors occurred"
ensure
  puts "This always executes"
end

# Inline rescue
result = 1 / 0 rescue "Division by zero"
puts result
