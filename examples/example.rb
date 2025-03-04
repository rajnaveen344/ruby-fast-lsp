# Example Ruby file for testing the LSP

class Person
  attr_accessor :name, :age
  
  def initialize(name, age)
    @name = name
    @age = age
  end
  
  def greeting
    "Hello, my name is #{@name} and I am #{@age} years old."
  end
  
  def adult?
    @age >= 18
  end
end

# Create a new person
person = Person.new("John", 30)

# Print greeting
puts person.greeting

# Check if adult
if person.adult?
  puts "#{person.name} is an adult."
else
  puts "#{person.name} is not an adult."
end
