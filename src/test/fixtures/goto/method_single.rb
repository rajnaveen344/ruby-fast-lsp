class Greeter
    def initialize
        puts "Greeter initialized"
    end
  
    def self.hello
        puts "Hello, World! from class method"
    end
  
    def hello
        puts "Hello, World! from instance method"
    end
  
    def greet(name)
        puts "Hello, #{name}"
    end
  
    def run
        greet("Alice")
    end
end

Greeter.hello

Greeter.new.hello

test.hello

def top_method
    puts 'top_method'
end

top_method
  