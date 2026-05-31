# frozen_string_literal: true

# Method objects are created by Object#method, and are associated
# with a particular object (not just with a class).  They may be
# used to invoke the method within the object, and as a block
# associated with an iterator.  They may also be unbound from one
# object (creating an UnboundMethod) and bound to another.
#
#    class Thing
#      def square(n)
#        n*n
#      end
#    end
#    thing = Thing.new
#    meth  = thing.method(:square)
#
#    meth.call(9)                 #=> 81
#    [ 1, 2, 3 ].collect(&meth)   #=> [1, 4, 9]
#
#    [ 1, 2, 3 ].each(&method(:puts)) #=> prints 1, 2, 3
#
#    require 'date'
#    %w[2017-03-01 2017-03-02].collect(&Date.method(:parse))
#    #=> [#<Date: 2017-03-01 ((2457814j,0s,0n),+0s,2299161j)>, #<Date: 2017-03-02 ((2457815j,0s,0n),+0s,2299161j)>]
class Method
  # Returns a proc that is the composition of this method and the given <i>g</i>.
  # The returned proc takes a variable number of arguments, calls <i>g</i> with them
  # then calls this method with the result.
  #
  #    def f(x)
  #      x * x
  #    end
  #
  #    f = self.method(:f)
  #    g = proc {|x| x + x }
  #    p (f << g).call(2) #=> 16
  def <<(g) end

  # Two method objects are equal if they are bound to the same
  # object and refer to the same method definition and the classes
  # defining the methods are the same class or module.
  def ==(other) end
  alias eql? ==

  # Returns a proc that is the composition of this method and the given <i>g</i>.
  # The returned proc takes a variable number of arguments, calls this method
  # with them then calls <i>g</i> with the result.
  #
  #    def f(x)
  #      x * x
  #    end
  #
  #    f = self.method(:f)
  #    g = proc {|x| x + x }
  #    p (f >> g).call(2) #=> 8
  def >>(other) end

  # Returns an indication of the number of arguments accepted by a
  # method. Returns a nonnegative integer for methods that take a fixed
  # number of arguments. For Ruby methods that take a variable number of
  # arguments, returns -n-1, where n is the number of required arguments.
  # Keyword arguments will be considered as a single additional argument,
  # that argument being mandatory if any keyword argument is mandatory.
  # For methods written in C, returns -1 if the call takes a
  # variable number of arguments.
  #
  #    class C
  #      def one;    end
  #      def two(a); end
  #      def three(*a);  end
  #      def four(a, b); end
  #      def five(a, b, *c);    end
  #      def six(a, b, *c, &d); end
  #      def seven(a, b, x:0); end
  #      def eight(x:, y:); end
  #      def nine(x:, y:, **z); end
  #      def ten(*a, x:, y:); end
  #    end
  #    c = C.new
  #    c.method(:one).arity     #=> 0
  #    c.method(:two).arity     #=> 1
  #    c.method(:three).arity   #=> -1
  #    c.method(:four).arity    #=> 2
  #    c.method(:five).arity    #=> -3
  #    c.method(:six).arity     #=> -3
  #    c.method(:seven).arity   #=> -3
  #    c.method(:eight).arity   #=> 1
  #    c.method(:nine).arity    #=> 1
  #    c.method(:ten).arity     #=> -2
  #
  #    "cat".method(:size).arity      #=> 0
  #    "cat".method(:replace).arity   #=> 1
  #    "cat".method(:squeeze).arity   #=> -1
  #    "cat".method(:count).arity     #=> -1
  def arity; end

  # Invokes the <i>meth</i> with the specified arguments, returning the
  # method's return value.
  #
  #    m = 12.method("+")
  #    m.call(3)    #=> 15
  #    m.call(20)   #=> 32
  def call(*args) end
  alias === call
  alias [] call

  # Returns a clone of this method.
  #
  #   class A
  #     def foo
  #       return "bar"
  #     end
  #   end
  #
  #   m = A.new.method(:foo)
  #   m.call # => "bar"
  #   n = m.clone.call # => "bar"
  def clone; end

  # Returns a curried proc based on the method. When the proc is called with a number of
  # arguments that is lower than the method's arity, then another curried proc is returned.
  # Only when enough arguments have been supplied to satisfy the method signature, will the
  # method actually be called.
  #
  # The optional <i>arity</i> argument should be supplied when currying methods with
  # variable arguments to determine how many arguments are needed before the method is
  # called.
  #
  #    def foo(a,b,c)
  #      [a, b, c]
  #    end
  #
  #    proc  = self.method(:foo).curry
  #    proc2 = proc.call(1, 2)          #=> #<Proc>
  #    proc2.call(3)                    #=> [1,2,3]
  #
  #    def vararg(*args)
  #      args
  #    end
  #
  #    proc = self.method(:vararg).curry(4)
  #    proc2 = proc.call(:x)      #=> #<Proc>
  #    proc3 = proc2.call(:y, :z) #=> #<Proc>
  #    proc3.call(:a)             #=> [:x, :y, :z, :a]
  def curry(...) end

  # Returns a hash value corresponding to the method object.
  #
  # See also Object#hash.
  def hash; end

  # Returns a human-readable description of the underlying method.
  #
  #   "cat".method(:count).inspect   #=> "#<Method: String#count(*)>"
  #   (1..3).method(:map).inspect    #=> "#<Method: Range(Enumerable)#map()>"
  #
  # In the latter case, the method description includes the "owner" of the
  # original method (+Enumerable+ module, which is included into +Range+).
  #
  # +inspect+ also provides, when possible, method argument names (call
  # sequence) and source location.
  #
  #   require 'net/http'
  #   Net::HTTP.method(:get).inspect
  #   #=> "#<Method: Net::HTTP.get(uri_or_host, path=..., port=...) <skip>/lib/ruby/2.7.0/net/http.rb:457>"
  #
  # <code>...</code> in argument definition means argument is optional (has
  # some default value).
  #
  # For methods defined in C (language core and extensions), location and
  # argument names can't be extracted, and only generic information is provided
  # in form of <code>*</code> (any number of arguments) or <code>_</code> (some
  # positional argument).
  #
  #   "cat".method(:count).inspect   #=> "#<Method: String#count(*)>"
  #   "cat".method(:+).inspect       #=> "#<Method: String#+(_)>""
  def inspect; end
  alias to_s inspect

  # Returns the name of the method.
  def name; end

  # Returns the original name of the method.
  #
  #   class C
  #     def foo; end
  #     alias bar foo
  #   end
  #   C.instance_method(:bar).original_name # => :foo
  def original_name; end

  # Returns the class or module on which this method is defined.
  # In other words,
  #
  #   meth.owner.instance_methods(false).include?(meth.name) # => true
  #
  # holds as long as the method is not removed/undefined/replaced,
  # (with private_instance_methods instead of instance_methods if the method
  # is private).
  #
  # See also Method#receiver.
  #
  #   (1..3).method(:map).owner #=> Enumerable
  def owner; end

  # Returns the parameter information of this method.
  #
  #    def foo(bar); end
  #    method(:foo).parameters #=> [[:req, :bar]]
  #
  #    def foo(bar, baz, bat, &blk); end
  #    method(:foo).parameters #=> [[:req, :bar], [:req, :baz], [:req, :bat], [:block, :blk]]
  #
  #    def foo(bar, *args); end
  #    method(:foo).parameters #=> [[:req, :bar], [:rest, :args]]
  #
  #    def foo(bar, baz, *args, &blk); end
  #    method(:foo).parameters #=> [[:req, :bar], [:req, :baz], [:rest, :args], [:block, :blk]]
  def parameters; end

  # Returns whether the method is private.
  def private?; end

  # Returns whether the method is protected.
  def protected?; end

  # Returns whether the method is public.
  def public?; end

  # Returns the bound receiver of the method object.
  #
  #   (1..3).method(:map).receiver # => 1..3
  def receiver; end

  # Returns the Ruby source filename and line number containing this method
  # or nil if this method was not defined in Ruby (i.e. native).
  def source_location; end

  # Returns a Method of superclass which would be called when super is used
  # or nil if there is no method on superclass.
  def super_method; end

  # Returns a Proc object corresponding to this method.
  def to_proc; end

  # Dissociates <i>meth</i> from its current receiver. The resulting
  # UnboundMethod can subsequently be bound to a new object of the
  # same class (see UnboundMethod).
  def unbind; end
end
