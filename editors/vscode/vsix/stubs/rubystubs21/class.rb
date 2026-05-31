# frozen_string_literal: true

# Classes in Ruby are first-class objects---each is an instance of
# class <code>Class</code>.
#
# Typically, you create a new class by using:
#
#   class Name
#    # some code describing the class behavior
#   end
#
# When a new class is created, an object of type Class is initialized and
# assigned to a global constant (<code>Name</code> in this case).
#
# When <code>Name.new</code> is called to create a new object, the
# <code>new</code> method in <code>Class</code> is run by default.
# This can be demonstrated by overriding <code>new</code> in
# <code>Class</code>:
#
#    class Class
#      alias old_new new
#      def new(*args)
#        print "Creating a new ", self.name, "\n"
#        old_new(*args)
#      end
#    end
#
#    class Name
#    end
#
#    n = Name.new
#
# <em>produces:</em>
#
#    Creating a new Name
#
# Classes, modules, and objects are interrelated. In the diagram
# that follows, the vertical arrows represent inheritance, and the
# parentheses metaclasses. All metaclasses are instances
# of the class `Class'.
#                            +---------+             +-...
#                            |         |             |
#            BasicObject-----|-->(BasicObject)-------|-...
#                ^           |         ^             |
#                |           |         |             |
#             Object---------|----->(Object)---------|-...
#                ^           |         ^             |
#                |           |         |             |
#                +-------+   |         +--------+    |
#                |       |   |         |        |    |
#                |    Module-|---------|--->(Module)-|-...
#                |       ^   |         |        ^    |
#                |       |   |         |        |    |
#                |     Class-|---------|---->(Class)-|-...
#                |       ^   |         |        ^    |
#                |       +---+         |        +----+
#                |                     |
#   obj--->OtherClass---------->(OtherClass)-----------...
class Class < Module
  # Creates a new anonymous (unnamed) class with the given superclass
  # (or <code>Object</code> if no parameter is given). You can give a
  # class a name by assigning the class object to a constant.
  #
  # If a block is given, it is passed the class object, and the block
  # is evaluated in the context of this class using
  # <code>class_eval</code>.
  #
  #    fred = Class.new do
  #      def meth1
  #        "hello"
  #      end
  #      def meth2
  #        "bye"
  #      end
  #    end
  #
  #    a = fred.new     #=> #<#<Class:0x100381890>:0x100376b98>
  #    a.meth1          #=> "hello"
  #    a.meth2          #=> "bye"
  #
  # Assign the class to a constant (name starting uppercase) if you
  # want to treat it like a regular class.
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
  #    File.superclass          #=> IO
  #    IO.superclass            #=> Object
  #    Object.superclass        #=> BasicObject
  #    class Foo; end
  #    class Bar < Foo; end
  #    Bar.superclass           #=> Foo
  #
  # Returns nil when the given class does not have a parent class:
  #
  #    BasicObject.superclass   #=> nil
  def superclass; end

  private

  # Callback invoked whenever a subclass of the current class is created.
  #
  # Example:
  #
  #    class Foo
  #      def self.inherited(subclass)
  #        puts "New subclass: #{subclass}"
  #      end
  #    end
  #
  #    class Bar < Foo
  #    end
  #
  #    class Baz < Bar
  #    end
  #
  # <em>produces:</em>
  #
  #    New subclass: Bar
  #    New subclass: Baz
  def inherited(subclass) end
end
