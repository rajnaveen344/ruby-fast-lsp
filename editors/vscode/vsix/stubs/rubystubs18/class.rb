# frozen_string_literal: true

class Class
  # Creates a new anonymous (unnamed) class with the given superclass
  # (or <code>Object</code> if no parameter is given). You can give a
  # class a name by assigning the class object to a constant.
  def initialize(super_class = Object) end

  # Allocates space for a new object of <i>class</i>'s class and does not
  # call initialize on the new instance. The returned object must be an
  # instance of <i>class</i>.
  #
  #     klass = Class.new do
  #       def initialize(*args)
  #         @initialized = true
  #       end
  #
  #       def initialized?
  #         @initialized || false
  #       end
  #     end
  #
  #     klass.allocate.initialized? #=> false
  def allocate; end

  # Calls <code>allocate</code> to create a new object of
  # <i>class</i>'s class, then invokes that object's
  # <code>initialize</code> method, passing it <i>args</i>.
  # This is the method that ends up getting called whenever
  # an object is constructed using .new.
  def new(*args) end

  # Returns the superclass of <i>class</i>, or <code>nil</code>.
  #
  #    File.superclass     #=> IO
  #    IO.superclass       #=> Object
  #    Object.superclass   #=> nil
  def superclass; end

  private

  # Callback invoked whenever a subclass of the current class is created.
  #
  # Example:
  #
  #    class Foo
  #       def self.inherited(subclass)
  #          puts "New subclass: #{subclass}"
  #       end
  #    end
  #
  #    class Bar < Foo
  #    end
  #
  #    class Baz < Bar
  #    end
  #
  # produces:
  #
  #    New subclass: Bar
  #    New subclass: Baz
  def inherited(subclass) end
end
