# Different variable types
class Person
  @@count = 0  # class variable
  ADULT_AGE = 18  # constant

  attr_accessor :name, :age

  def initialize(name, age)
    @name = name  # instance variable
    @age = age    # instance variable
    @@count += 1
  end

  def adult?
    local_var = 18  # local variable
    @age >= local_var
  end

  def self.count
    @@count
  end
end

$global_var = "I'm global"  # global variable

person = Person.new("John", 30)
puts "Name: #{person.name}"
puts "Age: #{person.age}"
puts "Adult? #{person.adult?}"
puts "Count: #{Person.count}"
puts "Global: #{$global_var}"
