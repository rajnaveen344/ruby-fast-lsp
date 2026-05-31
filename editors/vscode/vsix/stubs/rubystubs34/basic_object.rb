# frozen_string_literal: true

# +BasicObject+ is the parent class of all classes in Ruby.
# In particular, +BasicObject+ is the parent class of class Object,
# which is itself the default parent class of every Ruby class:
#
#   class Foo; end
#   Foo.superclass    # => Object
#   Object.superclass # => BasicObject
#
# +BasicObject+ is the only class that has no parent:
#
#   BasicObject.superclass # => nil
#
# \Class +BasicObject+ can be used to create an object hierarchy
# (e.g., class Delegator) that is independent of Ruby's object hierarchy.
# Such objects:
#
# - Do not have namespace "pollution" from the many methods
#   provided in class Object and its included module Kernel.
# - Do not have definitions of common classes,
#   and so references to such common classes must be fully qualified
#   (+::String+, not +String+).
#
# A variety of strategies can be used to provide useful portions
# of the Standard Library in subclasses of +BasicObject+:
#
# - The immediate subclass could <tt>include Kernel</tt>,
#   which would define methods such as +puts+, +exit+, etc.
# - A custom Kernel-like module could be created and included.
# - Delegation can be used via #method_missing:
#
#     class MyObjectSystem < BasicObject
#       DELEGATE = [:puts, :p]
#
#       def method_missing(name, *args, &block)
#         return super unless DELEGATE.include? name
#         ::Kernel.send(name, *args, &block)
#       end
#
#       def respond_to_missing?(name, include_private = false)
#         DELEGATE.include?(name)
#       end
#     end
#
# === What's Here
#
# These are the methods defined for \BasicObject:
#
# - ::new: Returns a new \BasicObject instance.
# - #!: Returns the boolean negation of +self+: +true+ or +false+.
# - #!=: Returns whether +self+ and the given object are _not_ equal.
# - #==: Returns whether +self+ and the given object are equivalent.
# - #__id__: Returns the integer object identifier for +self+.
# - #__send__: Calls the method identified by the given symbol.
# - #equal?: Returns whether +self+ and the given object are the same object.
# - #instance_eval: Evaluates the given string or block in the context of +self+.
# - #instance_exec: Executes the given block in the context of +self+, passing the given arguments.
# - #method_missing: Called when +self+ is called with a method it does not define.
# - #singleton_method_added: Called when a singleton method is added to +self+.
# - #singleton_method_removed: Called when a singleton method is removed from +self+.
# - #singleton_method_undefined: Called when a singleton method is undefined in +self+.
class BasicObject
  # Returns a new BasicObject.
  def initialize; end

  # Boolean negate.
  def !; end

  # Returns true if two objects are not-equal, otherwise false.
  def !=(other) end

  # Equality --- At the Object level, #== returns <code>true</code>
  # only if +obj+ and +other+ are the same object.  Typically, this
  # method is overridden in descendant classes to provide
  # class-specific meaning.
  #
  # Unlike #==, the #equal? method should never be overridden by
  # subclasses as it is used to determine object identity (that is,
  # <code>a.equal?(b)</code> if and only if <code>a</code> is the same
  # object as <code>b</code>):
  #
  #   obj = "a"
  #   other = obj.dup
  #
  #   obj == other      #=> true
  #   obj.equal? other  #=> false
  #   obj.equal? obj    #=> true
  #
  # The #eql? method returns <code>true</code> if +obj+ and +other+
  # refer to the same hash key.  This is used by Hash to test members
  # for equality.  For any pair of objects where #eql? returns +true+,
  # the #hash value of both objects must be equal. So any subclass
  # that overrides #eql? should also override #hash appropriately.
  #
  # For objects of class Object, #eql?  is synonymous
  # with #==.  Subclasses normally continue this tradition by aliasing
  # #eql? to their overridden #== method, but there are exceptions.
  # Numeric types, for example, perform type conversion across #==,
  # but not across #eql?, so:
  #
  #    1 == 1.0     #=> true
  #    1.eql? 1.0   #=> false
  def ==(other) end
  alias equal? ==

  # Returns an integer identifier for +obj+.
  #
  # The same number will be returned on all calls to +object_id+ for a given
  # object, and no two active objects will share an id.
  #
  # Note: that some objects of builtin classes are reused for optimization.
  # This is the case for immediate values and frozen string literals.
  #
  # BasicObject implements +__id__+, Kernel implements +object_id+.
  #
  # Immediate values are not passed by reference but are passed by value:
  # +nil+, +true+, +false+, Fixnums, Symbols, and some Floats.
  #
  #     Object.new.object_id  == Object.new.object_id  # => false
  #     (21 * 2).object_id    == (21 * 2).object_id    # => true
  #     "hello".object_id     == "hello".object_id     # => false
  #     "hi".freeze.object_id == "hi".freeze.object_id # => true
  def __id__; end

  #  Invokes the method identified by _symbol_, passing it any
  #  arguments specified.
  #  When the method is identified by a string, the string is converted
  #  to a symbol.
  #
  #  BasicObject implements +__send__+, Kernel implements +send+.
  #  <code>__send__</code> is safer than +send+
  #  when _obj_ has the same method name like <code>Socket</code>.
  #  See also <code>public_send</code>.
  #
  #     class Klass
  #       def hello(*args)
  #         "Hello " + args.join(' ')
  #       end
  #     end
  #     k = Klass.new
  #     k.send :hello, "gentle", "readers"   #=> "Hello gentle readers"
  def __send__(...) end

  # Evaluates a string containing Ruby source code, or the given block,
  # within the context of the receiver (_obj_). In order to set the
  # context, the variable +self+ is set to _obj_ while
  # the code is executing, giving the code access to _obj_'s
  # instance variables and private methods.
  #
  # When <code>instance_eval</code> is given a block, _obj_ is also
  # passed in as the block's only argument.
  #
  # When <code>instance_eval</code> is given a +String+, the optional
  # second and third parameters supply a filename and starting line number
  # that are used when reporting compilation errors.
  #
  #    class KlassWithSecret
  #      def initialize
  #        @secret = 99
  #      end
  #      private
  #      def the_secret
  #        "Ssssh! The secret is #{@secret}."
  #      end
  #    end
  #    k = KlassWithSecret.new
  #    k.instance_eval { @secret }          #=> 99
  #    k.instance_eval { the_secret }       #=> "Ssssh! The secret is 99."
  #    k.instance_eval {|obj| obj == self } #=> true
  def instance_eval(...) end

  # Executes the given block within the context of the receiver
  # (_obj_). In order to set the context, the variable +self+ is set
  # to _obj_ while the code is executing, giving the code access to
  # _obj_'s instance variables.  Arguments are passed as block parameters.
  #
  #    class KlassWithSecret
  #      def initialize
  #        @secret = 99
  #      end
  #    end
  #    k = KlassWithSecret.new
  #    k.instance_exec(5) {|x| @secret+x }   #=> 104
  def instance_exec(*args) end

  private

  # Invoked by Ruby when <i>obj</i> is sent a message it cannot handle.
  # <i>symbol</i> is the symbol for the method called, and <i>args</i>
  # are any arguments that were passed to it. By default, the interpreter
  # raises an error when this method is called. However, it is possible
  # to override the method to provide more dynamic behavior.
  # If it is decided that a particular method should not be handled, then
  # <i>super</i> should be called, so that ancestors can pick up the
  # missing method.
  # The example below creates
  # a class <code>Roman</code>, which responds to methods with names
  # consisting of roman numerals, returning the corresponding integer
  # values.
  #
  #    class Roman
  #      def roman_to_int(str)
  #        # ...
  #      end
  #
  #      def method_missing(symbol, *args)
  #        str = symbol.id2name
  #        begin
  #          roman_to_int(str)
  #        rescue
  #          super(symbol, *args)
  #        end
  #      end
  #    end
  #
  #    r = Roman.new
  #    r.iv      #=> 4
  #    r.xxiii   #=> 23
  #    r.mm      #=> 2000
  #    r.foo     #=> NoMethodError
  def method_missing(name, *args) end

  # Invoked as a callback whenever a singleton method is added to the
  # receiver.
  #
  #    module Chatty
  #      def Chatty.singleton_method_added(id)
  #        puts "Adding #{id.id2name}"
  #      end
  #      def self.one()     end
  #      def two()          end
  #      def Chatty.three() end
  #    end
  #
  # <em>produces:</em>
  #
  #    Adding singleton_method_added
  #    Adding one
  #    Adding three
  def singleton_method_added(symbol) end

  # Invoked as a callback whenever a singleton method is removed from
  # the receiver.
  #
  #    module Chatty
  #      def Chatty.singleton_method_removed(id)
  #        puts "Removing #{id.id2name}"
  #      end
  #      def self.one()     end
  #      def two()          end
  #      def Chatty.three() end
  #      class << self
  #        remove_method :three
  #        remove_method :one
  #      end
  #    end
  #
  # <em>produces:</em>
  #
  #    Removing three
  #    Removing one
  def singleton_method_removed(symbol) end

  # Invoked as a callback whenever a singleton method is undefined in
  # the receiver.
  #
  #    module Chatty
  #      def Chatty.singleton_method_undefined(id)
  #        puts "Undefining #{id.id2name}"
  #      end
  #      def Chatty.one()   end
  #      class << self
  #         undef_method(:one)
  #      end
  #    end
  #
  # <em>produces:</em>
  #
  #    Undefining one
  def singleton_method_undefined(symbol) end
end
