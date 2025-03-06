class Person
  attr_accessor :name, :age

  def initialize(name, age)
    @name = name  # instance variable
    @age = age    # instance variable
  end

  def greet
    greeting = "Hello, my name is #{@name}"  # local variable
    puts greeting

    begin
      puts greeting
      raise "Raise an error"
    rescue => e
      puts e.message
    end
  end

  def birthday
    @age += 1
    puts "Happy Birthday! Now I am #{@age} years old."
  end
end

# Create a new person
person = Person.new("John", 30)
person.greet
person.birthday
puts "Name: #{person.name}"  # Using the accessor
