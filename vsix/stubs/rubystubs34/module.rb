# frozen_string_literal: true

# A Module is a collection of methods and constants. The
# methods in a module may be instance methods or module methods.
# Instance methods appear as methods in a class when the module is
# included, module methods do not. Conversely, module methods may be
# called without creating an encapsulating object, while instance
# methods may not. (See Module#module_function.)
#
# In the descriptions that follow, the parameter <i>sym</i> refers
# to a symbol, which is either a quoted string or a
# Symbol (such as <code>:name</code>).
#
#    module Mod
#      include Math
#      CONST = 1
#      def meth
#        #  ...
#      end
#    end
#    Mod.class              #=> Module
#    Mod.constants          #=> [:CONST, :PI, :E]
#    Mod.instance_methods   #=> [:meth]
class Module
  # In the first form, returns an array of the names of all
  # constants accessible from the point of call.
  # This list includes the names of all modules and classes
  # defined in the global scope.
  #
  #    Module.constants.first(4)
  #       # => [:ARGF, :ARGV, :ArgumentError, :Array]
  #
  #    Module.constants.include?(:SEEK_SET)   # => false
  #
  #    class IO
  #      Module.constants.include?(:SEEK_SET) # => true
  #    end
  #
  # The second form calls the instance method +constants+.
  def self.constants(...) end

  # Returns the list of +Modules+ nested at the point of call.
  #
  #    module M1
  #      module M2
  #        $a = Module.nesting
  #      end
  #    end
  #    $a           #=> [M1::M2, M1]
  #    $a[0].name   #=> "M1::M2"
  def self.nesting; end

  # Returns an array of all modules used in the current scope. The ordering
  # of modules in the resulting array is not defined.
  #
  #    module A
  #      refine Object do
  #      end
  #    end
  #
  #    module B
  #      refine Object do
  #      end
  #    end
  #
  #    using A
  #    using B
  #    p Module.used_modules
  #
  # <em>produces:</em>
  #
  #    [B, A]
  def self.used_modules; end

  # Returns an array of all modules used in the current scope. The ordering
  # of modules in the resulting array is not defined.
  #
  #    module A
  #      refine Object do
  #      end
  #    end
  #
  #    module B
  #      refine Object do
  #      end
  #    end
  #
  #    using A
  #    using B
  #    p Module.used_refinements
  #
  # <em>produces:</em>
  #
  #    [#<refinement:Object@B>, #<refinement:Object@A>]
  def self.used_refinements; end

  # Creates a new anonymous module. If a block is given, it is passed
  # the module object, and the block is evaluated in the context of this
  # module like #module_eval.
  #
  #    fred = Module.new do
  #      def meth1
  #        "hello"
  #      end
  #      def meth2
  #        "bye"
  #      end
  #    end
  #    a = "my string"
  #    a.extend(fred)   #=> "my string"
  #    a.meth1          #=> "hello"
  #    a.meth2          #=> "bye"
  #
  # Assign the module to a constant (name starting uppercase) if you
  # want to treat it like a regular module.
  def initialize; end

  # Returns true if <i>mod</i> is a subclass of <i>other</i>. Returns
  # <code>false</code> if <i>mod</i> is the same as <i>other</i>
  # or <i>mod</i> is an ancestor of <i>other</i>.
  # Returns <code>nil</code> if there's no relationship between the two.
  # (Think of the relationship in terms of the class definition:
  # "class A < B" implies "A < B".)
  def <(other) end

  # Returns true if <i>mod</i> is a subclass of <i>other</i> or
  # is the same as <i>other</i>. Returns
  # <code>nil</code> if there's no relationship between the two.
  # (Think of the relationship in terms of the class definition:
  # "class A < B" implies "A < B".)
  def <=(other) end

  # Comparison---Returns -1, 0, +1 or nil depending on whether +module+
  # includes +other_module+, they are the same, or if +module+ is included by
  # +other_module+.
  #
  # Returns +nil+ if +module+ has no relationship with +other_module+, if
  # +other_module+ is not a module, or if the two values are incomparable.
  def <=>(other) end

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

  # Case Equality---Returns <code>true</code> if <i>obj</i> is an
  # instance of <i>mod</i> or an instance of one of <i>mod</i>'s descendants.
  # Of limited use for modules, but can be used in <code>case</code> statements
  # to classify objects by class.
  def ===(obj) end

  # Returns true if <i>mod</i> is an ancestor of <i>other</i>. Returns
  # <code>false</code> if <i>mod</i> is the same as <i>other</i>
  # or <i>mod</i> is a descendant of <i>other</i>.
  # Returns <code>nil</code> if there's no relationship between the two.
  # (Think of the relationship in terms of the class definition:
  # "class A < B" implies "B > A".)
  def >(other) end

  # Returns true if <i>mod</i> is an ancestor of <i>other</i>, or the
  # two modules are the same. Returns
  # <code>nil</code> if there's no relationship between the two.
  # (Think of the relationship in terms of the class definition:
  # "class A < B" implies "B > A".)
  def >=(other) end

  # Makes <i>new_name</i> a new copy of the method <i>old_name</i>. This can
  # be used to retain access to methods that are overridden.
  #
  #    module Mod
  #      alias_method :orig_exit, :exit #=> :orig_exit
  #      def exit(code=0)
  #        puts "Exiting with code #{code}"
  #        orig_exit(code)
  #      end
  #    end
  #    include Mod
  #    exit(99)
  #
  # <em>produces:</em>
  #
  #    Exiting with code 99
  def alias_method(new_name, old_name) end

  # Returns a list of modules included/prepended in <i>mod</i>
  # (including <i>mod</i> itself).
  #
  #    module Mod
  #      include Math
  #      include Comparable
  #      prepend Enumerable
  #    end
  #
  #    Mod.ancestors        #=> [Enumerable, Mod, Comparable, Math]
  #    Math.ancestors       #=> [Math]
  #    Enumerable.ancestors #=> [Enumerable]
  def ancestors; end

  # The first form is equivalent to #attr_reader.
  # The second form is equivalent to <code>attr_accessor(name)</code> but deprecated.
  # The last form is equivalent to <code>attr_reader(name)</code> but deprecated.
  # Returns an array of defined method names as symbols.
  def attr(...) end

  # Defines a named attribute for this module, where the name is
  # <i>symbol.</i><code>id2name</code>, creating an instance variable
  # (<code>@name</code>) and a corresponding access method to read it.
  # Also creates a method called <code>name=</code> to set the attribute.
  # String arguments are converted to symbols.
  # Returns an array of defined method names as symbols.
  #
  #    module Mod
  #      attr_accessor(:one, :two) #=> [:one, :one=, :two, :two=]
  #    end
  #    Mod.instance_methods.sort   #=> [:one, :one=, :two, :two=]
  def attr_accessor(...) end

  # Creates instance variables and corresponding methods that return the
  # value of each instance variable. Equivalent to calling
  # ``<code>attr</code><i>:name</i>'' on each name in turn.
  # String arguments are converted to symbols.
  # Returns an array of defined method names as symbols.
  def attr_reader(...) end

  # Creates an accessor method to allow assignment to the attribute
  # <i>symbol</i><code>.id2name</code>.
  # String arguments are converted to symbols.
  # Returns an array of defined method names as symbols.
  def attr_writer(...) end

  # Registers _filename_ to be loaded (using Kernel::require)
  # the first time that _const_ (which may be a String or
  # a symbol) is accessed in the namespace of _mod_.
  #
  #    module A
  #    end
  #    A.autoload(:B, "b")
  #    A::B.doit            # autoloads "b"
  #
  # If _const_ in _mod_ is defined as autoload, the file name to be
  # loaded is replaced with _filename_.  If _const_ is defined but not
  # as autoload, does nothing.
  def autoload(const, filename) end

  # Returns _filename_ to be loaded if _name_ is registered as
  # +autoload+ in the namespace of _mod_ or one of its ancestors.
  #
  #    module A
  #    end
  #    A.autoload(:B, "b")
  #    A.autoload?(:B)            #=> "b"
  #
  # If +inherit+ is false, the lookup only checks the autoloads in the receiver:
  #
  #    class A
  #      autoload :CONST, "const.rb"
  #    end
  #
  #    class B < A
  #    end
  #
  #    B.autoload?(:CONST)          #=> "const.rb", found in A (ancestor)
  #    B.autoload?(:CONST, false)   #=> nil, not found in B itself
  def autoload?(name, inherit = true) end

  # Returns <code>true</code> if the given class variable is defined
  # in <i>obj</i>.
  # String arguments are converted to symbols.
  #
  #    class Fred
  #      @@foo = 99
  #    end
  #    Fred.class_variable_defined?(:@@foo)    #=> true
  #    Fred.class_variable_defined?(:@@bar)    #=> false
  def class_variable_defined?(...) end

  # Returns the value of the given class variable (or throws a
  # NameError exception). The <code>@@</code> part of the
  # variable name should be included for regular class variables.
  # String arguments are converted to symbols.
  #
  #    class Fred
  #      @@foo = 99
  #    end
  #    Fred.class_variable_get(:@@foo)     #=> 99
  def class_variable_get(...) end

  # Sets the class variable named by <i>symbol</i> to the given
  # object.
  # If the class variable name is passed as a string, that string
  # is converted to a symbol.
  #
  #    class Fred
  #      @@foo = 99
  #      def foo
  #        @@foo
  #      end
  #    end
  #    Fred.class_variable_set(:@@foo, 101)     #=> 101
  #    Fred.new.foo                             #=> 101
  def class_variable_set(...) end

  # Returns an array of the names of class variables in <i>mod</i>.
  # This includes the names of class variables in any included
  # modules, unless the <i>inherit</i> parameter is set to
  # <code>false</code>.
  #
  #    class One
  #      @@var1 = 1
  #    end
  #    class Two < One
  #      @@var2 = 2
  #    end
  #    One.class_variables          #=> [:@@var1]
  #    Two.class_variables          #=> [:@@var2, :@@var1]
  #    Two.class_variables(false)   #=> [:@@var2]
  def class_variables(inherit = true) end

  # Says whether _mod_ or its ancestors have a constant with the given name:
  #
  #   Float.const_defined?(:EPSILON)      #=> true, found in Float itself
  #   Float.const_defined?("String")      #=> true, found in Object (ancestor)
  #   BasicObject.const_defined?(:Hash)   #=> false
  #
  # If _mod_ is a +Module+, additionally +Object+ and its ancestors are checked:
  #
  #   Math.const_defined?(:String)   #=> true, found in Object
  #
  # In each of the checked classes or modules, if the constant is not present
  # but there is an autoload for it, +true+ is returned directly without
  # autoloading:
  #
  #   module Admin
  #     autoload :User, 'admin/user'
  #   end
  #   Admin.const_defined?(:User)   #=> true
  #
  # If the constant is not found the callback +const_missing+ is *not* called
  # and the method returns +false+.
  #
  # If +inherit+ is false, the lookup only checks the constants in the receiver:
  #
  #   IO.const_defined?(:SYNC)          #=> true, found in File::Constants (ancestor)
  #   IO.const_defined?(:SYNC, false)   #=> false, not found in IO itself
  #
  # In this case, the same logic for autoloading applies.
  #
  # If the argument is not a valid constant name a +NameError+ is raised with the
  # message "wrong constant name _name_":
  #
  #   Hash.const_defined? 'foobar'   #=> NameError: wrong constant name foobar
  def const_defined?(...) end

  # Checks for a constant with the given name in <i>mod</i>.
  # If +inherit+ is set, the lookup will also search
  # the ancestors (and +Object+ if <i>mod</i> is a +Module+).
  #
  # The value of the constant is returned if a definition is found,
  # otherwise a +NameError+ is raised.
  #
  #    Math.const_get(:PI)   #=> 3.14159265358979
  #
  # This method will recursively look up constant names if a namespaced
  # class name is provided.  For example:
  #
  #    module Foo; class Bar; end end
  #    Object.const_get 'Foo::Bar'
  #
  # The +inherit+ flag is respected on each lookup.  For example:
  #
  #    module Foo
  #      class Bar
  #        VAL = 10
  #      end
  #
  #      class Baz < Bar; end
  #    end
  #
  #    Object.const_get 'Foo::Baz::VAL'         # => 10
  #    Object.const_get 'Foo::Baz::VAL', false  # => NameError
  #
  # If the argument is not a valid constant name a +NameError+ will be
  # raised with a warning "wrong constant name".
  #
  #     Object.const_get 'foobar' #=> NameError: wrong constant name foobar
  def const_get(...) end

  # Invoked when a reference is made to an undefined constant in
  # <i>mod</i>. It is passed a symbol for the undefined constant, and
  # returns a value to be used for that constant. For example, consider:
  #
  #   def Foo.const_missing(name)
  #     name # return the constant name as Symbol
  #   end
  #
  #   Foo::UNDEFINED_CONST    #=> :UNDEFINED_CONST: symbol returned
  #
  # As the example above shows, +const_missing+ is not required to create the
  # missing constant in <i>mod</i>, though that is often a side-effect. The
  # caller gets its return value when triggered. If the constant is also defined,
  # further lookups won't hit +const_missing+ and will return the value stored in
  # the constant as usual. Otherwise, +const_missing+ will be invoked again.
  #
  # In the next example, when a reference is made to an undefined constant,
  # +const_missing+ attempts to load a file whose path is the lowercase version
  # of the constant name (thus class <code>Fred</code> is assumed to be in file
  # <code>fred.rb</code>). If defined as a side-effect of loading the file, the
  # method returns the value stored in the constant. This implements an autoload
  # feature similar to Kernel#autoload and Module#autoload, though it differs in
  # important ways.
  #
  #   def Object.const_missing(name)
  #     @looked_for ||= {}
  #     str_name = name.to_s
  #     raise "Constant not found: #{name}" if @looked_for[str_name]
  #     @looked_for[str_name] = 1
  #     file = str_name.downcase
  #     require file
  #     const_get(name, false)
  #   end
  def const_missing(sym) end

  # Sets the named constant to the given object, returning that object.
  # Creates a new constant if no constant with the given name previously
  # existed.
  #
  #    Math.const_set("HIGH_SCHOOL_PI", 22.0/7.0)   #=> 3.14285714285714
  #    Math::HIGH_SCHOOL_PI - Math::PI              #=> 0.00126448926734968
  #
  # If +sym+ or +str+ is not a valid constant name a +NameError+ will be
  # raised with a warning "wrong constant name".
  #
  #     Object.const_set('foobar', 42) #=> NameError: wrong constant name foobar
  def const_set(...) end

  # Returns the Ruby source filename and line number containing the definition
  # of the constant specified. If the named constant is not found, +nil+ is returned.
  # If the constant is found, but its source location can not be extracted
  # (constant is defined in C code), empty array is returned.
  #
  # _inherit_ specifies whether to lookup in <code>mod.ancestors</code> (+true+
  # by default).
  #
  #     # test.rb:
  #     class A         # line 1
  #       C1 = 1
  #       C2 = 2
  #     end
  #
  #     module M        # line 6
  #       C3 = 3
  #     end
  #
  #     class B < A     # line 10
  #       include M
  #       C4 = 4
  #     end
  #
  #     class A # continuation of A definition
  #       C2 = 8 # constant redefinition; warned yet allowed
  #     end
  #
  #     p B.const_source_location('C4')           # => ["test.rb", 12]
  #     p B.const_source_location('C3')           # => ["test.rb", 7]
  #     p B.const_source_location('C1')           # => ["test.rb", 2]
  #
  #     p B.const_source_location('C3', false)    # => nil  -- don't lookup in ancestors
  #
  #     p A.const_source_location('C2')           # => ["test.rb", 16] -- actual (last) definition place
  #
  #     p Object.const_source_location('B')       # => ["test.rb", 10] -- top-level constant could be looked through Object
  #     p Object.const_source_location('A')       # => ["test.rb", 1] -- class reopening is NOT considered new definition
  #
  #     p B.const_source_location('A')            # => ["test.rb", 1]  -- because Object is in ancestors
  #     p M.const_source_location('A')            # => ["test.rb", 1]  -- Object is not ancestor, but additionally checked for modules
  #
  #     p Object.const_source_location('A::C1')   # => ["test.rb", 2]  -- nesting is supported
  #     p Object.const_source_location('String')  # => []  -- constant is defined in C code
  def const_source_location(...) end

  # Returns an array of the names of the constants accessible in
  # <i>mod</i>. This includes the names of constants in any included
  # modules (example at start of section), unless the <i>inherit</i>
  # parameter is set to <code>false</code>.
  #
  # The implementation makes no guarantees about the order in which the
  # constants are yielded.
  #
  #   IO.constants.include?(:SYNC)        #=> true
  #   IO.constants(false).include?(:SYNC) #=> false
  #
  # Also see Module#const_defined?.
  def constants(inherit = true) end

  # Defines an instance method in the receiver. The _method_
  # parameter can be a +Proc+, a +Method+ or an +UnboundMethod+ object.
  # If a block is specified, it is used as the method body.
  # If a block or the _method_ parameter has parameters,
  # they're used as method parameters.
  # This block is evaluated using #instance_eval.
  #
  #    class A
  #      def fred
  #        puts "In Fred"
  #      end
  #      def create_method(name, &block)
  #        self.class.define_method(name, &block)
  #      end
  #      define_method(:wilma) { puts "Charge it!" }
  #      define_method(:flint) {|name| puts "I'm #{name}!"}
  #    end
  #    class B < A
  #      define_method(:barney, instance_method(:fred))
  #    end
  #    a = B.new
  #    a.barney
  #    a.wilma
  #    a.flint('Dino')
  #    a.create_method(:betty) { p self }
  #    a.betty
  #
  # <em>produces:</em>
  #
  #    In Fred
  #    Charge it!
  #    I'm Dino!
  #    #<B:0x401b39e8>
  def define_method(...) end

  # Makes a list of existing constants deprecated. Attempt
  # to refer to them will produce a warning.
  #
  #    module HTTP
  #      NotFound = Exception.new
  #      NOT_FOUND = NotFound # previous version of the library used this name
  #
  #      deprecate_constant :NOT_FOUND
  #    end
  #
  #    HTTP::NOT_FOUND
  #    # warning: constant HTTP::NOT_FOUND is deprecated
  def deprecate_constant(symbol, *args) end

  # Prevents further modifications to <i>mod</i>.
  #
  # This method returns self.
  def freeze; end

  # Invokes Module.append_features on each parameter in reverse order.
  def include(*args) end

  # Returns <code>true</code> if <i>module</i> is included
  # or prepended in <i>mod</i> or one of <i>mod</i>'s ancestors.
  #
  #    module A
  #    end
  #    class B
  #      include A
  #    end
  #    class C < B
  #    end
  #    B.include?(A)   #=> true
  #    C.include?(A)   #=> true
  #    A.include?(A)   #=> false
  def include?(p1) end

  # Returns the list of modules included or prepended in <i>mod</i>
  # or one of <i>mod</i>'s ancestors.
  #
  #    module Sub
  #    end
  #
  #    module Mixin
  #      prepend Sub
  #    end
  #
  #    module Outer
  #      include Mixin
  #    end
  #
  #    Mixin.included_modules   #=> [Sub]
  #    Outer.included_modules   #=> [Sub, Mixin]
  def included_modules; end

  # Returns an +UnboundMethod+ representing the given
  # instance method in _mod_.
  #
  #    class Interpreter
  #      def do_a() print "there, "; end
  #      def do_d() print "Hello ";  end
  #      def do_e() print "!\n";     end
  #      def do_v() print "Dave";    end
  #      Dispatcher = {
  #        "a" => instance_method(:do_a),
  #        "d" => instance_method(:do_d),
  #        "e" => instance_method(:do_e),
  #        "v" => instance_method(:do_v)
  #      }
  #      def interpret(string)
  #        string.each_char {|b| Dispatcher[b].bind(self).call }
  #      end
  #    end
  #
  #    interpreter = Interpreter.new
  #    interpreter.interpret('dave')
  #
  # <em>produces:</em>
  #
  #    Hello there, Dave!
  def instance_method(symbol) end

  # Returns an array containing the names of the public and protected instance
  # methods in the receiver. For a module, these are the public and protected methods;
  # for a class, they are the instance (not singleton) methods. If the optional
  # parameter is <code>false</code>, the methods of any ancestors are not included.
  #
  #    module A
  #      def method1()  end
  #    end
  #    class B
  #      include A
  #      def method2()  end
  #    end
  #    class C < B
  #      def method3()  end
  #    end
  #
  #    A.instance_methods(false)                   #=> [:method1]
  #    B.instance_methods(false)                   #=> [:method2]
  #    B.instance_methods(true).include?(:method1) #=> true
  #    C.instance_methods(false)                   #=> [:method3]
  #    C.instance_methods.include?(:method2)       #=> true
  #
  # Note that method visibility changes in the current class, as well as aliases,
  # are considered as methods of the current class by this method:
  #
  #    class C < B
  #      alias method4 method2
  #      protected :method2
  #    end
  #    C.instance_methods(false).sort               #=> [:method2, :method3, :method4]
  def instance_methods(include_super = true) end

  # Returns +true+ if the named method is defined by
  # _mod_.  If _inherit_ is set, the lookup will also search _mod_'s
  # ancestors. Public and protected methods are matched.
  # String arguments are converted to symbols.
  #
  #    module A
  #      def method1()  end
  #      def protected_method1()  end
  #      protected :protected_method1
  #    end
  #    class B
  #      def method2()  end
  #      def private_method2()  end
  #      private :private_method2
  #    end
  #    class C < B
  #      include A
  #      def method3()  end
  #    end
  #
  #    A.method_defined? :method1              #=> true
  #    C.method_defined? "method1"             #=> true
  #    C.method_defined? "method2"             #=> true
  #    C.method_defined? "method2", true       #=> true
  #    C.method_defined? "method2", false      #=> false
  #    C.method_defined? "method3"             #=> true
  #    C.method_defined? "protected_method1"   #=> true
  #    C.method_defined? "method4"             #=> false
  #    C.method_defined? "private_method2"     #=> false
  def method_defined?(...) end

  # Evaluates the string or block in the context of _mod_, except that when
  # a block is given, constant/class variable lookup is not affected. This
  # can be used to add methods to a class. <code>module_eval</code> returns
  # the result of evaluating its argument. The optional _filename_ and
  # _lineno_ parameters set the text for error messages.
  #
  #    class Thing
  #    end
  #    a = %q{def hello() "Hello there!" end}
  #    Thing.module_eval(a)
  #    puts Thing.new.hello()
  #    Thing.module_eval("invalid code", "dummy", 123)
  #
  # <em>produces:</em>
  #
  #    Hello there!
  #    dummy:123:in `module_eval': undefined local variable
  #        or method `code' for Thing:Class
  def module_eval(...) end
  alias class_eval module_eval

  # Evaluates the given block in the context of the class/module.
  # The method defined in the block will belong to the receiver.
  # Any arguments passed to the method will be passed to the block.
  # This can be used if the block needs to access instance variables.
  #
  #    class Thing
  #    end
  #    Thing.class_exec{
  #      def hello() "Hello there!" end
  #    }
  #    puts Thing.new.hello()
  #
  # <em>produces:</em>
  #
  #    Hello there!
  def module_exec(*args) end
  alias class_exec module_exec

  # Returns the name of the module <i>mod</i>.  Returns +nil+ for anonymous modules.
  def name; end

  # Invokes Module.prepend_features on each parameter in reverse order.
  def prepend(*args) end

  # Makes existing class methods private. Often used to hide the default
  # constructor <code>new</code>.
  #
  # String arguments are converted to symbols.
  # An Array of Symbols and/or Strings is also accepted.
  #
  #    class SimpleSingleton  # Not thread safe
  #      private_class_method :new
  #      def SimpleSingleton.create(*args, &block)
  #        @me = new(*args, &block) if ! @me
  #        @me
  #      end
  #    end
  def private_class_method(...) end

  # Makes a list of existing constants private.
  def private_constant(symbol, *args) end

  # Returns a list of the private instance methods defined in
  # <i>mod</i>. If the optional parameter is <code>false</code>, the
  # methods of any ancestors are not included.
  #
  #    module Mod
  #      def method1()  end
  #      private :method1
  #      def method2()  end
  #    end
  #    Mod.instance_methods           #=> [:method2]
  #    Mod.private_instance_methods   #=> [:method1]
  def private_instance_methods(include_super = true) end

  # Returns +true+ if the named private method is defined by
  # _mod_.  If _inherit_ is set, the lookup will also search _mod_'s
  # ancestors.
  # String arguments are converted to symbols.
  #
  #    module A
  #      def method1()  end
  #    end
  #    class B
  #      private
  #      def method2()  end
  #    end
  #    class C < B
  #      include A
  #      def method3()  end
  #    end
  #
  #    A.method_defined? :method1                   #=> true
  #    C.private_method_defined? "method1"          #=> false
  #    C.private_method_defined? "method2"          #=> true
  #    C.private_method_defined? "method2", true    #=> true
  #    C.private_method_defined? "method2", false   #=> false
  #    C.method_defined? "method2"                  #=> false
  def private_method_defined?(...) end

  # Returns a list of the protected instance methods defined in
  # <i>mod</i>. If the optional parameter is <code>false</code>, the
  # methods of any ancestors are not included.
  def protected_instance_methods(include_super = true) end

  # Returns +true+ if the named protected method is defined
  # _mod_.  If _inherit_ is set, the lookup will also search _mod_'s
  # ancestors.
  # String arguments are converted to symbols.
  #
  #    module A
  #      def method1()  end
  #    end
  #    class B
  #      protected
  #      def method2()  end
  #    end
  #    class C < B
  #      include A
  #      def method3()  end
  #    end
  #
  #    A.method_defined? :method1                    #=> true
  #    C.protected_method_defined? "method1"         #=> false
  #    C.protected_method_defined? "method2"         #=> true
  #    C.protected_method_defined? "method2", true   #=> true
  #    C.protected_method_defined? "method2", false  #=> false
  #    C.method_defined? "method2"                   #=> true
  def protected_method_defined?(...) end

  # Makes a list of existing class methods public.
  #
  # String arguments are converted to symbols.
  # An Array of Symbols and/or Strings is also accepted.
  def public_class_method(...) end

  # Makes a list of existing constants public.
  def public_constant(symbol, *args) end

  # Similar to _instance_method_, searches public method only.
  def public_instance_method(symbol) end

  # Returns a list of the public instance methods defined in <i>mod</i>.
  # If the optional parameter is <code>false</code>, the methods of
  # any ancestors are not included.
  def public_instance_methods(include_super = true) end

  # Returns +true+ if the named public method is defined by
  # _mod_.  If _inherit_ is set, the lookup will also search _mod_'s
  # ancestors.
  # String arguments are converted to symbols.
  #
  #    module A
  #      def method1()  end
  #    end
  #    class B
  #      protected
  #      def method2()  end
  #    end
  #    class C < B
  #      include A
  #      def method3()  end
  #    end
  #
  #    A.method_defined? :method1                 #=> true
  #    C.public_method_defined? "method1"         #=> true
  #    C.public_method_defined? "method1", true   #=> true
  #    C.public_method_defined? "method1", false  #=> true
  #    C.public_method_defined? "method2"         #=> false
  #    C.method_defined? "method2"                #=> true
  def public_method_defined?(...) end

  # Returns an array of +Refinement+ defined within the receiver.
  #
  #    module A
  #      refine Integer do
  #      end
  #
  #      refine String do
  #      end
  #    end
  #
  #    p A.refinements
  #
  # <em>produces:</em>
  #
  #    [#<refinement:Integer@A>, #<refinement:String@A>]
  def refinements; end

  # Removes the named class variable from the receiver, returning that
  # variable's value.
  #
  #    class Example
  #      @@var = 99
  #      puts remove_class_variable(:@@var)
  #      p(defined? @@var)
  #    end
  #
  # <em>produces:</em>
  #
  #    99
  #    nil
  def remove_class_variable(sym) end

  # Removes the method identified by _symbol_ from the current
  # class. For an example, see Module#undef_method.
  # String arguments are converted to symbols.
  def remove_method(...) end

  # Sets the temporary name of the module. This name is reflected in
  # introspection of the module and the values that are related to it, such
  # as instances, constants, and methods.
  #
  # The name should be +nil+ or a non-empty string that is not a valid constant
  # path (to avoid confusing between permanent and temporary names).
  #
  # The method can be useful to distinguish dynamically generated classes and
  # modules without assigning them to constants.
  #
  # If the module is given a permanent name by assigning it to a constant,
  # the temporary name is discarded. A temporary name can't be assigned to
  # modules that have a permanent name.
  #
  # If the given name is +nil+, the module becomes anonymous again.
  #
  # Example:
  #
  #   m = Module.new # => #<Module:0x0000000102c68f38>
  #   m.name #=> nil
  #
  #   m.set_temporary_name("fake_name") # => fake_name
  #   m.name #=> "fake_name"
  #
  #   m.set_temporary_name(nil) # => #<Module:0x0000000102c68f38>
  #   m.name #=> nil
  #
  #   c = Class.new
  #   c.set_temporary_name("MyClass(with description)")
  #
  #   c.new # => #<MyClass(with description):0x0....>
  #
  #   c::M = m
  #   c::M.name #=> "MyClass(with description)::M"
  #
  #   # Assigning to a constant replaces the name with a permanent one
  #   C = c
  #
  #   C.name #=> "C"
  #   C::M.name #=> "C::M"
  #   c.new # => #<C:0x0....>
  def set_temporary_name(...) end

  # Returns <code>true</code> if <i>mod</i> is a singleton class or
  # <code>false</code> if it is an ordinary class or module.
  #
  #    class C
  #    end
  #    C.singleton_class?                  #=> false
  #    C.singleton_class.singleton_class?  #=> true
  def singleton_class?; end

  # Returns a string representing this module or class. For basic
  # classes and modules, this is the name. For singletons, we
  # show information on the thing we're attached to as well.
  def to_s; end
  alias inspect to_s

  # Prevents the current class from responding to calls to the named
  # method. Contrast this with <code>remove_method</code>, which deletes
  # the method from the particular class; Ruby will still search
  # superclasses and mixed-in modules for a possible receiver.
  # String arguments are converted to symbols.
  #
  #    class Parent
  #      def hello
  #        puts "In parent"
  #      end
  #    end
  #    class Child < Parent
  #      def hello
  #        puts "In child"
  #      end
  #    end
  #
  #    c = Child.new
  #    c.hello
  #
  #    class Child
  #      remove_method :hello  # remove from child, still in parent
  #    end
  #    c.hello
  #
  #    class Child
  #      undef_method :hello   # prevent any calls to 'hello'
  #    end
  #    c.hello
  #
  # <em>produces:</em>
  #
  #    In child
  #    In parent
  #    prog.rb:23: undefined method 'hello' for #<Child:0x401b3bb4> (NoMethodError)
  def undef_method(...) end

  # Returns a list of the undefined instance methods defined in <i>mod</i>.
  # The undefined methods of any ancestors are not included.
  def undefined_instance_methods; end

  private

  # When this module is included in another, Ruby calls
  # #append_features in this module, passing it the receiving module
  # in _mod_. Ruby's default implementation is to add the constants,
  # methods, and module variables of this module to _mod_ if this
  # module has not already been added to _mod_ or one of its
  # ancestors. See also Module#include.
  def append_features(mod) end

  # Invoked as a callback whenever a constant is assigned on the receiver
  #
  #   module Chatty
  #     def self.const_added(const_name)
  #       super
  #       puts "Added #{const_name.inspect}"
  #     end
  #     FOO = 1
  #   end
  #
  # <em>produces:</em>
  #
  #   Added :FOO
  def const_added(const_name) end

  # Extends the specified object by adding this module's constants and
  # methods (which are added as singleton methods). This is the callback
  # method used by Object#extend.
  #
  #    module Picky
  #      def Picky.extend_object(o)
  #        if String === o
  #          puts "Can't add Picky to a String"
  #        else
  #          puts "Picky added to #{o.class}"
  #          super
  #        end
  #      end
  #    end
  #    (s = Array.new).extend Picky  # Call Object.extend
  #    (s = "quick brown fox").extend Picky
  #
  # <em>produces:</em>
  #
  #    Picky added to Array
  #    Can't add Picky to a String
  def extend_object(obj) end

  # The equivalent of <tt>included</tt>, but for extended modules.
  #
  #        module A
  #          def self.extended(mod)
  #            puts "#{self} extended in #{mod}"
  #          end
  #        end
  #        module Enumerable
  #          extend A
  #        end
  #         # => prints "A extended in Enumerable"
  def extended(othermod) end

  # Callback invoked whenever the receiver is included in another
  # module or class. This should be used in preference to
  # <tt>Module.append_features</tt> if your code wants to perform some
  # action when a module is included in another.
  #
  #        module A
  #          def A.included(mod)
  #            puts "#{self} included in #{mod}"
  #          end
  #        end
  #        module Enumerable
  #          include A
  #        end
  #         # => prints "A included in Enumerable"
  def included(othermod) end

  # Invoked as a callback whenever an instance method is added to the
  # receiver.
  #
  #   module Chatty
  #     def self.method_added(method_name)
  #       puts "Adding #{method_name.inspect}"
  #     end
  #     def self.some_class_method() end
  #     def some_instance_method() end
  #   end
  #
  # <em>produces:</em>
  #
  #   Adding :some_instance_method
  def method_added(method_name) end

  # Invoked as a callback whenever an instance method is removed from the
  # receiver.
  #
  #   module Chatty
  #     def self.method_removed(method_name)
  #       puts "Removing #{method_name.inspect}"
  #     end
  #     def self.some_class_method() end
  #     def some_instance_method() end
  #     class << self
  #       remove_method :some_class_method
  #     end
  #     remove_method :some_instance_method
  #   end
  #
  # <em>produces:</em>
  #
  #   Removing :some_instance_method
  def method_removed(method_name) end

  # Invoked as a callback whenever an instance method is undefined from the
  # receiver.
  #
  #   module Chatty
  #     def self.method_undefined(method_name)
  #       puts "Undefining #{method_name.inspect}"
  #     end
  #     def self.some_class_method() end
  #     def some_instance_method() end
  #     class << self
  #       undef_method :some_class_method
  #     end
  #     undef_method :some_instance_method
  #   end
  #
  # <em>produces:</em>
  #
  #   Undefining :some_instance_method
  def method_undefined(method_name) end

  # Creates module functions for the named methods. These functions may
  # be called with the module as a receiver, and also become available
  # as instance methods to classes that mix in the module. Module
  # functions are copies of the original, and so may be changed
  # independently. The instance-method versions are made private. If
  # used with no arguments, subsequently defined methods become module
  # functions.
  # String arguments are converted to symbols.
  # If a single argument is passed, it is returned.
  # If no argument is passed, nil is returned.
  # If multiple arguments are passed, the arguments are returned as an array.
  #
  #    module Mod
  #      def one
  #        "This is one"
  #      end
  #      module_function :one
  #    end
  #    class Cls
  #      include Mod
  #      def call_one
  #        one
  #      end
  #    end
  #    Mod.one     #=> "This is one"
  #    c = Cls.new
  #    c.call_one  #=> "This is one"
  #    module Mod
  #      def one
  #        "This is the new one"
  #      end
  #    end
  #    Mod.one     #=> "This is one"
  #    c.call_one  #=> "This is the new one"
  def module_function(...) end

  # When this module is prepended in another, Ruby calls
  # #prepend_features in this module, passing it the receiving module
  # in _mod_. Ruby's default implementation is to overlay the
  # constants, methods, and module variables of this module to _mod_
  # if this module has not already been added to _mod_ or one of its
  # ancestors. See also Module#prepend.
  def prepend_features(mod) end

  # The equivalent of <tt>included</tt>, but for prepended modules.
  #
  #        module A
  #          def self.prepended(mod)
  #            puts "#{self} prepended to #{mod}"
  #          end
  #        end
  #        module Enumerable
  #          prepend A
  #        end
  #         # => prints "A prepended to Enumerable"
  def prepended(othermod) end

  # With no arguments, sets the default visibility for subsequently
  # defined methods to private. With arguments, sets the named methods
  # to have private visibility.
  # String arguments are converted to symbols.
  # An Array of Symbols and/or Strings is also accepted.
  # If a single argument is passed, it is returned.
  # If no argument is passed, nil is returned.
  # If multiple arguments are passed, the arguments are returned as an array.
  #
  #    module Mod
  #      def a()  end
  #      def b()  end
  #      private
  #      def c()  end
  #      private :a
  #    end
  #    Mod.private_instance_methods   #=> [:a, :c]
  #
  # Note that to show a private method on RDoc, use <code>:doc:</code>.
  def private(...) end

  # With no arguments, sets the default visibility for subsequently
  # defined methods to protected. With arguments, sets the named methods
  # to have protected visibility.
  # String arguments are converted to symbols.
  # An Array of Symbols and/or Strings is also accepted.
  # If a single argument is passed, it is returned.
  # If no argument is passed, nil is returned.
  # If multiple arguments are passed, the arguments are returned as an array.
  #
  # If a method has protected visibility, it is callable only where
  # <code>self</code> of the context is the same as the method.
  # (method definition or instance_eval). This behavior is different from
  # Java's protected method. Usually <code>private</code> should be used.
  #
  # Note that a protected method is slow because it can't use inline cache.
  #
  # To show a private method on RDoc, use <code>:doc:</code> instead of this.
  def protected(...) end

  # With no arguments, sets the default visibility for subsequently
  # defined methods to public. With arguments, sets the named methods to
  # have public visibility.
  # String arguments are converted to symbols.
  # An Array of Symbols and/or Strings is also accepted.
  # If a single argument is passed, it is returned.
  # If no argument is passed, nil is returned.
  # If multiple arguments are passed, the arguments are returned as an array.
  def public(...) end

  # Refine <i>mod</i> in the receiver.
  #
  # Returns a module, where refined methods are defined.
  def refine(mod) end

  # Removes the definition of the given constant, returning that
  # constant's previous value.  If that constant referred to
  # a module, this will not change that module's name and can lead
  # to confusion.
  def remove_const(sym) end

  # For the given method names, marks the method as passing keywords through
  # a normal argument splat.  This should only be called on methods that
  # accept an argument splat (<tt>*args</tt>) but not explicit keywords or
  # a keyword splat.  It marks the method such that if the method is called
  # with keyword arguments, the final hash argument is marked with a special
  # flag such that if it is the final element of a normal argument splat to
  # another method call, and that method call does not include explicit
  # keywords or a keyword splat, the final element is interpreted as keywords.
  # In other words, keywords will be passed through the method to other
  # methods.
  #
  # This should only be used for methods that delegate keywords to another
  # method, and only for backwards compatibility with Ruby versions before 3.0.
  # See https://www.ruby-lang.org/en/news/2019/12/12/separation-of-positional-and-keyword-arguments-in-ruby-3-0/
  # for details on why +ruby2_keywords+ exists and when and how to use it.
  #
  # This method will probably be removed at some point, as it exists only
  # for backwards compatibility. As it does not exist in Ruby versions before
  # 2.7, check that the module responds to this method before calling it:
  #
  #   module Mod
  #     def foo(meth, *args, &block)
  #       send(:"do_#{meth}", *args, &block)
  #     end
  #     ruby2_keywords(:foo) if respond_to?(:ruby2_keywords, true)
  #   end
  #
  # However, be aware that if the +ruby2_keywords+ method is removed, the
  # behavior of the +foo+ method using the above approach will change so that
  # the method does not pass through keywords.
  def ruby2_keywords(method_name, *args) end

  # Import class refinements from <i>module</i> into the current class or
  # module definition.
  def using(p1) end
end
