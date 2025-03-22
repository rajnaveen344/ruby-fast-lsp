class Foo
  def bar
    puts "Hello!"
  end

  def another_method
    # Call bar method to create a reference
    bar
  end
end

class Foo
    def abcd
      puts "Hello!"
    end
end

# Create an instance and call methods to test references
foo_instance = Foo.new
foo_instance.bar
