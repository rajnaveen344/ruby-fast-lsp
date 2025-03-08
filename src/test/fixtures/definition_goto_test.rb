class Animal
  attr_reader :name

  def initialize(name)
    @name = name
  end

  def speak
    raise NotImplementedError, "Subclasses must implement speak"
  end
end

class Dog < Animal
  def speak
    "#{name} says Woof!"
  end
end

class Cat < Animal
  def speak
    "#{name} says Meow!"
  end
end

dog = Dog.new("Buddy")
puts dog.speak

cat = Cat.new("Whiskers")
puts cat.speak
