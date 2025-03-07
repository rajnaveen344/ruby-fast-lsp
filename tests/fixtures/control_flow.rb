# if/elsif/else statements
age = 25

if age < 18
  puts "Minor"
elsif age < 21
  puts "Young adult"
else
  puts "Adult"
end

# Unless statement
unless age < 18
  puts "Not a minor"
end

# Ternary operator
status = age >= 18 ? "Adult" : "Minor"
puts status

# Case/when statement
case age
when 0..12
  puts "Child"
when 13..17
  puts "Teenager"
when 18..64
  puts "Adult"
else
  puts "Senior"
end

# Loops
i = 0
while i < 5
  puts "While: #{i}"
  i += 1
end

i = 0
until i >= 5
  puts "Until: #{i}"
  i += 1
end

for i in 0..4
  puts "For: #{i}"
end

# Loop with break and next
i = 0
loop do
  i += 1
  next if i % 2 == 0
  puts "Loop: #{i}"
  break if i >= 5
end
