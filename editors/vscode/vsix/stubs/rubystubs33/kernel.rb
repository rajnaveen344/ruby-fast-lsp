# frozen_string_literal: true

# The Kernel module is included by class Object, so its methods are
# available in every Ruby object.
#
# The Kernel instance methods are documented in class Object while the
# module methods are documented here.  These methods are called without a
# receiver and thus can be called in functional form:
#
#   sprintf "%.1f", 1.234 #=> "1.2"
#
# == What's Here
#
# \Module \Kernel provides methods that are useful for:
#
# - {Converting}[rdoc-ref:Kernel@Converting]
# - {Querying}[rdoc-ref:Kernel@Querying]
# - {Exiting}[rdoc-ref:Kernel@Exiting]
# - {Exceptions}[rdoc-ref:Kernel@Exceptions]
# - {IO}[rdoc-ref:Kernel@IO]
# - {Procs}[rdoc-ref:Kernel@Procs]
# - {Tracing}[rdoc-ref:Kernel@Tracing]
# - {Subprocesses}[rdoc-ref:Kernel@Subprocesses]
# - {Loading}[rdoc-ref:Kernel@Loading]
# - {Yielding}[rdoc-ref:Kernel@Yielding]
# - {Random Values}[rdoc-ref:Kernel@Random+Values]
# - {Other}[rdoc-ref:Kernel@Other]
#
# === Converting
#
# - #Array: Returns an Array based on the given argument.
# - #Complex: Returns a Complex based on the given arguments.
# - #Float: Returns a Float based on the given arguments.
# - #Hash: Returns a Hash based on the given argument.
# - #Integer: Returns an Integer based on the given arguments.
# - #Rational: Returns a Rational based on the given arguments.
# - #String: Returns a String based on the given argument.
#
# === Querying
#
# - #__callee__: Returns the called name of the current method as a symbol.
# - #__dir__: Returns the path to the directory from which the current
#   method is called.
# - #__method__: Returns the name of the current method as a symbol.
# - #autoload?: Returns the file to be loaded when the given module is referenced.
# - #binding: Returns a Binding for the context at the point of call.
# - #block_given?: Returns +true+ if a block was passed to the calling method.
# - #caller: Returns the current execution stack as an array of strings.
# - #caller_locations: Returns the current execution stack as an array
#   of Thread::Backtrace::Location objects.
# - #class: Returns the class of +self+.
# - #frozen?: Returns whether +self+ is frozen.
# - #global_variables: Returns an array of global variables as symbols.
# - #local_variables: Returns an array of local variables as symbols.
# - #test: Performs specified tests on the given single file or pair of files.
#
# === Exiting
#
# - #abort: Exits the current process after printing the given arguments.
# - #at_exit: Executes the given block when the process exits.
# - #exit: Exits the current process after calling any registered
#   +at_exit+ handlers.
# - #exit!: Exits the current process without calling any registered
#   +at_exit+ handlers.
#
# === Exceptions
#
# - #catch: Executes the given block, possibly catching a thrown object.
# - #raise (aliased as #fail): Raises an exception based on the given arguments.
# - #throw: Returns from the active catch block waiting for the given tag.
#
# === \IO
#
# - ::pp: Prints the given objects in pretty form.
# - #gets: Returns and assigns to <tt>$_</tt> the next line from the current input.
# - #open: Creates an IO object connected to the given stream, file, or subprocess.
# - #p:  Prints the given objects' inspect output to the standard output.
# - #print: Prints the given objects to standard output without a newline.
# - #printf: Prints the string resulting from applying the given format string
#   to any additional arguments.
# - #putc: Equivalent to <tt.$stdout.putc(object)</tt> for the given object.
# - #puts: Equivalent to <tt>$stdout.puts(*objects)</tt> for the given objects.
# - #readline: Similar to #gets, but raises an exception at the end of file.
# - #readlines: Returns an array of the remaining lines from the current input.
# - #select: Same as IO.select.
#
# === Procs
#
# - #lambda: Returns a lambda proc for the given block.
# - #proc: Returns a new Proc; equivalent to Proc.new.
#
# === Tracing
#
# - #set_trace_func: Sets the given proc as the handler for tracing,
#   or disables tracing if given +nil+.
# - #trace_var: Starts tracing assignments to the given global variable.
# - #untrace_var: Disables tracing of assignments to the given global variable.
#
# === Subprocesses
#
# - {\`command`}[rdoc-ref:Kernel#`]: Returns the standard output of running
#   +command+ in a subshell.
# - #exec: Replaces current process with a new process.
# - #fork: Forks the current process into two processes.
# - #spawn: Executes the given command and returns its pid without waiting
#   for completion.
# - #system: Executes the given command in a subshell.
#
# === Loading
#
# - #autoload: Registers the given file to be loaded when the given constant
#   is first referenced.
# - #load: Loads the given Ruby file.
# - #require: Loads the given Ruby file unless it has already been loaded.
# - #require_relative: Loads the Ruby file path relative to the calling file,
#   unless it has already been loaded.
#
# === Yielding
#
# - #tap: Yields +self+ to the given block; returns +self+.
# - #then (aliased as #yield_self): Yields +self+ to the block
#   and returns the result of the block.
#
# === \Random Values
#
# - #rand: Returns a pseudo-random floating point number
#   strictly between 0.0 and 1.0.
# - #srand: Seeds the pseudo-random number generator with the given number.
#
# === Other
#
# - #eval: Evaluates the given string as Ruby code.
# - #loop: Repeatedly executes the given block.
# - #sleep: Suspends the current thread for the given number of seconds.
# - #sprintf (aliased as #format): Returns the string resulting from applying
#   the given format string to any additional arguments.
# - #syscall: Runs an operating system call.
# - #trap: Specifies the handling of system signals.
# - #warn: Issue a warning based on the given messages and options.
module Kernel
  # Returns the called name of the current method as a Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def self.__callee__; end

  # Returns the canonicalized absolute path of the directory of the file from
  # which this method is called. It means symlinks in the path is resolved.
  # If <code>__FILE__</code> is <code>nil</code>, it returns <code>nil</code>.
  # The return value equals to <code>File.dirname(File.realpath(__FILE__))</code>.
  def self.__dir__; end

  # Returns the name at the definition of the current method as a
  # Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def self.__method__; end

  # Returns the <tt>$stdout</tt> output from running +command+ in a subshell;
  # sets global variable <tt>$?</tt> to the process status.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Examples:
  #
  #   $ `date`                 # => "Wed Apr  9 08:56:30 CDT 2003\n"
  #   $ `echo oops && exit 99` # => "oops\n"
  #   $ $?                     # => #<Process::Status: pid 17088 exit 99>
  #   $ $?.status              # => 99>
  #
  # The built-in syntax <tt>%x{...}</tt> uses this method.
  def self.`(command) end

  # Returns an array converted from +object+.
  #
  # Tries to convert +object+ to an array
  # using +to_ary+ first and +to_a+ second:
  #
  #   Array([0, 1, 2])        # => [0, 1, 2]
  #   Array({foo: 0, bar: 1}) # => [[:foo, 0], [:bar, 1]]
  #   Array(0..4)             # => [0, 1, 2, 3, 4]
  #
  # Returns +object+ in an array, <tt>[object]</tt>,
  # if +object+ cannot be converted:
  #
  #   Array(:foo)             # => [:foo]
  def self.Array(object) end

  # Returns the \BigDecimal converted from +value+
  # with a precision of +ndigits+ decimal digits.
  #
  # When +ndigits+ is less than the number of significant digits
  # in the value, the result is rounded to that number of digits,
  # according to the current rounding mode; see BigDecimal.mode.
  #
  # When +ndigits+ is 0, the number of digits to correctly represent a float number
  # is determined automatically.
  #
  # Returns +value+ converted to a \BigDecimal, depending on the type of +value+:
  #
  # - Integer, Float, Rational, Complex, or BigDecimal: converted directly:
  #
  #     # Integer, Complex, or BigDecimal value does not require ndigits; ignored if given.
  #     BigDecimal(2)                     # => 0.2e1
  #     BigDecimal(Complex(2, 0))         # => 0.2e1
  #     BigDecimal(BigDecimal(2))         # => 0.2e1
  #     # Float or Rational value requires ndigits.
  #     BigDecimal(2.0, 0)                # => 0.2e1
  #     BigDecimal(Rational(2, 1), 0)     # => 0.2e1
  #
  # - String: converted by parsing if it contains an integer or floating-point literal;
  #   leading and trailing whitespace is ignored:
  #
  #     # String does not require ndigits; ignored if given.
  #     BigDecimal('2')     # => 0.2e1
  #     BigDecimal('2.0')   # => 0.2e1
  #     BigDecimal('0.2e1') # => 0.2e1
  #     BigDecimal(' 2.0 ') # => 0.2e1
  #
  # - Other type that responds to method <tt>:to_str</tt>:
  #   first converted to a string, then converted to a \BigDecimal, as above.
  #
  # - Other type:
  #
  #   - Raises an exception if keyword argument +exception+ is +true+.
  #   - Returns +nil+ if keyword argument +exception+ is +false+.
  #
  # Raises an exception if +value+ evaluates to a Float
  # and +digits+ is larger than Float::DIG + 1.
  def self.BigDecimal(...) end

  # Returns a new \Complex object if the arguments are valid;
  # otherwise raises an exception if +exception+ is +true+;
  # otherwise returns +nil+.
  #
  # With Numeric arguments +real+ and +imag+,
  # returns <tt>Complex.rect(real, imag)</tt> if the arguments are valid.
  #
  # With string argument +s+, returns a new \Complex object if the argument is valid;
  # the string may have:
  #
  # - One or two numeric substrings,
  #   each of which specifies a Complex, Float, Integer, Numeric, or Rational value,
  #   specifying {rectangular coordinates}[rdoc-ref:Complex@Rectangular+Coordinates]:
  #
  #   - Sign-separated real and imaginary numeric substrings
  #     (with trailing character <tt>'i'</tt>):
  #
  #       Complex('1+2i')  # => (1+2i)
  #       Complex('+1+2i') # => (1+2i)
  #       Complex('+1-2i') # => (1-2i)
  #       Complex('-1+2i') # => (-1+2i)
  #       Complex('-1-2i') # => (-1-2i)
  #
  #   - Real-only numeric string (without trailing character <tt>'i'</tt>):
  #
  #       Complex('1')  # => (1+0i)
  #       Complex('+1') # => (1+0i)
  #       Complex('-1') # => (-1+0i)
  #
  #   - Imaginary-only numeric string (with trailing character <tt>'i'</tt>):
  #
  #       Complex('1i')  # => (0+1i)
  #       Complex('+1i') # => (0+1i)
  #       Complex('-1i') # => (0-1i)
  #
  # - At-sign separated real and imaginary rational substrings,
  #   each of which specifies a Rational value,
  #   specifying {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates]:
  #
  #     Complex('1/2@3/4')   # => (0.36584443443691045+0.34081938001166706i)
  #     Complex('+1/2@+3/4') # => (0.36584443443691045+0.34081938001166706i)
  #     Complex('+1/2@-3/4') # => (0.36584443443691045-0.34081938001166706i)
  #     Complex('-1/2@+3/4') # => (-0.36584443443691045-0.34081938001166706i)
  #     Complex('-1/2@-3/4') # => (-0.36584443443691045+0.34081938001166706i)
  def self.Complex(...) end

  # Returns a hash converted from +object+.
  #
  # - If +object+ is:
  #
  #   - A hash, returns +object+.
  #   - An empty array or +nil+, returns an empty hash.
  #
  # - Otherwise, if <tt>object.to_hash</tt> returns a hash, returns that hash.
  # - Otherwise, returns TypeError.
  #
  # Examples:
  #
  #   Hash({foo: 0, bar: 1}) # => {:foo=>0, :bar=>1}
  #   Hash(nil)              # => {}
  #   Hash([])               # => {}
  def self.Hash(object) end

  # Creates a new Pathname object from the given string, +path+, and returns
  # pathname object.
  #
  # In order to use this constructor, you must first require the Pathname
  # standard library extension.
  #
  #      require 'pathname'
  #      Pathname("/home/zzak")
  #      #=> #<Pathname:/home/zzak>
  #
  # See also Pathname::new for more information.
  def self.Pathname(path) end

  # Returns +x/y+ or +arg+ as a Rational.
  #
  #    Rational(2, 3)   #=> (2/3)
  #    Rational(5)      #=> (5/1)
  #    Rational(0.5)    #=> (1/2)
  #    Rational(0.3)    #=> (5404319552844595/18014398509481984)
  #
  #    Rational("2/3")  #=> (2/3)
  #    Rational("0.3")  #=> (3/10)
  #
  #    Rational("10 cents")  #=> ArgumentError
  #    Rational(nil)         #=> TypeError
  #    Rational(1, nil)      #=> TypeError
  #
  #    Rational("10 cents", exception: false)  #=> nil
  #
  # Syntax of the string form:
  #
  #   string form = extra spaces , rational , extra spaces ;
  #   rational = [ sign ] , unsigned rational ;
  #   unsigned rational = numerator | numerator , "/" , denominator ;
  #   numerator = integer part | fractional part | integer part , fractional part ;
  #   denominator = digits ;
  #   integer part = digits ;
  #   fractional part = "." , digits , [ ( "e" | "E" ) , [ sign ] , digits ] ;
  #   sign = "-" | "+" ;
  #   digits = digit , { digit | "_" , digit } ;
  #   digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
  #   extra spaces = ? \s* ? ;
  #
  # See also String#to_r.
  def self.Rational(...) end

  # Returns a string converted from +object+.
  #
  # Tries to convert +object+ to a string
  # using +to_str+ first and +to_s+ second:
  #
  #   String([0, 1, 2])        # => "[0, 1, 2]"
  #   String(0..5)             # => "0..5"
  #   String({foo: 0, bar: 1}) # => "{:foo=>0, :bar=>1}"
  #
  # Raises +TypeError+ if +object+ cannot be converted to a string.
  def self.String(object) end

  # Terminates execution immediately, effectively by calling
  # <tt>Kernel.exit(false)</tt>.
  #
  # If string argument +msg+ is given,
  # it is written to STDERR prior to termination;
  # otherwise, if an exception was raised,
  # prints its message and backtrace.
  def self.abort(message = _) end

  # Converts _block_ to a +Proc+ object (and therefore
  # binds it at the point of call) and registers it for execution when
  # the program exits. If multiple handlers are registered, they are
  # executed in reverse order of registration.
  #
  #    def do_at_exit(str1)
  #      at_exit { print str1 }
  #    end
  #    at_exit { puts "cruel world" }
  #    do_at_exit("goodbye ")
  #    exit
  #
  # <em>produces:</em>
  #
  #    goodbye cruel world
  def self.at_exit; end

  #  Registers _filename_ to be loaded (using Kernel::require)
  #  the first time that _const_ (which may be a String or
  #  a symbol) is accessed.
  #
  #     autoload(:MyModule, "/usr/local/lib/modules/my_module.rb")
  #
  # If _const_ is defined as autoload, the file name to be loaded is
  # replaced with _filename_.  If _const_ is defined but not as
  # autoload, does nothing.
  def self.autoload(module_, filename) end

  # Returns _filename_ to be loaded if _name_ is registered as
  # +autoload+.
  #
  #    autoload(:B, "b")
  #    autoload?(:B)            #=> "b"
  def self.autoload?(name, inherit = true) end

  # Returns a Binding object, describing the variable and
  # method bindings at the point of call. This object can be used when
  # calling Binding#eval to execute the evaluated command in this
  # environment, or extracting its local variables.
  #
  #    class User
  #      def initialize(name, position)
  #        @name = name
  #        @position = position
  #      end
  #
  #      def get_binding
  #        binding
  #      end
  #    end
  #
  #    user = User.new('Joan', 'manager')
  #    template = '{name: @name, position: @position}'
  #
  #    # evaluate template in context of the object
  #    eval(template, user.get_binding)
  #    #=> {:name=>"Joan", :position=>"manager"}
  #
  # Binding#local_variable_get can be used to access the variables
  # whose names are reserved Ruby keywords:
  #
  #    # This is valid parameter declaration, but `if` parameter can't
  #    # be accessed by name, because it is a reserved word.
  #    def validate(field, validation, if: nil)
  #      condition = binding.local_variable_get('if')
  #      return unless condition
  #
  #      # ...Some implementation ...
  #    end
  #
  #    validate(:name, :empty?, if: false) # skips validation
  #    validate(:name, :empty?, if: true) # performs validation
  def self.binding; end

  # Returns <code>true</code> if <code>yield</code> would execute a
  # block in the current context. The <code>iterator?</code> form
  # is mildly deprecated.
  #
  #    def try
  #      if block_given?
  #        yield
  #      else
  #        "no block"
  #      end
  #    end
  #    try                  #=> "no block"
  #    try { "hello" }      #=> "hello"
  #    try do "hello" end   #=> "hello"
  def self.block_given?; end

  # Generates a Continuation object, which it passes to
  # the associated block. You need to <code>require
  # 'continuation'</code> before using this method. Performing a
  # <em>cont</em><code>.call</code> will cause the #callcc
  # to return (as will falling through the end of the block). The
  # value returned by the #callcc is the value of the
  # block, or the value passed to <em>cont</em><code>.call</code>. See
  # class Continuation for more details. Also see
  # Kernel#throw for an alternative mechanism for
  # unwinding a call stack.
  def self.callcc; end

  # Returns the current execution stack---an array containing strings in
  # the form <code>file:line</code> or <code>file:line: in
  # `method'</code>.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  #
  #    def a(skip)
  #      caller(skip)
  #    end
  #    def b(skip)
  #      a(skip)
  #    end
  #    def c(skip)
  #      b(skip)
  #    end
  #    c(0)   #=> ["prog:2:in `a'", "prog:5:in `b'", "prog:8:in `c'", "prog:10:in `<main>'"]
  #    c(1)   #=> ["prog:5:in `b'", "prog:8:in `c'", "prog:11:in `<main>'"]
  #    c(2)   #=> ["prog:8:in `c'", "prog:12:in `<main>'"]
  #    c(3)   #=> ["prog:13:in `<main>'"]
  #    c(4)   #=> []
  #    c(5)   #=> nil
  def self.caller(...) end

  # Returns the current execution stack---an array containing
  # backtrace location objects.
  #
  # See Thread::Backtrace::Location for more information.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  def self.caller_locations(...) end

  # +catch+ executes its block. If +throw+ is not called, the block executes
  # normally, and +catch+ returns the value of the last expression evaluated.
  #
  #    catch(1) { 123 }            # => 123
  #
  # If <code>throw(tag2, val)</code> is called, Ruby searches up its stack for
  # a +catch+ block whose +tag+ has the same +object_id+ as _tag2_. When found,
  # the block stops executing and returns _val_ (or +nil+ if no second argument
  # was given to +throw+).
  #
  #    catch(1) { throw(1, 456) }  # => 456
  #    catch(1) { throw(1) }       # => nil
  #
  # When +tag+ is passed as the first argument, +catch+ yields it as the
  # parameter of the block.
  #
  #    catch(1) {|x| x + 2 }       # => 3
  #
  # When no +tag+ is given, +catch+ yields a new unique object (as from
  # +Object.new+) as the block parameter. This object can then be used as the
  # argument to +throw+, and will match the correct +catch+ block.
  #
  #    catch do |obj_A|
  #      catch do |obj_B|
  #        throw(obj_B, 123)
  #        puts "This puts is not reached"
  #      end
  #
  #      puts "This puts is displayed"
  #      456
  #    end
  #
  #    # => 456
  #
  #    catch do |obj_A|
  #      catch do |obj_B|
  #        throw(obj_A, 123)
  #        puts "This puts is still not reached"
  #      end
  #
  #      puts "Now this puts is also not reached"
  #      456
  #    end
  #
  #    # => 123
  def self.catch(*tag) end

  # Equivalent to <code>$_ = $_.chomp(<em>string</em>)</code>. See
  # String#chomp.
  # Available only when -p/-n command line option specified.
  def self.chomp(...) end

  # Equivalent to <code>($_.dup).chop!</code>, except <code>nil</code>
  # is never returned. See String#chop!.
  # Available only when -p/-n command line option specified.
  def self.chop; end

  # Evaluates the Ruby expression(s) in <em>string</em>. If
  # <em>binding</em> is given, which must be a Binding object, the
  # evaluation is performed in its context. If the optional
  # <em>filename</em> and <em>lineno</em> parameters are present, they
  # will be used when reporting syntax errors.
  #
  #    def get_binding(str)
  #      return binding
  #    end
  #    str = "hello"
  #    eval "str + ' Fred'"                      #=> "hello Fred"
  #    eval "str + ' Fred'", get_binding("bye")  #=> "bye Fred"
  def self.eval(string, binding = _, filename = _, lineno = _) end

  # Replaces the current process by doing one of the following:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The new process is created using the
  # {exec system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/execve.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   exec('if true; then echo "Foo"; fi') # Shell reserved word.
  #   exec('echo')                         # Built-in.
  #   exec('date > date.tmp')              # Contains meta character.
  #
  # The command line may also contain arguments and options for the command:
  #
  #   exec('echo "Foo"')
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called.
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process.
  #
  # Example:
  #
  #   exec('/usr/bin/date')
  #
  # Output:
  #
  #   Sat Aug 26 09:38:00 AM CDT 2023
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion:
  #
  #   exec('doesnt_exist') # Raises Errno::ENOENT
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   exec('echo', 'C*')
  #   exec('echo', 'hello', 'world')
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def self.exec(...) end

  # Initiates termination of the Ruby script by raising SystemExit;
  # the exception may be caught.
  # Returns exit status +status+ to the underlying operating system.
  #
  # Values +true+ and +false+ for argument +status+
  # indicate, respectively, success and failure;
  # The meanings of integer values are system-dependent.
  #
  # Example:
  #
  #   begin
  #     exit
  #     puts 'Never get here.'
  #   rescue SystemExit
  #     puts 'Rescued a SystemExit exception.'
  #   end
  #   puts 'After begin block.'
  #
  # Output:
  #
  #   Rescued a SystemExit exception.
  #   After begin block.
  #
  # Just prior to final termination,
  # Ruby executes any at-exit procedures (see Kernel::at_exit)
  # and any object finalizers (see ObjectSpace::define_finalizer).
  #
  # Example:
  #
  #   at_exit { puts 'In at_exit function.' }
  #   ObjectSpace.define_finalizer('string', proc { puts 'In finalizer.' })
  #   exit
  #
  # Output:
  #
  #    In at_exit function.
  #    In finalizer.
  def self.exit(status = true) end

  # Exits the process immediately; no exit handlers are called.
  # Returns exit status +status+ to the underlying operating system.
  #
  #    Process.exit!(true)
  #
  # Values +true+ and +false+ for argument +status+
  # indicate, respectively, success and failure;
  # The meanings of integer values are system-dependent.
  def self.exit!(status = false) end

  # With no arguments, raises the exception in <code>$!</code> or raises
  # a RuntimeError if <code>$!</code> is +nil+.  With a single +String+
  # argument, raises a +RuntimeError+ with the string as a message. Otherwise,
  # the first parameter should be an +Exception+ class (or another
  # object that returns an +Exception+ object when sent an +exception+
  # message).  The optional second parameter sets the message associated with
  # the exception (accessible via Exception#message), and the third parameter
  # is an array of callback information (accessible via Exception#backtrace).
  # The +cause+ of the generated exception (accessible via Exception#cause)
  # is automatically set to the "current" exception (<code>$!</code>), if any.
  # An alternative value, either an +Exception+ object or +nil+, can be
  # specified via the +:cause+ argument.
  #
  # Exceptions are caught by the +rescue+ clause of
  # <code>begin...end</code> blocks.
  #
  #    raise "Failed to create socket"
  #    raise ArgumentError, "No parameters", caller
  def self.fail(...) end

  # Creates a child process.
  #
  # With a block given, runs the block in the child process;
  # on block exit, the child terminates with a status of zero:
  #
  #   puts "Before the fork: #{Process.pid}"
  #   fork do
  #     puts "In the child process: #{Process.pid}"
  #   end                   # => 382141
  #   puts "After the fork: #{Process.pid}"
  #
  # Output:
  #
  #   Before the fork: 420496
  #   After the fork: 420496
  #   In the child process: 420520
  #
  # With no block given, the +fork+ call returns twice:
  #
  # - Once in the parent process, returning the pid of the child process.
  # - Once in the child process, returning +nil+.
  #
  # Example:
  #
  #   puts "This is the first line before the fork (pid #{Process.pid})"
  #   puts fork
  #   puts "This is the second line after the fork (pid #{Process.pid})"
  #
  # Output:
  #
  #   This is the first line before the fork (pid 420199)
  #   420223
  #   This is the second line after the fork (pid 420199)
  #
  #   This is the second line after the fork (pid 420223)
  #
  # In either case, the child process may exit using
  # Kernel.exit! to avoid the call to Kernel#at_exit.
  #
  # To avoid zombie processes, the parent process should call either:
  #
  # - Process.wait, to collect the termination statuses of its children.
  # - Process.detach, to register disinterest in their status.
  #
  # The thread calling +fork+ is the only thread in the created child process;
  # +fork+ doesn't copy other threads.
  #
  # Note that method +fork+ is available on some platforms,
  # but not on others:
  #
  #   Process.respond_to?(:fork) # => true # Would be false on some.
  #
  # If not, you may use ::spawn instead of +fork+.
  def self.fork; end

  # Returns the string resulting from formatting +objects+
  # into +format_string+.
  #
  # For details on +format_string+, see
  # {Format Specifications}[rdoc-ref:format_specifications.rdoc].
  def self.format(format_string, *args) end

  # Returns (and assigns to <code>$_</code>) the next line from the list
  # of files in +ARGV+ (or <code>$*</code>), or from standard input if
  # no files are present on the command line. Returns +nil+ at end of
  # file. The optional argument specifies the record separator. The
  # separator is included with the contents of each record. A separator
  # of +nil+ reads the entire contents, and a zero-length separator
  # reads the input one paragraph at a time, where paragraphs are
  # divided by two consecutive newlines.  If the first argument is an
  # integer, or optional second argument is given, the returning string
  # would not be longer than the given value in bytes.  If multiple
  # filenames are present in +ARGV+, <code>gets(nil)</code> will read
  # the contents one file at a time.
  #
  #    ARGV << "testfile"
  #    print while gets
  #
  # <em>produces:</em>
  #
  #    This is line one
  #    This is line two
  #    This is line three
  #    And so on...
  #
  # The style of programming using <code>$_</code> as an implicit
  # parameter is gradually losing favor in the Ruby community.
  def self.gets(...) end

  # Returns an array of the names of global variables. This includes
  # special regexp global variables such as <tt>$~</tt> and <tt>$+</tt>,
  # but does not include the numbered regexp global variables (<tt>$1</tt>,
  # <tt>$2</tt>, etc.).
  #
  #    global_variables.grep /std/   #=> [:$stdin, :$stdout, :$stderr]
  def self.global_variables; end

  # Equivalent to <code>$_.gsub...</code>, except that <code>$_</code>
  # will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def self.gsub(...) end

  # Deprecated.  Use block_given? instead.
  def self.iterator?; end

  # Equivalent to Proc.new, except the resulting Proc objects check the
  # number of parameters passed when called.
  def self.lambda; end

  # Loads and executes the Ruby program in the file _filename_.
  #
  # If the filename is an absolute path (e.g. starts with '/'), the file
  # will be loaded directly using the absolute path.
  #
  # If the filename is an explicit relative path (e.g. starts with './' or
  # '../'), the file will be loaded using the relative path from the current
  # directory.
  #
  # Otherwise, the file will be searched for in the library
  # directories listed in <code>$LOAD_PATH</code> (<code>$:</code>).
  # If the file is found in a directory, it will attempt to load the file
  # relative to that directory.  If the file is not found in any of the
  # directories in <code>$LOAD_PATH</code>, the file will be loaded using
  # the relative path from the current directory.
  #
  # If the file doesn't exist when there is an attempt to load it, a
  # LoadError will be raised.
  #
  # If the optional _wrap_ parameter is +true+, the loaded script will
  # be executed under an anonymous module, protecting the calling
  # program's global namespace.  If the optional _wrap_ parameter is a
  # module, the loaded script will be executed under the given module.
  # In no circumstance will any local variables in the loaded file be
  # propagated to the loading environment.
  def self.load(filename, wrap = false) end

  # Returns the names of the current local variables.
  #
  #    fred = 1
  #    for i in 1..10
  #       # ...
  #    end
  #    local_variables   #=> [:fred, :i]
  def self.local_variables; end

  # Creates an IO object connected to the given file.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # With no block given, file stream is returned:
  #
  #   open('t.txt') # => #<File:t.txt>
  #
  # With a block given, calls the block with the open file stream,
  # then closes the stream:
  #
  #   open('t.txt') {|f| p f } # => #<File:t.txt (closed)>
  #
  # Output:
  #
  #   #<File:t.txt>
  #
  # See File.open for details.
  def self.open(path, mode = 'r', perm = 0o666, **opts) end

  # For each object +obj+, executes:
  #
  #   $stdout.write(obj.inspect, "\n")
  #
  # With one object given, returns the object;
  # with multiple objects given, returns an array containing the objects;
  # with no object given, returns +nil+.
  #
  # Examples:
  #
  #   r = Range.new(0, 4)
  #   p r                 # => 0..4
  #   p [r, r, r]         # => [0..4, 0..4, 0..4]
  #   p                   # => nil
  #
  # Output:
  #
  #    0..4
  #    [0..4, 0..4, 0..4]
  #
  # Kernel#p is designed for debugging purposes.
  # Ruby implementations may define Kernel#p to be uninterruptible
  # in whole or in part.
  # On CRuby, Kernel#p's writing of data is uninterruptible.
  def self.p(...) end

  # Equivalent to <tt>$stdout.print(*objects)</tt>,
  # this method is the straightforward way to write to <tt>$stdout</tt>.
  #
  # Writes the given objects to <tt>$stdout</tt>; returns +nil+.
  # Appends the output record separator <tt>$OUTPUT_RECORD_SEPARATOR</tt>
  # <tt>$\\</tt>), if it is not +nil+.
  #
  # With argument +objects+ given, for each object:
  #
  # - Converts via its method +to_s+ if not a string.
  # - Writes to <tt>stdout</tt>.
  # - If not the last object, writes the output field separator
  #   <tt>$OUTPUT_FIELD_SEPARATOR</tt> (<tt>$,</tt> if it is not +nil+.
  #
  # With default separators:
  #
  #   objects = [0, 0.0, Rational(0, 1), Complex(0, 0), :zero, 'zero']
  #   $OUTPUT_RECORD_SEPARATOR
  #   $OUTPUT_FIELD_SEPARATOR
  #   print(*objects)
  #
  # Output:
  #
  #   nil
  #   nil
  #   00.00/10+0izerozero
  #
  # With specified separators:
  #
  #   $OUTPUT_RECORD_SEPARATOR = "\n"
  #   $OUTPUT_FIELD_SEPARATOR = ','
  #   print(*objects)
  #
  # Output:
  #
  #   0,0.0,0/1,0+0i,zero,zero
  #
  # With no argument given, writes the content of <tt>$_</tt>
  # (which is usually the most recent user input):
  #
  #   gets  # Sets $_ to the most recent user input.
  #   print # Prints $_.
  def self.print(*objects) end

  # Equivalent to:
  #
  #   io.write(sprintf(format_string, *objects))
  #
  # For details on +format_string+, see
  # {Format Specifications}[rdoc-ref:format_specifications.rdoc].
  #
  # With the single argument +format_string+, formats +objects+ into the string,
  # then writes the formatted string to $stdout:
  #
  #   printf('%4.4d %10s %2.2f', 24, 24, 24.0)
  #
  # Output (on $stdout):
  #
  #   0024         24 24.00#
  #
  # With arguments +io+ and +format_string+, formats +objects+ into the string,
  # then writes the formatted string to +io+:
  #
  #   printf($stderr, '%4.4d %10s %2.2f', 24, 24, 24.0)
  #
  # Output (on $stderr):
  #
  #   0024         24 24.00# => nil
  #
  # With no arguments, does nothing.
  def self.printf(...) end

  # Equivalent to Proc.new.
  def self.proc; end

  # Equivalent to:
  #
  #   $stdout.putc(int)
  #
  # See IO#putc for important information regarding multi-byte characters.
  def self.putc(int) end

  # Equivalent to
  #
  #    $stdout.puts(objects)
  def self.puts(*objects) end

  # With no arguments, raises the exception in <code>$!</code> or raises
  # a RuntimeError if <code>$!</code> is +nil+.  With a single +String+
  # argument, raises a +RuntimeError+ with the string as a message. Otherwise,
  # the first parameter should be an +Exception+ class (or another
  # object that returns an +Exception+ object when sent an +exception+
  # message).  The optional second parameter sets the message associated with
  # the exception (accessible via Exception#message), and the third parameter
  # is an array of callback information (accessible via Exception#backtrace).
  # The +cause+ of the generated exception (accessible via Exception#cause)
  # is automatically set to the "current" exception (<code>$!</code>), if any.
  # An alternative value, either an +Exception+ object or +nil+, can be
  # specified via the +:cause+ argument.
  #
  # Exceptions are caught by the +rescue+ clause of
  # <code>begin...end</code> blocks.
  #
  #    raise "Failed to create socket"
  #    raise ArgumentError, "No parameters", caller
  def self.raise(...) end

  # If called without an argument, or if <tt>max.to_i.abs == 0</tt>, rand
  # returns a pseudo-random floating point number between 0.0 and 1.0,
  # including 0.0 and excluding 1.0.
  #
  #   rand        #=> 0.2725926052826416
  #
  # When +max.abs+ is greater than or equal to 1, +rand+ returns a pseudo-random
  # integer greater than or equal to 0 and less than +max.to_i.abs+.
  #
  #   rand(100)   #=> 12
  #
  # When +max+ is a Range, +rand+ returns a random number where
  # <code>range.member?(number) == true</code>.
  #
  # Negative or floating point values for +max+ are allowed, but may give
  # surprising results.
  #
  #   rand(-100) # => 87
  #   rand(-0.5) # => 0.8130921818028143
  #   rand(1.9)  # equivalent to rand(1), which is always 0
  #
  # Kernel.srand may be used to ensure that sequences of random numbers are
  # reproducible between different runs of a program.
  #
  # See also Random.rand.
  def self.rand(max = 0) end

  # Equivalent to method Kernel#gets, except that it raises an exception
  # if called at end-of-stream:
  #
  #   $ cat t.txt | ruby -e "p readlines; readline"
  #   ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #   in `readline': end of file reached (EOFError)
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted.
  def self.readline(...) end

  # Returns an array containing the lines returned by calling
  # Kernel#gets until the end-of-stream is reached;
  # (see {Line IO}[rdoc-ref:IO@Line+IO]).
  #
  # With only string argument +sep+ given,
  # returns the remaining lines as determined by line separator +sep+,
  # or +nil+ if none;
  # see {Line Separator}[rdoc-ref:IO@Line+Separator]:
  #
  #   # Default separator.
  #   $ cat t.txt | ruby -e "p readlines"
  #   ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #
  #   # Specified separator.
  #   $ cat t.txt | ruby -e "p readlines 'li'"
  #   ["First li", "ne\nSecond li", "ne\n\nFourth li", "ne\nFifth li", "ne\n"]
  #
  #   # Get-all separator.
  #   $ cat t.txt | ruby -e "p readlines nil"
  #   ["First line\nSecond line\n\nFourth line\nFifth line\n"]
  #
  #   # Get-paragraph separator.
  #   $ cat t.txt | ruby -e "p readlines ''"
  #   ["First line\nSecond line\n\n", "Fourth line\nFifth line\n"]
  #
  # With only integer argument +limit+ given,
  # limits the number of bytes in the line;
  # see {Line Limit}[rdoc-ref:IO@Line+Limit]:
  #
  #   $cat t.txt | ruby -e "p readlines 10"
  #   ["First line", "\n", "Second lin", "e\n", "\n", "Fourth lin", "e\n", "Fifth line", "\n"]
  #
  #   $cat t.txt | ruby -e "p readlines 11"
  #   ["First line\n", "Second line", "\n", "\n", "Fourth line", "\n", "Fifth line\n"]
  #
  #   $cat t.txt | ruby -e "p readlines 12"
  #   ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #
  # With arguments +sep+ and +limit+ given, combines the two behaviors;
  # see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit].
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted:
  #
  #   $ cat t.txt | ruby -e "p readlines(chomp: true)"
  #   ["First line", "Second line", "", "Fourth line", "Fifth line"]
  #
  # Optional keyword arguments +enc_opts+ specify encoding options;
  # see {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def self.readlines(...) end

  # Loads the given +name+, returning +true+ if successful and +false+ if the
  # feature is already loaded.
  #
  # If the filename neither resolves to an absolute path nor starts with
  # './' or '../', the file will be searched for in the library
  # directories listed in <code>$LOAD_PATH</code> (<code>$:</code>).
  # If the filename starts with './' or '../', resolution is based on Dir.pwd.
  #
  # If the filename has the extension ".rb", it is loaded as a source file; if
  # the extension is ".so", ".o", or the default shared library extension on
  # the current platform, Ruby loads the shared library as a Ruby extension.
  # Otherwise, Ruby tries adding ".rb", ".so", and so on to the name until
  # found.  If the file named cannot be found, a LoadError will be raised.
  #
  # For Ruby extensions the filename given may use ".so" or ".o".  For example,
  # on macOS the socket extension is "socket.bundle" and
  # <code>require 'socket.so'</code> will load the socket extension.
  #
  # The absolute path of the loaded file is added to
  # <code>$LOADED_FEATURES</code> (<code>$"</code>).  A file will not be
  # loaded again if its path already appears in <code>$"</code>.  For example,
  # <code>require 'a'; require './a'</code> will not load <code>a.rb</code>
  # again.
  #
  #   require "my-library.rb"
  #   require "db-driver"
  #
  # Any constants or globals within the loaded source file will be available
  # in the calling program's global namespace. However, local variables will
  # not be propagated to the loading environment.
  def self.require(name) end

  # Ruby tries to load the library named _string_ relative to the directory
  # containing the requiring file.  If the file does not exist a LoadError is
  # raised. Returns +true+ if the file was loaded and +false+ if the file was
  # already loaded before.
  def self.require_relative(string) end

  # Invokes system call {select(2)}[https://linux.die.net/man/2/select],
  # which monitors multiple file descriptors,
  # waiting until one or more of the file descriptors
  # becomes ready for some class of I/O operation.
  #
  # Not implemented on all platforms.
  #
  # Each of the arguments +read_ios+, +write_ios+, and +error_ios+
  # is an array of IO objects.
  #
  # Argument +timeout+ is an integer timeout interval in seconds.
  #
  # The method monitors the \IO objects given in all three arrays,
  # waiting for some to be ready;
  # returns a 3-element array whose elements are:
  #
  # - An array of the objects in +read_ios+ that are ready for reading.
  # - An array of the objects in +write_ios+ that are ready for writing.
  # - An array of the objects in +error_ios+ have pending exceptions.
  #
  # If no object becomes ready within the given +timeout+, +nil+ is returned.
  #
  # \IO.select peeks the buffer of \IO objects for testing readability.
  # If the \IO buffer is not empty, \IO.select immediately notifies
  # readability.  This "peek" only happens for \IO objects.  It does not
  # happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use \IO.select is invoking it after non-blocking
  # methods such as #read_nonblock, #write_nonblock, etc.  The methods
  # raise an exception which is extended by IO::WaitReadable or
  # IO::WaitWritable.  The modules notify how the caller should wait
  # with \IO.select.  If IO::WaitReadable is raised, the caller should
  # wait for reading.  If IO::WaitWritable is raised, the caller should
  # wait for writing.
  #
  # So, blocking read (#readpartial) can be emulated using
  # #read_nonblock and \IO.select as follows:
  #
  #   begin
  #     result = io_like.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io_like])
  #     retry
  #   rescue IO::WaitWritable
  #     IO.select(nil, [io_like])
  #     retry
  #   end
  #
  # Especially, the combination of non-blocking methods and \IO.select is
  # preferred for IO like objects such as OpenSSL::SSL::SSLSocket.  It
  # has #to_io method to return underlying IO object.  IO.select calls
  # #to_io to obtain the file descriptor to wait.
  #
  # This means that readability notified by \IO.select doesn't mean
  # readability from OpenSSL::SSL::SSLSocket object.
  #
  # The most likely situation is that OpenSSL::SSL::SSLSocket buffers
  # some data.  \IO.select doesn't see the buffer.  So \IO.select can
  # block when OpenSSL::SSL::SSLSocket#readpartial doesn't block.
  #
  # However, several more complicated situations exist.
  #
  # SSL is a protocol which is sequence of records.
  # The record consists of multiple bytes.
  # So, the remote side of SSL sends a partial record, IO.select
  # notifies readability but OpenSSL::SSL::SSLSocket cannot decrypt a
  # byte and OpenSSL::SSL::SSLSocket#readpartial will block.
  #
  # Also, the remote side can request SSL renegotiation which forces
  # the local SSL engine to write some data.
  # This means OpenSSL::SSL::SSLSocket#readpartial may invoke #write
  # system call and it can block.
  # In such a situation, OpenSSL::SSL::SSLSocket#read_nonblock raises
  # IO::WaitWritable instead of blocking.
  # So, the caller should wait for ready for writability as above
  # example.
  #
  # The combination of non-blocking methods and \IO.select is also useful
  # for streams such as tty, pipe socket socket when multiple processes
  # read from a stream.
  #
  # Finally, Linux kernel developers don't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for a single process;
  # see {select(2)}[https://linux.die.net/man/2/select]
  #
  # Invoking \IO.select before IO#readpartial works well as usual.
  # However it is not the best way to use \IO.select.
  #
  # The writability notified by select(2) doesn't show
  # how many bytes are writable.
  # IO#write method blocks until given whole string is written.
  # So, <tt>IO#write(two or more bytes)</tt> can block after
  # writability is notified by \IO.select.  IO#write_nonblock is required
  # to avoid the blocking.
  #
  # Blocking write (#write) can be emulated using #write_nonblock and
  # IO.select as follows: IO::WaitReadable should also be rescued for
  # SSL renegotiation in OpenSSL::SSL::SSLSocket.
  #
  #   while 0 < string.bytesize
  #     begin
  #       written = io_like.write_nonblock(string)
  #     rescue IO::WaitReadable
  #       IO.select([io_like])
  #       retry
  #     rescue IO::WaitWritable
  #       IO.select(nil, [io_like])
  #       retry
  #     end
  #     string = string.byteslice(written..-1)
  #   end
  #
  # Example:
  #
  #     rp, wp = IO.pipe
  #     mesg = "ping "
  #     100.times {
  #       # IO.select follows IO#read.  Not the best way to use IO.select.
  #       rs, ws, = IO.select([rp], [wp])
  #       if r = rs[0]
  #         ret = r.read(5)
  #         print ret
  #         case ret
  #         when /ping/
  #           mesg = "pong\n"
  #         when /pong/
  #           mesg = "ping "
  #         end
  #       end
  #       if w = ws[0]
  #         w.write(mesg)
  #       end
  #     }
  #
  # Output:
  #
  #     ping pong
  #     ping pong
  #     ping pong
  #     (snipped)
  #     ping
  def self.select(p1, p2 = v2, p3 = v3, p4 = v4) end

  # Establishes _proc_ as the handler for tracing, or disables
  # tracing if the parameter is +nil+.
  #
  # *Note:* this method is obsolete, please use TracePoint instead.
  #
  # _proc_ takes up to six parameters:
  #
  # * an event name string
  # * a filename string
  # * a line number
  # * a method name symbol, or nil
  # * a binding, or nil
  # * the class, module, or nil
  #
  # _proc_ is invoked whenever an event occurs.
  #
  # Events are:
  #
  # <code>"c-call"</code>:: call a C-language routine
  # <code>"c-return"</code>:: return from a C-language routine
  # <code>"call"</code>:: call a Ruby method
  # <code>"class"</code>:: start a class or module definition
  # <code>"end"</code>:: finish a class or module definition
  # <code>"line"</code>:: execute code on a new line
  # <code>"raise"</code>:: raise an exception
  # <code>"return"</code>:: return from a Ruby method
  #
  # Tracing is disabled within the context of _proc_.
  #
  #   class Test
  #     def test
  #       a = 1
  #       b = 2
  #     end
  #   end
  #
  #   set_trace_func proc { |event, file, line, id, binding, class_or_module|
  #     printf "%8s %s:%-2d %16p %14p\n", event, file, line, id, class_or_module
  #   }
  #   t = Test.new
  #   t.test
  #
  # Produces:
  #
  #   c-return prog.rb:8   :set_trace_func         Kernel
  #       line prog.rb:11              nil            nil
  #     c-call prog.rb:11             :new          Class
  #     c-call prog.rb:11      :initialize    BasicObject
  #   c-return prog.rb:11      :initialize    BasicObject
  #   c-return prog.rb:11             :new          Class
  #       line prog.rb:12              nil            nil
  #       call prog.rb:2             :test           Test
  #       line prog.rb:3             :test           Test
  #       line prog.rb:4             :test           Test
  #     return prog.rb:5             :test           Test
  def self.set_trace_func(...) end

  # Suspends execution of the current thread for the number of seconds
  # specified by numeric argument +secs+, or forever if +secs+ is +nil+;
  # returns the integer number of seconds suspended (rounded).
  #
  #   Time.new  # => 2008-03-08 19:56:19 +0900
  #   sleep 1.2 # => 1
  #   Time.new  # => 2008-03-08 19:56:20 +0900
  #   sleep 1.9 # => 2
  #   Time.new  # => 2008-03-08 19:56:22 +0900
  def self.sleep(secs = nil) end

  # Creates a new child process by doing one of the following
  # in that process:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Returns the process ID (pid) of the new process,
  # without waiting for it to complete.
  #
  # To avoid zombie processes, the parent process should call either:
  #
  # - Process.wait, to collect the termination statuses of its children.
  # - Process.detach, to register disinterest in their status.
  #
  # The new process is created using the
  # {exec system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/execve.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   spawn('if true; then echo "Foo"; fi') # => 798847 # Shell reserved word.
  #   Process.wait                          # => 798847
  #   spawn('echo')                         # => 798848 # Built-in.
  #   Process.wait                          # => 798848
  #   spawn('date > /tmp/date.tmp')         # => 798879 # Contains meta character.
  #   Process.wait                          # => 798849
  #   spawn('date > /nop/date.tmp')         # => 798882 # Issues error message.
  #   Process.wait                          # => 798882
  #
  # The command line may also contain arguments and options for the command:
  #
  #   spawn('echo "Foo"') # => 799031
  #   Process.wait        # => 799031
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called:
  #
  #     spawn('/usr/bin/date') # Path to date on Unix-style system.
  #     Process.wait
  #
  #   Output:
  #
  #     Thu Aug 31 10:06:48 AM CDT 2023
  #
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process:
  #
  #     pid = spawn(['sleep', 'Hello!'], '1') # 2-element array.
  #     p `ps -p #{pid} -o command=`
  #
  #   Output:
  #
  #     "Hello! 1\n"
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion.
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   spawn('echo', 'C*')             # => 799392
  #   Process.wait                    # => 799392
  #   spawn('echo', 'hello', 'world') # => 799393
  #   Process.wait                    # => 799393
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def self.spawn(...) end

  # Returns the string resulting from formatting +objects+
  # into +format_string+.
  #
  # For details on +format_string+, see
  # {Format Specifications}[rdoc-ref:format_specifications.rdoc].
  def self.sprintf(format_string, *args) end

  # Seeds the system pseudo-random number generator, with +number+.
  # The previous seed value is returned.
  #
  # If +number+ is omitted, seeds the generator using a source of entropy
  # provided by the operating system, if available (/dev/urandom on Unix systems
  # or the RSA cryptographic provider on Windows), which is then combined with
  # the time, the process id, and a sequence number.
  #
  # srand may be used to ensure repeatable sequences of pseudo-random numbers
  # between different runs of the program. By setting the seed to a known value,
  # programs can be made deterministic during testing.
  #
  #   srand 1234               # => 268519324636777531569100071560086917274
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  #   [ rand(10), rand(1000) ] # => [4, 664]
  #   srand 1234               # => 1234
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  def self.srand(number = Random.new_seed) end

  # Equivalent to <code>$_.sub(<i>args</i>)</code>, except that
  # <code>$_</code> will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def self.sub(...) end

  # Invokes Posix system call {syscall(2)}[https://linux.die.net/man/2/syscall],
  # which calls a specified function.
  #
  # Calls the operating system function identified by +integer_callno+;
  # returns the result of the function or raises SystemCallError if it failed.
  # The effect of the call is platform-dependent.
  # The arguments and returned value are platform-dependent.
  #
  # For each of +arguments+: if it is an integer, it is passed directly;
  # if it is a string, it is interpreted as a binary sequence of bytes.
  # There may be as many as nine such arguments.
  #
  # Arguments +integer_callno+ and +argument+, as well as the returned value,
  # are platform-dependent.
  #
  # Note: Method +syscall+ is essentially unsafe and unportable.
  # The DL (Fiddle) library is preferred for safer and a bit
  # more portable programming.
  #
  # Not implemented on all platforms.
  def self.syscall(integer_callno, *arguments) end

  # Creates a new child process by doing one of the following
  # in that process:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Returns:
  #
  # - +true+ if the command exits with status zero.
  # - +false+ if the exit status is a non-zero integer.
  # - +nil+ if the command could not execute.
  #
  # Raises an exception (instead of returning +false+ or +nil+)
  # if keyword argument +exception+ is set to +true+.
  #
  # Assigns the command's error status to <tt>$?</tt>.
  #
  # The new process is created using the
  # {system system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/system.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   system('if true; then echo "Foo"; fi')          # => true  # Shell reserved word.
  #   system('echo')                                  # => true  # Built-in.
  #   system('date > /tmp/date.tmp')                  # => true  # Contains meta character.
  #   system('date > /nop/date.tmp')                  # => false
  #   system('date > /nop/date.tmp', exception: true) # Raises RuntimeError.
  #
  # Assigns the command's error status to <tt>$?</tt>:
  #
  #   system('echo')                             # => true  # Built-in.
  #   $?                                         # => #<Process::Status: pid 640610 exit 0>
  #   system('date > /nop/date.tmp')             # => false
  #   $?                                         # => #<Process::Status: pid 640742 exit 2>
  #
  # The command line may also contain arguments and options for the command:
  #
  #   system('echo "Foo"') # => true
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called.
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process.
  #
  # Example:
  #
  #   system('/usr/bin/date') # => true # Path to date on Unix-style system.
  #   system('foo')           # => nil  # Command failed.
  #
  # Output:
  #
  #   Mon Aug 28 11:43:10 AM CDT 2023
  #
  # Assigns the command's error status to <tt>$?</tt>:
  #
  #   system('/usr/bin/date') # => true
  #   $?                      # => #<Process::Status: pid 645605 exit 0>
  #   system('foo')           # => nil
  #   $?                      # => #<Process::Status: pid 645608 exit 127>
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion:
  #
  #   system('doesnt_exist') # => nil
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   system('echo', 'C*')             # => true
  #   system('echo', 'hello', 'world') # => true
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def self.system(...) end

  # Uses the character +cmd+ to perform various tests on +file1+ (first
  # table below) or on +file1+ and +file2+ (second table).
  #
  # File tests on a single file:
  #
  #   Cmd    Returns   Meaning
  #   "A"  | Time    | Last access time for file1
  #   "b"  | boolean | True if file1 is a block device
  #   "c"  | boolean | True if file1 is a character device
  #   "C"  | Time    | Last change time for file1
  #   "d"  | boolean | True if file1 exists and is a directory
  #   "e"  | boolean | True if file1 exists
  #   "f"  | boolean | True if file1 exists and is a regular file
  #   "g"  | boolean | True if file1 has the setgid bit set
  #   "G"  | boolean | True if file1 exists and has a group
  #        |         | ownership equal to the caller's group
  #   "k"  | boolean | True if file1 exists and has the sticky bit set
  #   "l"  | boolean | True if file1 exists and is a symbolic link
  #   "M"  | Time    | Last modification time for file1
  #   "o"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's effective uid
  #   "O"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's real uid
  #   "p"  | boolean | True if file1 exists and is a fifo
  #   "r"  | boolean | True if file1 is readable by the effective
  #        |         | uid/gid of the caller
  #   "R"  | boolean | True if file is readable by the real
  #        |         | uid/gid of the caller
  #   "s"  | int/nil | If file1 has nonzero size, return the size,
  #        |         | otherwise return nil
  #   "S"  | boolean | True if file1 exists and is a socket
  #   "u"  | boolean | True if file1 has the setuid bit set
  #   "w"  | boolean | True if file1 exists and is writable by
  #        |         | the effective uid/gid
  #   "W"  | boolean | True if file1 exists and is writable by
  #        |         | the real uid/gid
  #   "x"  | boolean | True if file1 exists and is executable by
  #        |         | the effective uid/gid
  #   "X"  | boolean | True if file1 exists and is executable by
  #        |         | the real uid/gid
  #   "z"  | boolean | True if file1 exists and has a zero length
  #
  # Tests that take two files:
  #
  #   "-"  | boolean | True if file1 and file2 are identical
  #   "="  | boolean | True if the modification times of file1
  #        |         | and file2 are equal
  #   "<"  | boolean | True if the modification time of file1
  #        |         | is prior to that of file2
  #   ">"  | boolean | True if the modification time of file1
  #        |         | is after that of file2
  def self.test(*args) end

  # Transfers control to the end of the active +catch+ block
  # waiting for _tag_. Raises +UncaughtThrowError+ if there
  # is no +catch+ block for the _tag_. The optional second
  # parameter supplies a return value for the +catch+ block,
  # which otherwise defaults to +nil+. For examples, see
  # Kernel::catch.
  def self.throw(p1, p2 = v2) end

  # Controls tracing of assignments to global variables. The parameter
  # +symbol+ identifies the variable (as either a string name or a
  # symbol identifier). _cmd_ (which may be a string or a
  # +Proc+ object) or block is executed whenever the variable
  # is assigned. The block or +Proc+ object receives the
  # variable's new value as a parameter. Also see
  # Kernel::untrace_var.
  #
  #    trace_var :$_, proc {|v| puts "$_ is now '#{v}'" }
  #    $_ = "hello"
  #    $_ = ' there'
  #
  # <em>produces:</em>
  #
  #    $_ is now 'hello'
  #    $_ is now ' there'
  def self.trace_var(...) end

  # Specifies the handling of signals. The first parameter is a signal
  # name (a string such as ``SIGALRM'', ``SIGUSR1'', and so on) or a
  # signal number. The characters ``SIG'' may be omitted from the
  # signal name. The command or block specifies code to be run when the
  # signal is raised.
  # If the command is the string ``IGNORE'' or ``SIG_IGN'', the signal
  # will be ignored.
  # If the command is ``DEFAULT'' or ``SIG_DFL'', the Ruby's default handler
  # will be invoked.
  # If the command is ``EXIT'', the script will be terminated by the signal.
  # If the command is ``SYSTEM_DEFAULT'', the operating system's default
  # handler will be invoked.
  # Otherwise, the given command or block will be run.
  # The special signal name ``EXIT'' or signal number zero will be
  # invoked just prior to program termination.
  # trap returns the previous handler for the given signal.
  #
  #     Signal.trap(0, proc { puts "Terminating: #{$$}" })
  #     Signal.trap("CLD")  { puts "Child died" }
  #     fork && Process.wait
  #
  # <em>produces:</em>
  #     Terminating: 27461
  #     Child died
  #     Terminating: 27460
  def self.trap(...) end

  # Removes tracing for the specified command on the given global
  # variable and returns +nil+. If no command is specified,
  # removes all tracing for that variable and returns an array
  # containing the commands actually removed.
  def self.untrace_var(symbol, *cmd) end

  # Returns true if two objects do not match (using the <i>=~</i>
  # method), otherwise false.
  def !~(other) end

  # Returns 0 if +obj+ and +other+ are the same object
  # or <code>obj == other</code>, otherwise nil.
  #
  # The #<=> is used by various methods to compare objects, for example
  # Enumerable#sort, Enumerable#max etc.
  #
  # Your implementation of #<=> should return one of the following values: -1, 0,
  # 1 or nil. -1 means self is smaller than other. 0 means self is equal to other.
  # 1 means self is bigger than other. Nil means the two values could not be
  # compared.
  #
  # When you define #<=>, you can include Comparable to gain the
  # methods #<=, #<, #==, #>=, #> and #between?.
  def <=>(other) end

  # Returns +true+ or +false+.
  #
  # Like Object#==, if +object+ is an instance of Object
  # (and not an instance of one of its many subclasses).
  #
  # This method is commonly overridden by those subclasses,
  # to provide meaningful semantics in +case+ statements.
  def ===(other) end

  # Returns <i>arg</i> converted to a float. Numeric types are
  # converted directly, and with exception to String and
  # <code>nil</code> the rest are converted using
  # <i>arg</i><code>.to_f</code>.  Converting a String with invalid
  # characters will result in a ArgumentError.  Converting
  # <code>nil</code> generates a TypeError.  Exceptions can be
  # suppressed by passing <code>exception: false</code>.
  #
  #    Float(1)                 #=> 1.0
  #    Float("123.456")         #=> 123.456
  #    Float("123.0_badstring") #=> ArgumentError: invalid value for Float(): "123.0_badstring"
  #    Float(nil)               #=> TypeError: can't convert nil into Float
  #    Float("123.0_badstring", exception: false)  #=> nil
  def Float(arg, exception: true) end

  # Returns an integer converted from +object+.
  #
  # Tries to convert +object+ to an integer
  # using +to_int+ first and +to_i+ second;
  # see below for exceptions.
  #
  # With a non-zero +base+, +object+ must be a string or convertible
  # to a string.
  #
  # ==== numeric objects
  #
  # With integer argument +object+ given, returns +object+:
  #
  #   Integer(1)                # => 1
  #   Integer(-1)               # => -1
  #
  # With floating-point argument +object+ given,
  # returns +object+ truncated to an integer:
  #
  #   Integer(1.9)              # => 1  # Rounds toward zero.
  #   Integer(-1.9)             # => -1 # Rounds toward zero.
  #
  # ==== string objects
  #
  # With string argument +object+ and zero +base+ given,
  # returns +object+ converted to an integer in base 10:
  #
  #   Integer('100')    # => 100
  #   Integer('-100')   # => -100
  #
  # With +base+ zero, string +object+ may contain leading characters
  # to specify the actual base (radix indicator):
  #
  #   Integer('0100')  # => 64  # Leading '0' specifies base 8.
  #   Integer('0b100') # => 4   # Leading '0b', specifies base 2.
  #   Integer('0x100') # => 256 # Leading '0x' specifies base 16.
  #
  # With a positive +base+ (in range 2..36) given, returns +object+
  # converted to an integer in the given base:
  #
  #   Integer('100', 2)   # => 4
  #   Integer('100', 8)   # => 64
  #   Integer('-100', 16) # => -256
  #
  # With a negative +base+ (in range -36..-2) given, returns +object+
  # converted to an integer in the radix indicator if exists or
  # +-base+:
  #
  #   Integer('0x100', -2)   # => 256
  #   Integer('100', -2)     # => 4
  #   Integer('0b100', -8)   # => 4
  #   Integer('100', -8)     # => 64
  #   Integer('0o100', -10)  # => 64
  #   Integer('100', -10)    # => 100
  #
  # +base+ -1 is equal the -10 case.
  #
  # When converting strings, surrounding whitespace and embedded underscores
  # are allowed and ignored:
  #
  #   Integer(' 100 ')      # => 100
  #   Integer('-1_0_0', 16) # => -256
  #
  # ==== other classes
  #
  # Examples with +object+ of various other classes:
  #
  #   Integer(Rational(9, 10)) # => 0  # Rounds toward zero.
  #   Integer(Complex(2, 0))   # => 2  # Imaginary part must be zero.
  #   Integer(Time.now)        # => 1650974042
  #
  # ==== keywords
  #
  # With optional keyword argument +exception+ given as +true+ (the default):
  #
  # - Raises TypeError if +object+ does not respond to +to_int+ or +to_i+.
  # - Raises TypeError if +object+ is +nil+.
  # - Raise ArgumentError if +object+ is an invalid string.
  #
  # With +exception+ given as +false+, an exception of any kind is suppressed
  # and +nil+ is returned.
  def Integer(arg, base = 0, exception: true) end

  # Returns the class of <i>obj</i>. This method must always be called
  # with an explicit receiver, as #class is also a reserved word in
  # Ruby.
  #
  #    1.class      #=> Integer
  #    self.class   #=> Object
  def class; end

  # Produces a shallow copy of <i>obj</i>---the instance variables of
  # <i>obj</i> are copied, but not the objects they reference.
  # #clone copies the frozen value state of <i>obj</i>, unless the
  # +:freeze+ keyword argument is given with a false or true value.
  # See also the discussion under Object#dup.
  #
  #    class Klass
  #       attr_accessor :str
  #    end
  #    s1 = Klass.new      #=> #<Klass:0x401b3a38>
  #    s1.str = "Hello"    #=> "Hello"
  #    s2 = s1.clone       #=> #<Klass:0x401b3998 @str="Hello">
  #    s2.str[1,4] = "i"   #=> "i"
  #    s1.inspect          #=> "#<Klass:0x401b3a38 @str=\"Hi\">"
  #    s2.inspect          #=> "#<Klass:0x401b3998 @str=\"Hi\">"
  #
  # This method may have class-specific behavior.  If so, that
  # behavior will be documented under the #+initialize_copy+ method of
  # the class.
  def clone(freeze: nil) end

  # Defines a public singleton method in the receiver. The _method_
  # parameter can be a +Proc+, a +Method+ or an +UnboundMethod+ object.
  # If a block is specified, it is used as the method body.
  # If a block or a method has parameters, they're used as method parameters.
  #
  #    class A
  #      class << self
  #        def class_name
  #          to_s
  #        end
  #      end
  #    end
  #    A.define_singleton_method(:who_am_i) do
  #      "I am: #{class_name}"
  #    end
  #    A.who_am_i   # ==> "I am: A"
  #
  #    guy = "Bob"
  #    guy.define_singleton_method(:hello) { "#{self}: Hello there!" }
  #    guy.hello    #=>  "Bob: Hello there!"
  #
  #    chris = "Chris"
  #    chris.define_singleton_method(:greet) {|greeting| "#{greeting}, I'm Chris!" }
  #    chris.greet("Hi") #=> "Hi, I'm Chris!"
  def define_singleton_method(...) end

  # Writes +self+ on the given port:
  #
  #    1.display
  #    "cat".display
  #    [ 4, 5, 6 ].display
  #    puts
  #
  # Output:
  #
  #    1cat[4, 5, 6]
  def display(port = $>) end

  # Produces a shallow copy of <i>obj</i>---the instance variables of
  # <i>obj</i> are copied, but not the objects they reference.
  #
  # This method may have class-specific behavior.  If so, that
  # behavior will be documented under the #+initialize_copy+ method of
  # the class.
  #
  # === on dup vs clone
  #
  # In general, #clone and #dup may have different semantics in
  # descendant classes. While #clone is used to duplicate an object,
  # including its internal state, #dup typically uses the class of the
  # descendant object to create the new instance.
  #
  # When using #dup, any modules that the object has been extended with will not
  # be copied.
  #
  #     class Klass
  #       attr_accessor :str
  #     end
  #
  #     module Foo
  #       def foo; 'foo'; end
  #     end
  #
  #     s1 = Klass.new #=> #<Klass:0x401b3a38>
  #     s1.extend(Foo) #=> #<Klass:0x401b3a38>
  #     s1.foo #=> "foo"
  #
  #     s2 = s1.clone #=> #<Klass:0x401be280>
  #     s2.foo #=> "foo"
  #
  #     s3 = s1.dup #=> #<Klass:0x401c1084>
  #     s3.foo #=> NoMethodError: undefined method `foo' for #<Klass:0x401c1084>
  def dup; end

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
  def eql?(other) end

  # Adds to _obj_ the instance methods from each module given as a
  # parameter.
  #
  #    module Mod
  #      def hello
  #        "Hello from Mod.\n"
  #      end
  #    end
  #
  #    class Klass
  #      def hello
  #        "Hello from Klass.\n"
  #      end
  #    end
  #
  #    k = Klass.new
  #    k.hello         #=> "Hello from Klass.\n"
  #    k.extend(Mod)   #=> #<Klass:0x401b3bc8>
  #    k.hello         #=> "Hello from Mod.\n"
  def extend(*args) end

  # Prevents further modifications to <i>obj</i>. A
  # FrozenError will be raised if modification is attempted.
  # There is no way to unfreeze a frozen object. See also
  # Object#frozen?.
  #
  # This method returns self.
  #
  #    a = [ "a", "b", "c" ]
  #    a.freeze
  #    a << "z"
  #
  # <em>produces:</em>
  #
  #    prog.rb:3:in `<<': can't modify frozen Array (FrozenError)
  #     from prog.rb:3
  #
  # Objects of the following classes are always frozen: Integer,
  # Float, Symbol.
  def freeze; end

  # Returns the freeze status of <i>obj</i>.
  #
  #    a = [ "a", "b", "c" ]
  #    a.freeze    #=> ["a", "b", "c"]
  #    a.frozen?   #=> true
  def frozen?; end

  # Generates an Integer hash value for this object.  This function must have the
  # property that <code>a.eql?(b)</code> implies <code>a.hash == b.hash</code>.
  #
  # The hash value is used along with #eql? by the Hash class to determine if
  # two objects reference the same hash key.  Any hash value that exceeds the
  # capacity of an Integer will be truncated before being used.
  #
  # The hash value for an object may not be identical across invocations or
  # implementations of Ruby.  If you need a stable identifier across Ruby
  # invocations and implementations you will need to generate one with a custom
  # method.
  #
  # Certain core classes such as Integer use built-in hash calculations and
  # do not call the #hash method when used as a hash key.
  #
  # When implementing your own #hash based on multiple values, the best
  # practice is to combine the class and any values using the hash code of an
  # array:
  #
  # For example:
  #
  #   def hash
  #     [self.class, a, b, c].hash
  #   end
  #
  # The reason for this is that the Array#hash method already has logic for
  # safely and efficiently combining multiple hash values.
  def hash; end

  # Returns a string containing a human-readable representation of <i>obj</i>.
  # The default #inspect shows the object's class name, an encoding of
  # its memory address, and a list of the instance variables and their
  # values (by calling #inspect on each of them).  User defined classes
  # should override this method to provide a better representation of
  # <i>obj</i>.  When overriding this method, it should return a string
  # whose encoding is compatible with the default external encoding.
  #
  #     [ 1, 2, 3..4, 'five' ].inspect   #=> "[1, 2, 3..4, \"five\"]"
  #     Time.new.inspect                 #=> "2008-03-08 19:43:39 +0900"
  #
  #     class Foo
  #     end
  #     Foo.new.inspect                  #=> "#<Foo:0x0300c868>"
  #
  #     class Bar
  #       def initialize
  #         @bar = 1
  #       end
  #     end
  #     Bar.new.inspect                  #=> "#<Bar:0x0300c868 @bar=1>"
  def inspect; end

  # Returns <code>true</code> if <i>obj</i> is an instance of the given
  # class. See also Object#kind_of?.
  #
  #    class A;     end
  #    class B < A; end
  #    class C < B; end
  #
  #    b = B.new
  #    b.instance_of? A   #=> false
  #    b.instance_of? B   #=> true
  #    b.instance_of? C   #=> false
  def instance_of?(clazz) end

  # Returns <code>true</code> if the given instance variable is
  # defined in <i>obj</i>.
  # String arguments are converted to symbols.
  #
  #    class Fred
  #      def initialize(p1, p2)
  #        @a, @b = p1, p2
  #      end
  #    end
  #    fred = Fred.new('cat', 99)
  #    fred.instance_variable_defined?(:@a)    #=> true
  #    fred.instance_variable_defined?("@b")   #=> true
  #    fred.instance_variable_defined?("@c")   #=> false
  def instance_variable_defined?(...) end

  # Returns the value of the given instance variable, or nil if the
  # instance variable is not set. The <code>@</code> part of the
  # variable name should be included for regular instance
  # variables. Throws a NameError exception if the
  # supplied symbol is not valid as an instance variable name.
  # String arguments are converted to symbols.
  #
  #    class Fred
  #      def initialize(p1, p2)
  #        @a, @b = p1, p2
  #      end
  #    end
  #    fred = Fred.new('cat', 99)
  #    fred.instance_variable_get(:@a)    #=> "cat"
  #    fred.instance_variable_get("@b")   #=> 99
  def instance_variable_get(...) end

  # Sets the instance variable named by <i>symbol</i> to the given
  # object. This may circumvent the encapsulation intended by
  # the author of the class, so it should be used with care.
  # The variable does not have to exist prior to this call.
  # If the instance variable name is passed as a string, that string
  # is converted to a symbol.
  #
  #    class Fred
  #      def initialize(p1, p2)
  #        @a, @b = p1, p2
  #      end
  #    end
  #    fred = Fred.new('cat', 99)
  #    fred.instance_variable_set(:@a, 'dog')   #=> "dog"
  #    fred.instance_variable_set(:@c, 'cat')   #=> "cat"
  #    fred.inspect                             #=> "#<Fred:0x401b3da8 @a=\"dog\", @b=99, @c=\"cat\">"
  def instance_variable_set(...) end

  # Returns an array of instance variable names for the receiver. Note
  # that simply defining an accessor does not create the corresponding
  # instance variable.
  #
  #    class Fred
  #      attr_accessor :a1
  #      def initialize
  #        @iv = 3
  #      end
  #    end
  #    Fred.new.instance_variables   #=> [:@iv]
  def instance_variables; end

  # Returns the receiver.
  #
  #    string = "my string"
  #    string.itself.object_id == string.object_id   #=> true
  def itself; end

  # Returns <code>true</code> if <i>class</i> is the class of
  # <i>obj</i>, or if <i>class</i> is one of the superclasses of
  # <i>obj</i> or modules included in <i>obj</i>.
  #
  #    module M;    end
  #    class A
  #      include M
  #    end
  #    class B < A; end
  #    class C < B; end
  #
  #    b = B.new
  #    b.is_a? A          #=> true
  #    b.is_a? B          #=> true
  #    b.is_a? C          #=> false
  #    b.is_a? M          #=> true
  #
  #    b.kind_of? A       #=> true
  #    b.kind_of? B       #=> true
  #    b.kind_of? C       #=> false
  #    b.kind_of? M       #=> true
  def kind_of?(clazz) end
  alias is_a? kind_of?

  # Repeatedly executes the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    loop do
  #      print "Input: "
  #      line = gets
  #      break if !line or line =~ /^q/i
  #      # ...
  #    end
  #
  # StopIteration raised in the block breaks the loop.  In this case,
  # loop returns the "result" value stored in the exception.
  #
  #    enum = Enumerator.new { |y|
  #      y << "one"
  #      y << "two"
  #      :ok
  #    }
  #
  #    result = loop {
  #      puts enum.next
  #    } #=> :ok
  def loop; end

  # Looks up the named method as a receiver in <i>obj</i>, returning a
  # Method object (or raising NameError). The Method object acts as a
  # closure in <i>obj</i>'s object instance, so instance variables and
  # the value of <code>self</code> remain available.
  #
  #    class Demo
  #      def initialize(n)
  #        @iv = n
  #      end
  #      def hello()
  #        "Hello, @iv = #{@iv}"
  #      end
  #    end
  #
  #    k = Demo.new(99)
  #    m = k.method(:hello)
  #    m.call   #=> "Hello, @iv = 99"
  #
  #    l = Demo.new('Fred')
  #    m = l.method("hello")
  #    m.call   #=> "Hello, @iv = Fred"
  #
  # Note that Method implements <code>to_proc</code> method, which
  # means it can be used with iterators.
  #
  #    [ 1, 2, 3 ].each(&method(:puts)) # => prints 3 lines to stdout
  #
  #    out = File.open('test.txt', 'w')
  #    [ 1, 2, 3 ].each(&out.method(:puts)) # => prints 3 lines to file
  #
  #    require 'date'
  #    %w[2017-03-01 2017-03-02].collect(&Date.method(:parse))
  #    #=> [#<Date: 2017-03-01 ((2457814j,0s,0n),+0s,2299161j)>, #<Date: 2017-03-02 ((2457815j,0s,0n),+0s,2299161j)>]
  def method(sym) end

  # Returns a list of the names of public and protected methods of
  # <i>obj</i>. This will include all the methods accessible in
  # <i>obj</i>'s ancestors.
  # If the optional parameter is <code>false</code>, it
  # returns an array of <i>obj</i>'s public and protected singleton methods,
  # the array will not include methods in modules included in <i>obj</i>.
  #
  #    class Klass
  #      def klass_method()
  #      end
  #    end
  #    k = Klass.new
  #    k.methods[0..9]    #=> [:klass_method, :nil?, :===,
  #                       #    :==~, :!, :eql?
  #                       #    :hash, :<=>, :class, :singleton_class]
  #    k.methods.length   #=> 56
  #
  #    k.methods(false)   #=> []
  #    def k.singleton_method; end
  #    k.methods(false)   #=> [:singleton_method]
  #
  #    module M123; def m123; end end
  #    k.extend M123
  #    k.methods(false)   #=> [:singleton_method]
  def methods(regular = true) end

  # Only the object <i>nil</i> responds <code>true</code> to <code>nil?</code>.
  #
  #    Object.new.nil?   #=> false
  #    nil.nil?          #=> true
  def nil?; end

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
  def object_id; end

  # Returns the list of private methods accessible to <i>obj</i>. If
  # the <i>all</i> parameter is set to <code>false</code>, only those methods
  # in the receiver will be listed.
  def private_methods(all = true) end

  # Returns the list of protected methods accessible to <i>obj</i>. If
  # the <i>all</i> parameter is set to <code>false</code>, only those methods
  # in the receiver will be listed.
  def protected_methods(all = true) end

  # Similar to _method_, searches public method only.
  def public_method(sym) end

  # Returns the list of public methods accessible to <i>obj</i>. If
  # the <i>all</i> parameter is set to <code>false</code>, only those methods
  # in the receiver will be listed.
  def public_methods(all = true) end

  # Invokes the method identified by _symbol_, passing it any
  # arguments specified. Unlike send, public_send calls public
  # methods only.
  # When the method is identified by a string, the string is converted
  # to a symbol.
  #
  #    1.public_send(:puts, "hello")  # causes NoMethodError
  def public_send(...) end

  # Removes the named instance variable from <i>obj</i>, returning that
  # variable's value.
  # String arguments are converted to symbols.
  #
  #    class Dummy
  #      attr_reader :var
  #      def initialize
  #        @var = 99
  #      end
  #      def remove
  #        remove_instance_variable(:@var)
  #      end
  #    end
  #    d = Dummy.new
  #    d.var      #=> 99
  #    d.remove   #=> 99
  #    d.var      #=> nil
  def remove_instance_variable(...) end

  # Returns +true+ if _obj_ responds to the given method.  Private and
  # protected methods are included in the search only if the optional
  # second parameter evaluates to +true+.
  #
  # If the method is not implemented,
  # as Process.fork on Windows, File.lchmod on GNU/Linux, etc.,
  # false is returned.
  #
  # If the method is not defined, <code>respond_to_missing?</code>
  # method is called and the result is returned.
  #
  # When the method name parameter is given as a string, the string is
  # converted to a symbol.
  def respond_to?(...) end

  # DO NOT USE THIS DIRECTLY.
  #
  # Hook method to return whether the _obj_ can respond to _id_ method
  # or not.
  #
  # When the method name parameter is given as a string, the string is
  # converted to a symbol.
  #
  # See #respond_to?, and the example of BasicObject.
  def respond_to_missing?(...) end

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
  def send(...) end

  # Returns the singleton class of <i>obj</i>.  This method creates
  # a new singleton class if <i>obj</i> does not have one.
  #
  # If <i>obj</i> is <code>nil</code>, <code>true</code>, or
  # <code>false</code>, it returns NilClass, TrueClass, or FalseClass,
  # respectively.
  # If <i>obj</i> is an Integer, a Float or a Symbol, it raises a TypeError.
  #
  #    Object.new.singleton_class  #=> #<Class:#<Object:0xb7ce1e24>>
  #    String.singleton_class      #=> #<Class:String>
  #    nil.singleton_class         #=> NilClass
  def singleton_class; end

  # Similar to _method_, searches singleton method only.
  #
  #    class Demo
  #      def initialize(n)
  #        @iv = n
  #      end
  #      def hello()
  #        "Hello, @iv = #{@iv}"
  #      end
  #    end
  #
  #    k = Demo.new(99)
  #    def k.hi
  #      "Hi, @iv = #{@iv}"
  #    end
  #    m = k.singleton_method(:hi)
  #    m.call   #=> "Hi, @iv = 99"
  #    m = k.singleton_method(:hello) #=> NameError
  def singleton_method(sym) end

  # Returns an array of the names of singleton methods for <i>obj</i>.
  # If the optional <i>all</i> parameter is true, the list will include
  # methods in modules included in <i>obj</i>.
  # Only public and protected singleton methods are returned.
  #
  #    module Other
  #      def three() end
  #    end
  #
  #    class Single
  #      def Single.four() end
  #    end
  #
  #    a = Single.new
  #
  #    def a.one()
  #    end
  #
  #    class << a
  #      include Other
  #      def two()
  #      end
  #    end
  #
  #    Single.singleton_methods    #=> [:four]
  #    a.singleton_methods(false)  #=> [:two, :one]
  #    a.singleton_methods         #=> [:two, :one, :three]
  def singleton_methods(all = true) end

  # Yields self to the block, and then returns self.
  # The primary purpose of this method is to "tap into" a method chain,
  # in order to perform operations on intermediate results within the chain.
  #
  #    (1..10)                  .tap {|x| puts "original: #{x}" }
  #      .to_a                  .tap {|x| puts "array:    #{x}" }
  #      .select {|x| x.even? } .tap {|x| puts "evens:    #{x}" }
  #      .map {|x| x*x }        .tap {|x| puts "squares:  #{x}" }
  def tap; end

  # Yields self to the block and returns the result of the block.
  #
  #    3.next.then {|x| x**x }.to_s             #=> "256"
  #
  # Good usage for +then+ is value piping in method chains:
  #
  #    require 'open-uri'
  #    require 'json'
  #
  #    construct_url(arguments).
  #      then {|url| URI(url).read }.
  #      then {|response| JSON.parse(response) }
  #
  # When called without block, the method returns +Enumerator+,
  # which can be used, for example, for conditional
  # circuit-breaking:
  #
  #    # meets condition, no-op
  #    1.then.detect(&:odd?)            # => 1
  #    # does not meet condition, drop value
  #    2.then.detect(&:odd?)            # => nil
  #
  # Good usage for +then+ is value piping in method chains:
  #
  #    require 'open-uri'
  #    require 'json'
  #
  #    construct_url(arguments).
  #      then {|url| URI(url).read }.
  #      then {|response| JSON.parse(response) }
  def then; end

  # Creates a new Enumerator which will enumerate by calling +method+ on
  # +obj+, passing +args+ if any. What was _yielded_ by method becomes
  # values of enumerator.
  #
  # If a block is given, it will be used to calculate the size of
  # the enumerator without the need to iterate it (see Enumerator#size).
  #
  # === Examples
  #
  #   str = "xyz"
  #
  #   enum = str.enum_for(:each_byte)
  #   enum.each { |b| puts b }
  #   # => 120
  #   # => 121
  #   # => 122
  #
  #   # protect an array from being modified by some_method
  #   a = [1, 2, 3]
  #   some_method(a.to_enum)
  #
  #   # String#split in block form is more memory-effective:
  #   very_large_string.split("|") { |chunk| return chunk if chunk.include?('DATE') }
  #   # This could be rewritten more idiomatically with to_enum:
  #   very_large_string.to_enum(:split, "|").lazy.grep(/DATE/).first
  #
  # It is typical to call to_enum when defining methods for
  # a generic Enumerable, in case no block is passed.
  #
  # Here is such an example, with parameter passing and a sizing block:
  #
  #   module Enumerable
  #     # a generic method to repeat the values of any enumerable
  #     def repeat(n)
  #       raise ArgumentError, "#{n} is negative!" if n < 0
  #       unless block_given?
  #         return to_enum(__method__, n) do # __method__ is :repeat here
  #           sz = size     # Call size and multiply by n...
  #           sz * n if sz  # but return nil if size itself is nil
  #         end
  #       end
  #       each do |*val|
  #         n.times { yield *val }
  #       end
  #     end
  #   end
  #
  #   %i[hello world].repeat(2) { |w| puts w }
  #     # => Prints 'hello', 'hello', 'world', 'world'
  #   enum = (1..14).repeat(3)
  #     # => returns an Enumerator when called without a block
  #   enum.first(4) # => [1, 1, 1, 2]
  #   enum.size # => 42
  def to_enum(method = :each, *args) end
  alias enum_for to_enum

  # Returns a string representing <i>obj</i>. The default #to_s prints
  # the object's class and an encoding of the object id. As a special
  # case, the top-level object that is the initial execution context
  # of Ruby programs returns ``main''.
  def to_s; end

  # If warnings have been disabled (for example with the
  # <code>-W0</code> flag), does nothing.  Otherwise,
  # converts each of the messages to strings, appends a newline
  # character to the string if the string does not end in a newline,
  # and calls Warning.warn with the string.
  #
  #    warn("warning 1", "warning 2")
  #
  # <em>produces:</em>
  #
  #    warning 1
  #    warning 2
  #
  # If the <code>uplevel</code> keyword argument is given, the string will
  # be prepended with information for the given caller frame in
  # the same format used by the <code>rb_warn</code> C function.
  #
  #    # In baz.rb
  #    def foo
  #      warn("invalid call to foo", uplevel: 1)
  #    end
  #
  #    def bar
  #      foo
  #    end
  #
  #    bar
  #
  # <em>produces:</em>
  #
  #    baz.rb:6: warning: invalid call to foo
  #
  # If <code>category</code> keyword argument is given, passes the category
  # to <code>Warning.warn</code>.  The category given must be be one of the
  # following categories:
  #
  # :deprecated :: Used for warning for deprecated functionality that may
  #                be removed in the future.
  # :experimental :: Used for experimental features that may change in
  #                  future releases.
  def warn(*msgs, uplevel: nil, category: nil) end

  # Yields self to the block and returns the result of the block.
  #
  #    "my string".yield_self {|s| s.upcase }   #=> "MY STRING"
  def yield_self; end

  private

  # Returns the called name of the current method as a Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def __callee__; end

  # Returns the canonicalized absolute path of the directory of the file from
  # which this method is called. It means symlinks in the path is resolved.
  # If <code>__FILE__</code> is <code>nil</code>, it returns <code>nil</code>.
  # The return value equals to <code>File.dirname(File.realpath(__FILE__))</code>.
  def __dir__; end

  # Returns the name at the definition of the current method as a
  # Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def __method__; end

  # Returns the <tt>$stdout</tt> output from running +command+ in a subshell;
  # sets global variable <tt>$?</tt> to the process status.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Examples:
  #
  #   $ `date`                 # => "Wed Apr  9 08:56:30 CDT 2003\n"
  #   $ `echo oops && exit 99` # => "oops\n"
  #   $ $?                     # => #<Process::Status: pid 17088 exit 99>
  #   $ $?.status              # => 99>
  #
  # The built-in syntax <tt>%x{...}</tt> uses this method.
  def `(command) end

  # Returns an array converted from +object+.
  #
  # Tries to convert +object+ to an array
  # using +to_ary+ first and +to_a+ second:
  #
  #   Array([0, 1, 2])        # => [0, 1, 2]
  #   Array({foo: 0, bar: 1}) # => [[:foo, 0], [:bar, 1]]
  #   Array(0..4)             # => [0, 1, 2, 3, 4]
  #
  # Returns +object+ in an array, <tt>[object]</tt>,
  # if +object+ cannot be converted:
  #
  #   Array(:foo)             # => [:foo]
  def Array(object) end

  # Returns the \BigDecimal converted from +value+
  # with a precision of +ndigits+ decimal digits.
  #
  # When +ndigits+ is less than the number of significant digits
  # in the value, the result is rounded to that number of digits,
  # according to the current rounding mode; see BigDecimal.mode.
  #
  # When +ndigits+ is 0, the number of digits to correctly represent a float number
  # is determined automatically.
  #
  # Returns +value+ converted to a \BigDecimal, depending on the type of +value+:
  #
  # - Integer, Float, Rational, Complex, or BigDecimal: converted directly:
  #
  #     # Integer, Complex, or BigDecimal value does not require ndigits; ignored if given.
  #     BigDecimal(2)                     # => 0.2e1
  #     BigDecimal(Complex(2, 0))         # => 0.2e1
  #     BigDecimal(BigDecimal(2))         # => 0.2e1
  #     # Float or Rational value requires ndigits.
  #     BigDecimal(2.0, 0)                # => 0.2e1
  #     BigDecimal(Rational(2, 1), 0)     # => 0.2e1
  #
  # - String: converted by parsing if it contains an integer or floating-point literal;
  #   leading and trailing whitespace is ignored:
  #
  #     # String does not require ndigits; ignored if given.
  #     BigDecimal('2')     # => 0.2e1
  #     BigDecimal('2.0')   # => 0.2e1
  #     BigDecimal('0.2e1') # => 0.2e1
  #     BigDecimal(' 2.0 ') # => 0.2e1
  #
  # - Other type that responds to method <tt>:to_str</tt>:
  #   first converted to a string, then converted to a \BigDecimal, as above.
  #
  # - Other type:
  #
  #   - Raises an exception if keyword argument +exception+ is +true+.
  #   - Returns +nil+ if keyword argument +exception+ is +false+.
  #
  # Raises an exception if +value+ evaluates to a Float
  # and +digits+ is larger than Float::DIG + 1.
  def BigDecimal(...) end

  # Returns a new \Complex object if the arguments are valid;
  # otherwise raises an exception if +exception+ is +true+;
  # otherwise returns +nil+.
  #
  # With Numeric arguments +real+ and +imag+,
  # returns <tt>Complex.rect(real, imag)</tt> if the arguments are valid.
  #
  # With string argument +s+, returns a new \Complex object if the argument is valid;
  # the string may have:
  #
  # - One or two numeric substrings,
  #   each of which specifies a Complex, Float, Integer, Numeric, or Rational value,
  #   specifying {rectangular coordinates}[rdoc-ref:Complex@Rectangular+Coordinates]:
  #
  #   - Sign-separated real and imaginary numeric substrings
  #     (with trailing character <tt>'i'</tt>):
  #
  #       Complex('1+2i')  # => (1+2i)
  #       Complex('+1+2i') # => (1+2i)
  #       Complex('+1-2i') # => (1-2i)
  #       Complex('-1+2i') # => (-1+2i)
  #       Complex('-1-2i') # => (-1-2i)
  #
  #   - Real-only numeric string (without trailing character <tt>'i'</tt>):
  #
  #       Complex('1')  # => (1+0i)
  #       Complex('+1') # => (1+0i)
  #       Complex('-1') # => (-1+0i)
  #
  #   - Imaginary-only numeric string (with trailing character <tt>'i'</tt>):
  #
  #       Complex('1i')  # => (0+1i)
  #       Complex('+1i') # => (0+1i)
  #       Complex('-1i') # => (0-1i)
  #
  # - At-sign separated real and imaginary rational substrings,
  #   each of which specifies a Rational value,
  #   specifying {polar coordinates}[rdoc-ref:Complex@Polar+Coordinates]:
  #
  #     Complex('1/2@3/4')   # => (0.36584443443691045+0.34081938001166706i)
  #     Complex('+1/2@+3/4') # => (0.36584443443691045+0.34081938001166706i)
  #     Complex('+1/2@-3/4') # => (0.36584443443691045-0.34081938001166706i)
  #     Complex('-1/2@+3/4') # => (-0.36584443443691045-0.34081938001166706i)
  #     Complex('-1/2@-3/4') # => (-0.36584443443691045+0.34081938001166706i)
  def Complex(...) end

  # Returns a hash converted from +object+.
  #
  # - If +object+ is:
  #
  #   - A hash, returns +object+.
  #   - An empty array or +nil+, returns an empty hash.
  #
  # - Otherwise, if <tt>object.to_hash</tt> returns a hash, returns that hash.
  # - Otherwise, returns TypeError.
  #
  # Examples:
  #
  #   Hash({foo: 0, bar: 1}) # => {:foo=>0, :bar=>1}
  #   Hash(nil)              # => {}
  #   Hash([])               # => {}
  def Hash(object) end

  # Creates a new Pathname object from the given string, +path+, and returns
  # pathname object.
  #
  # In order to use this constructor, you must first require the Pathname
  # standard library extension.
  #
  #      require 'pathname'
  #      Pathname("/home/zzak")
  #      #=> #<Pathname:/home/zzak>
  #
  # See also Pathname::new for more information.
  def Pathname(path) end

  # Returns +x/y+ or +arg+ as a Rational.
  #
  #    Rational(2, 3)   #=> (2/3)
  #    Rational(5)      #=> (5/1)
  #    Rational(0.5)    #=> (1/2)
  #    Rational(0.3)    #=> (5404319552844595/18014398509481984)
  #
  #    Rational("2/3")  #=> (2/3)
  #    Rational("0.3")  #=> (3/10)
  #
  #    Rational("10 cents")  #=> ArgumentError
  #    Rational(nil)         #=> TypeError
  #    Rational(1, nil)      #=> TypeError
  #
  #    Rational("10 cents", exception: false)  #=> nil
  #
  # Syntax of the string form:
  #
  #   string form = extra spaces , rational , extra spaces ;
  #   rational = [ sign ] , unsigned rational ;
  #   unsigned rational = numerator | numerator , "/" , denominator ;
  #   numerator = integer part | fractional part | integer part , fractional part ;
  #   denominator = digits ;
  #   integer part = digits ;
  #   fractional part = "." , digits , [ ( "e" | "E" ) , [ sign ] , digits ] ;
  #   sign = "-" | "+" ;
  #   digits = digit , { digit | "_" , digit } ;
  #   digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
  #   extra spaces = ? \s* ? ;
  #
  # See also String#to_r.
  def Rational(...) end

  # Returns a string converted from +object+.
  #
  # Tries to convert +object+ to a string
  # using +to_str+ first and +to_s+ second:
  #
  #   String([0, 1, 2])        # => "[0, 1, 2]"
  #   String(0..5)             # => "0..5"
  #   String({foo: 0, bar: 1}) # => "{:foo=>0, :bar=>1}"
  #
  # Raises +TypeError+ if +object+ cannot be converted to a string.
  def String(object) end

  # Terminates execution immediately, effectively by calling
  # <tt>Kernel.exit(false)</tt>.
  #
  # If string argument +msg+ is given,
  # it is written to STDERR prior to termination;
  # otherwise, if an exception was raised,
  # prints its message and backtrace.
  def abort(...) end

  # Converts _block_ to a +Proc+ object (and therefore
  # binds it at the point of call) and registers it for execution when
  # the program exits. If multiple handlers are registered, they are
  # executed in reverse order of registration.
  #
  #    def do_at_exit(str1)
  #      at_exit { print str1 }
  #    end
  #    at_exit { puts "cruel world" }
  #    do_at_exit("goodbye ")
  #    exit
  #
  # <em>produces:</em>
  #
  #    goodbye cruel world
  def at_exit; end

  #  Registers _filename_ to be loaded (using Kernel::require)
  #  the first time that _const_ (which may be a String or
  #  a symbol) is accessed.
  #
  #     autoload(:MyModule, "/usr/local/lib/modules/my_module.rb")
  #
  # If _const_ is defined as autoload, the file name to be loaded is
  # replaced with _filename_.  If _const_ is defined but not as
  # autoload, does nothing.
  def autoload(module_, filename) end

  # Returns _filename_ to be loaded if _name_ is registered as
  # +autoload+.
  #
  #    autoload(:B, "b")
  #    autoload?(:B)            #=> "b"
  def autoload?(name, inherit = true) end

  # Returns a Binding object, describing the variable and
  # method bindings at the point of call. This object can be used when
  # calling Binding#eval to execute the evaluated command in this
  # environment, or extracting its local variables.
  #
  #    class User
  #      def initialize(name, position)
  #        @name = name
  #        @position = position
  #      end
  #
  #      def get_binding
  #        binding
  #      end
  #    end
  #
  #    user = User.new('Joan', 'manager')
  #    template = '{name: @name, position: @position}'
  #
  #    # evaluate template in context of the object
  #    eval(template, user.get_binding)
  #    #=> {:name=>"Joan", :position=>"manager"}
  #
  # Binding#local_variable_get can be used to access the variables
  # whose names are reserved Ruby keywords:
  #
  #    # This is valid parameter declaration, but `if` parameter can't
  #    # be accessed by name, because it is a reserved word.
  #    def validate(field, validation, if: nil)
  #      condition = binding.local_variable_get('if')
  #      return unless condition
  #
  #      # ...Some implementation ...
  #    end
  #
  #    validate(:name, :empty?, if: false) # skips validation
  #    validate(:name, :empty?, if: true) # performs validation
  def binding; end

  # Returns <code>true</code> if <code>yield</code> would execute a
  # block in the current context. The <code>iterator?</code> form
  # is mildly deprecated.
  #
  #    def try
  #      if block_given?
  #        yield
  #      else
  #        "no block"
  #      end
  #    end
  #    try                  #=> "no block"
  #    try { "hello" }      #=> "hello"
  #    try do "hello" end   #=> "hello"
  def block_given?; end

  # Generates a Continuation object, which it passes to
  # the associated block. You need to <code>require
  # 'continuation'</code> before using this method. Performing a
  # <em>cont</em><code>.call</code> will cause the #callcc
  # to return (as will falling through the end of the block). The
  # value returned by the #callcc is the value of the
  # block, or the value passed to <em>cont</em><code>.call</code>. See
  # class Continuation for more details. Also see
  # Kernel#throw for an alternative mechanism for
  # unwinding a call stack.
  def callcc; end

  # Returns the current execution stack---an array containing strings in
  # the form <code>file:line</code> or <code>file:line: in
  # `method'</code>.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  #
  #    def a(skip)
  #      caller(skip)
  #    end
  #    def b(skip)
  #      a(skip)
  #    end
  #    def c(skip)
  #      b(skip)
  #    end
  #    c(0)   #=> ["prog:2:in `a'", "prog:5:in `b'", "prog:8:in `c'", "prog:10:in `<main>'"]
  #    c(1)   #=> ["prog:5:in `b'", "prog:8:in `c'", "prog:11:in `<main>'"]
  #    c(2)   #=> ["prog:8:in `c'", "prog:12:in `<main>'"]
  #    c(3)   #=> ["prog:13:in `<main>'"]
  #    c(4)   #=> []
  #    c(5)   #=> nil
  def caller(...) end

  # Returns the current execution stack---an array containing
  # backtrace location objects.
  #
  # See Thread::Backtrace::Location for more information.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  def caller_locations(...) end

  # +catch+ executes its block. If +throw+ is not called, the block executes
  # normally, and +catch+ returns the value of the last expression evaluated.
  #
  #    catch(1) { 123 }            # => 123
  #
  # If <code>throw(tag2, val)</code> is called, Ruby searches up its stack for
  # a +catch+ block whose +tag+ has the same +object_id+ as _tag2_. When found,
  # the block stops executing and returns _val_ (or +nil+ if no second argument
  # was given to +throw+).
  #
  #    catch(1) { throw(1, 456) }  # => 456
  #    catch(1) { throw(1) }       # => nil
  #
  # When +tag+ is passed as the first argument, +catch+ yields it as the
  # parameter of the block.
  #
  #    catch(1) {|x| x + 2 }       # => 3
  #
  # When no +tag+ is given, +catch+ yields a new unique object (as from
  # +Object.new+) as the block parameter. This object can then be used as the
  # argument to +throw+, and will match the correct +catch+ block.
  #
  #    catch do |obj_A|
  #      catch do |obj_B|
  #        throw(obj_B, 123)
  #        puts "This puts is not reached"
  #      end
  #
  #      puts "This puts is displayed"
  #      456
  #    end
  #
  #    # => 456
  #
  #    catch do |obj_A|
  #      catch do |obj_B|
  #        throw(obj_A, 123)
  #        puts "This puts is still not reached"
  #      end
  #
  #      puts "Now this puts is also not reached"
  #      456
  #    end
  #
  #    # => 123
  def catch(*tag) end

  # Equivalent to <code>$_ = $_.chomp(<em>string</em>)</code>. See
  # String#chomp.
  # Available only when -p/-n command line option specified.
  def chomp(...) end

  # Equivalent to <code>($_.dup).chop!</code>, except <code>nil</code>
  # is never returned. See String#chop!.
  # Available only when -p/-n command line option specified.
  def chop; end

  # Evaluates the Ruby expression(s) in <em>string</em>. If
  # <em>binding</em> is given, which must be a Binding object, the
  # evaluation is performed in its context. If the optional
  # <em>filename</em> and <em>lineno</em> parameters are present, they
  # will be used when reporting syntax errors.
  #
  #    def get_binding(str)
  #      return binding
  #    end
  #    str = "hello"
  #    eval "str + ' Fred'"                      #=> "hello Fred"
  #    eval "str + ' Fred'", get_binding("bye")  #=> "bye Fred"
  def eval(string, binding = _, filename = _, lineno = _) end

  # Replaces the current process by doing one of the following:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The new process is created using the
  # {exec system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/execve.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   exec('if true; then echo "Foo"; fi') # Shell reserved word.
  #   exec('echo')                         # Built-in.
  #   exec('date > date.tmp')              # Contains meta character.
  #
  # The command line may also contain arguments and options for the command:
  #
  #   exec('echo "Foo"')
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called.
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process.
  #
  # Example:
  #
  #   exec('/usr/bin/date')
  #
  # Output:
  #
  #   Sat Aug 26 09:38:00 AM CDT 2023
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion:
  #
  #   exec('doesnt_exist') # Raises Errno::ENOENT
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   exec('echo', 'C*')
  #   exec('echo', 'hello', 'world')
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def exec(...) end

  # Initiates termination of the Ruby script by raising SystemExit;
  # the exception may be caught.
  # Returns exit status +status+ to the underlying operating system.
  #
  # Values +true+ and +false+ for argument +status+
  # indicate, respectively, success and failure;
  # The meanings of integer values are system-dependent.
  #
  # Example:
  #
  #   begin
  #     exit
  #     puts 'Never get here.'
  #   rescue SystemExit
  #     puts 'Rescued a SystemExit exception.'
  #   end
  #   puts 'After begin block.'
  #
  # Output:
  #
  #   Rescued a SystemExit exception.
  #   After begin block.
  #
  # Just prior to final termination,
  # Ruby executes any at-exit procedures (see Kernel::at_exit)
  # and any object finalizers (see ObjectSpace::define_finalizer).
  #
  # Example:
  #
  #   at_exit { puts 'In at_exit function.' }
  #   ObjectSpace.define_finalizer('string', proc { puts 'In finalizer.' })
  #   exit
  #
  # Output:
  #
  #    In at_exit function.
  #    In finalizer.
  def exit(status = true) end

  # Exits the process immediately; no exit handlers are called.
  # Returns exit status +status+ to the underlying operating system.
  #
  #    Process.exit!(true)
  #
  # Values +true+ and +false+ for argument +status+
  # indicate, respectively, success and failure;
  # The meanings of integer values are system-dependent.
  def exit!(status = false) end

  # Creates a child process.
  #
  # With a block given, runs the block in the child process;
  # on block exit, the child terminates with a status of zero:
  #
  #   puts "Before the fork: #{Process.pid}"
  #   fork do
  #     puts "In the child process: #{Process.pid}"
  #   end                   # => 382141
  #   puts "After the fork: #{Process.pid}"
  #
  # Output:
  #
  #   Before the fork: 420496
  #   After the fork: 420496
  #   In the child process: 420520
  #
  # With no block given, the +fork+ call returns twice:
  #
  # - Once in the parent process, returning the pid of the child process.
  # - Once in the child process, returning +nil+.
  #
  # Example:
  #
  #   puts "This is the first line before the fork (pid #{Process.pid})"
  #   puts fork
  #   puts "This is the second line after the fork (pid #{Process.pid})"
  #
  # Output:
  #
  #   This is the first line before the fork (pid 420199)
  #   420223
  #   This is the second line after the fork (pid 420199)
  #
  #   This is the second line after the fork (pid 420223)
  #
  # In either case, the child process may exit using
  # Kernel.exit! to avoid the call to Kernel#at_exit.
  #
  # To avoid zombie processes, the parent process should call either:
  #
  # - Process.wait, to collect the termination statuses of its children.
  # - Process.detach, to register disinterest in their status.
  #
  # The thread calling +fork+ is the only thread in the created child process;
  # +fork+ doesn't copy other threads.
  #
  # Note that method +fork+ is available on some platforms,
  # but not on others:
  #
  #   Process.respond_to?(:fork) # => true # Would be false on some.
  #
  # If not, you may use ::spawn instead of +fork+.
  def fork; end

  # Returns (and assigns to <code>$_</code>) the next line from the list
  # of files in +ARGV+ (or <code>$*</code>), or from standard input if
  # no files are present on the command line. Returns +nil+ at end of
  # file. The optional argument specifies the record separator. The
  # separator is included with the contents of each record. A separator
  # of +nil+ reads the entire contents, and a zero-length separator
  # reads the input one paragraph at a time, where paragraphs are
  # divided by two consecutive newlines.  If the first argument is an
  # integer, or optional second argument is given, the returning string
  # would not be longer than the given value in bytes.  If multiple
  # filenames are present in +ARGV+, <code>gets(nil)</code> will read
  # the contents one file at a time.
  #
  #    ARGV << "testfile"
  #    print while gets
  #
  # <em>produces:</em>
  #
  #    This is line one
  #    This is line two
  #    This is line three
  #    And so on...
  #
  # The style of programming using <code>$_</code> as an implicit
  # parameter is gradually losing favor in the Ruby community.
  def gets(...) end

  # Returns an array of the names of global variables. This includes
  # special regexp global variables such as <tt>$~</tt> and <tt>$+</tt>,
  # but does not include the numbered regexp global variables (<tt>$1</tt>,
  # <tt>$2</tt>, etc.).
  #
  #    global_variables.grep /std/   #=> [:$stdin, :$stdout, :$stderr]
  def global_variables; end

  # Equivalent to <code>$_.gsub...</code>, except that <code>$_</code>
  # will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def gsub(...) end

  # Deprecated.  Use block_given? instead.
  def iterator?; end

  # Equivalent to Proc.new, except the resulting Proc objects check the
  # number of parameters passed when called.
  def lambda; end

  # Loads and executes the Ruby program in the file _filename_.
  #
  # If the filename is an absolute path (e.g. starts with '/'), the file
  # will be loaded directly using the absolute path.
  #
  # If the filename is an explicit relative path (e.g. starts with './' or
  # '../'), the file will be loaded using the relative path from the current
  # directory.
  #
  # Otherwise, the file will be searched for in the library
  # directories listed in <code>$LOAD_PATH</code> (<code>$:</code>).
  # If the file is found in a directory, it will attempt to load the file
  # relative to that directory.  If the file is not found in any of the
  # directories in <code>$LOAD_PATH</code>, the file will be loaded using
  # the relative path from the current directory.
  #
  # If the file doesn't exist when there is an attempt to load it, a
  # LoadError will be raised.
  #
  # If the optional _wrap_ parameter is +true+, the loaded script will
  # be executed under an anonymous module, protecting the calling
  # program's global namespace.  If the optional _wrap_ parameter is a
  # module, the loaded script will be executed under the given module.
  # In no circumstance will any local variables in the loaded file be
  # propagated to the loading environment.
  def load(filename, wrap = false) end

  # Returns the names of the current local variables.
  #
  #    fred = 1
  #    for i in 1..10
  #       # ...
  #    end
  #    local_variables   #=> [:fred, :i]
  def local_variables; end

  # Creates an IO object connected to the given file.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # With no block given, file stream is returned:
  #
  #   open('t.txt') # => #<File:t.txt>
  #
  # With a block given, calls the block with the open file stream,
  # then closes the stream:
  #
  #   open('t.txt') {|f| p f } # => #<File:t.txt (closed)>
  #
  # Output:
  #
  #   #<File:t.txt>
  #
  # See File.open for details.
  def open(path, mode = 'r', perm = 0o666, **opts) end

  # For each object +obj+, executes:
  #
  #   $stdout.write(obj.inspect, "\n")
  #
  # With one object given, returns the object;
  # with multiple objects given, returns an array containing the objects;
  # with no object given, returns +nil+.
  #
  # Examples:
  #
  #   r = Range.new(0, 4)
  #   p r                 # => 0..4
  #   p [r, r, r]         # => [0..4, 0..4, 0..4]
  #   p                   # => nil
  #
  # Output:
  #
  #    0..4
  #    [0..4, 0..4, 0..4]
  #
  # Kernel#p is designed for debugging purposes.
  # Ruby implementations may define Kernel#p to be uninterruptible
  # in whole or in part.
  # On CRuby, Kernel#p's writing of data is uninterruptible.
  def p(...) end

  def pp(*objs) end

  # Equivalent to <tt>$stdout.print(*objects)</tt>,
  # this method is the straightforward way to write to <tt>$stdout</tt>.
  #
  # Writes the given objects to <tt>$stdout</tt>; returns +nil+.
  # Appends the output record separator <tt>$OUTPUT_RECORD_SEPARATOR</tt>
  # <tt>$\\</tt>), if it is not +nil+.
  #
  # With argument +objects+ given, for each object:
  #
  # - Converts via its method +to_s+ if not a string.
  # - Writes to <tt>stdout</tt>.
  # - If not the last object, writes the output field separator
  #   <tt>$OUTPUT_FIELD_SEPARATOR</tt> (<tt>$,</tt> if it is not +nil+.
  #
  # With default separators:
  #
  #   objects = [0, 0.0, Rational(0, 1), Complex(0, 0), :zero, 'zero']
  #   $OUTPUT_RECORD_SEPARATOR
  #   $OUTPUT_FIELD_SEPARATOR
  #   print(*objects)
  #
  # Output:
  #
  #   nil
  #   nil
  #   00.00/10+0izerozero
  #
  # With specified separators:
  #
  #   $OUTPUT_RECORD_SEPARATOR = "\n"
  #   $OUTPUT_FIELD_SEPARATOR = ','
  #   print(*objects)
  #
  # Output:
  #
  #   0,0.0,0/1,0+0i,zero,zero
  #
  # With no argument given, writes the content of <tt>$_</tt>
  # (which is usually the most recent user input):
  #
  #   gets  # Sets $_ to the most recent user input.
  #   print # Prints $_.
  def print(*objects) end

  # Equivalent to:
  #
  #   io.write(sprintf(format_string, *objects))
  #
  # For details on +format_string+, see
  # {Format Specifications}[rdoc-ref:format_specifications.rdoc].
  #
  # With the single argument +format_string+, formats +objects+ into the string,
  # then writes the formatted string to $stdout:
  #
  #   printf('%4.4d %10s %2.2f', 24, 24, 24.0)
  #
  # Output (on $stdout):
  #
  #   0024         24 24.00#
  #
  # With arguments +io+ and +format_string+, formats +objects+ into the string,
  # then writes the formatted string to +io+:
  #
  #   printf($stderr, '%4.4d %10s %2.2f', 24, 24, 24.0)
  #
  # Output (on $stderr):
  #
  #   0024         24 24.00# => nil
  #
  # With no arguments, does nothing.
  def printf(...) end

  # Equivalent to Proc.new.
  def proc; end

  # Equivalent to:
  #
  #   $stdout.putc(int)
  #
  # See IO#putc for important information regarding multi-byte characters.
  def putc(int) end

  # Equivalent to
  #
  #    $stdout.puts(objects)
  def puts(*objects) end

  # With no arguments, raises the exception in <code>$!</code> or raises
  # a RuntimeError if <code>$!</code> is +nil+.  With a single +String+
  # argument, raises a +RuntimeError+ with the string as a message. Otherwise,
  # the first parameter should be an +Exception+ class (or another
  # object that returns an +Exception+ object when sent an +exception+
  # message).  The optional second parameter sets the message associated with
  # the exception (accessible via Exception#message), and the third parameter
  # is an array of callback information (accessible via Exception#backtrace).
  # The +cause+ of the generated exception (accessible via Exception#cause)
  # is automatically set to the "current" exception (<code>$!</code>), if any.
  # An alternative value, either an +Exception+ object or +nil+, can be
  # specified via the +:cause+ argument.
  #
  # Exceptions are caught by the +rescue+ clause of
  # <code>begin...end</code> blocks.
  #
  #    raise "Failed to create socket"
  #    raise ArgumentError, "No parameters", caller
  def raise(...) end
  alias fail raise

  # If called without an argument, or if <tt>max.to_i.abs == 0</tt>, rand
  # returns a pseudo-random floating point number between 0.0 and 1.0,
  # including 0.0 and excluding 1.0.
  #
  #   rand        #=> 0.2725926052826416
  #
  # When +max.abs+ is greater than or equal to 1, +rand+ returns a pseudo-random
  # integer greater than or equal to 0 and less than +max.to_i.abs+.
  #
  #   rand(100)   #=> 12
  #
  # When +max+ is a Range, +rand+ returns a random number where
  # <code>range.member?(number) == true</code>.
  #
  # Negative or floating point values for +max+ are allowed, but may give
  # surprising results.
  #
  #   rand(-100) # => 87
  #   rand(-0.5) # => 0.8130921818028143
  #   rand(1.9)  # equivalent to rand(1), which is always 0
  #
  # Kernel.srand may be used to ensure that sequences of random numbers are
  # reproducible between different runs of a program.
  #
  # See also Random.rand.
  def rand(max = 0) end

  # Equivalent to method Kernel#gets, except that it raises an exception
  # if called at end-of-stream:
  #
  #   $ cat t.txt | ruby -e "p readlines; readline"
  #   ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #   in `readline': end of file reached (EOFError)
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted.
  def readline(...) end

  # Returns an array containing the lines returned by calling
  # Kernel#gets until the end-of-stream is reached;
  # (see {Line IO}[rdoc-ref:IO@Line+IO]).
  #
  # With only string argument +sep+ given,
  # returns the remaining lines as determined by line separator +sep+,
  # or +nil+ if none;
  # see {Line Separator}[rdoc-ref:IO@Line+Separator]:
  #
  #   # Default separator.
  #   $ cat t.txt | ruby -e "p readlines"
  #   ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #
  #   # Specified separator.
  #   $ cat t.txt | ruby -e "p readlines 'li'"
  #   ["First li", "ne\nSecond li", "ne\n\nFourth li", "ne\nFifth li", "ne\n"]
  #
  #   # Get-all separator.
  #   $ cat t.txt | ruby -e "p readlines nil"
  #   ["First line\nSecond line\n\nFourth line\nFifth line\n"]
  #
  #   # Get-paragraph separator.
  #   $ cat t.txt | ruby -e "p readlines ''"
  #   ["First line\nSecond line\n\n", "Fourth line\nFifth line\n"]
  #
  # With only integer argument +limit+ given,
  # limits the number of bytes in the line;
  # see {Line Limit}[rdoc-ref:IO@Line+Limit]:
  #
  #   $cat t.txt | ruby -e "p readlines 10"
  #   ["First line", "\n", "Second lin", "e\n", "\n", "Fourth lin", "e\n", "Fifth line", "\n"]
  #
  #   $cat t.txt | ruby -e "p readlines 11"
  #   ["First line\n", "Second line", "\n", "\n", "Fourth line", "\n", "Fifth line\n"]
  #
  #   $cat t.txt | ruby -e "p readlines 12"
  #   ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #
  # With arguments +sep+ and +limit+ given, combines the two behaviors;
  # see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit].
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted:
  #
  #   $ cat t.txt | ruby -e "p readlines(chomp: true)"
  #   ["First line", "Second line", "", "Fourth line", "Fifth line"]
  #
  # Optional keyword arguments +enc_opts+ specify encoding options;
  # see {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def readlines(...) end

  # Loads the given +name+, returning +true+ if successful and +false+ if the
  # feature is already loaded.
  #
  # If the filename neither resolves to an absolute path nor starts with
  # './' or '../', the file will be searched for in the library
  # directories listed in <code>$LOAD_PATH</code> (<code>$:</code>).
  # If the filename starts with './' or '../', resolution is based on Dir.pwd.
  #
  # If the filename has the extension ".rb", it is loaded as a source file; if
  # the extension is ".so", ".o", or the default shared library extension on
  # the current platform, Ruby loads the shared library as a Ruby extension.
  # Otherwise, Ruby tries adding ".rb", ".so", and so on to the name until
  # found.  If the file named cannot be found, a LoadError will be raised.
  #
  # For Ruby extensions the filename given may use ".so" or ".o".  For example,
  # on macOS the socket extension is "socket.bundle" and
  # <code>require 'socket.so'</code> will load the socket extension.
  #
  # The absolute path of the loaded file is added to
  # <code>$LOADED_FEATURES</code> (<code>$"</code>).  A file will not be
  # loaded again if its path already appears in <code>$"</code>.  For example,
  # <code>require 'a'; require './a'</code> will not load <code>a.rb</code>
  # again.
  #
  #   require "my-library.rb"
  #   require "db-driver"
  #
  # Any constants or globals within the loaded source file will be available
  # in the calling program's global namespace. However, local variables will
  # not be propagated to the loading environment.
  def require(name) end

  # Ruby tries to load the library named _string_ relative to the directory
  # containing the requiring file.  If the file does not exist a LoadError is
  # raised. Returns +true+ if the file was loaded and +false+ if the file was
  # already loaded before.
  def require_relative(string) end

  # Invokes system call {select(2)}[https://linux.die.net/man/2/select],
  # which monitors multiple file descriptors,
  # waiting until one or more of the file descriptors
  # becomes ready for some class of I/O operation.
  #
  # Not implemented on all platforms.
  #
  # Each of the arguments +read_ios+, +write_ios+, and +error_ios+
  # is an array of IO objects.
  #
  # Argument +timeout+ is an integer timeout interval in seconds.
  #
  # The method monitors the \IO objects given in all three arrays,
  # waiting for some to be ready;
  # returns a 3-element array whose elements are:
  #
  # - An array of the objects in +read_ios+ that are ready for reading.
  # - An array of the objects in +write_ios+ that are ready for writing.
  # - An array of the objects in +error_ios+ have pending exceptions.
  #
  # If no object becomes ready within the given +timeout+, +nil+ is returned.
  #
  # \IO.select peeks the buffer of \IO objects for testing readability.
  # If the \IO buffer is not empty, \IO.select immediately notifies
  # readability.  This "peek" only happens for \IO objects.  It does not
  # happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use \IO.select is invoking it after non-blocking
  # methods such as #read_nonblock, #write_nonblock, etc.  The methods
  # raise an exception which is extended by IO::WaitReadable or
  # IO::WaitWritable.  The modules notify how the caller should wait
  # with \IO.select.  If IO::WaitReadable is raised, the caller should
  # wait for reading.  If IO::WaitWritable is raised, the caller should
  # wait for writing.
  #
  # So, blocking read (#readpartial) can be emulated using
  # #read_nonblock and \IO.select as follows:
  #
  #   begin
  #     result = io_like.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io_like])
  #     retry
  #   rescue IO::WaitWritable
  #     IO.select(nil, [io_like])
  #     retry
  #   end
  #
  # Especially, the combination of non-blocking methods and \IO.select is
  # preferred for IO like objects such as OpenSSL::SSL::SSLSocket.  It
  # has #to_io method to return underlying IO object.  IO.select calls
  # #to_io to obtain the file descriptor to wait.
  #
  # This means that readability notified by \IO.select doesn't mean
  # readability from OpenSSL::SSL::SSLSocket object.
  #
  # The most likely situation is that OpenSSL::SSL::SSLSocket buffers
  # some data.  \IO.select doesn't see the buffer.  So \IO.select can
  # block when OpenSSL::SSL::SSLSocket#readpartial doesn't block.
  #
  # However, several more complicated situations exist.
  #
  # SSL is a protocol which is sequence of records.
  # The record consists of multiple bytes.
  # So, the remote side of SSL sends a partial record, IO.select
  # notifies readability but OpenSSL::SSL::SSLSocket cannot decrypt a
  # byte and OpenSSL::SSL::SSLSocket#readpartial will block.
  #
  # Also, the remote side can request SSL renegotiation which forces
  # the local SSL engine to write some data.
  # This means OpenSSL::SSL::SSLSocket#readpartial may invoke #write
  # system call and it can block.
  # In such a situation, OpenSSL::SSL::SSLSocket#read_nonblock raises
  # IO::WaitWritable instead of blocking.
  # So, the caller should wait for ready for writability as above
  # example.
  #
  # The combination of non-blocking methods and \IO.select is also useful
  # for streams such as tty, pipe socket socket when multiple processes
  # read from a stream.
  #
  # Finally, Linux kernel developers don't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for a single process;
  # see {select(2)}[https://linux.die.net/man/2/select]
  #
  # Invoking \IO.select before IO#readpartial works well as usual.
  # However it is not the best way to use \IO.select.
  #
  # The writability notified by select(2) doesn't show
  # how many bytes are writable.
  # IO#write method blocks until given whole string is written.
  # So, <tt>IO#write(two or more bytes)</tt> can block after
  # writability is notified by \IO.select.  IO#write_nonblock is required
  # to avoid the blocking.
  #
  # Blocking write (#write) can be emulated using #write_nonblock and
  # IO.select as follows: IO::WaitReadable should also be rescued for
  # SSL renegotiation in OpenSSL::SSL::SSLSocket.
  #
  #   while 0 < string.bytesize
  #     begin
  #       written = io_like.write_nonblock(string)
  #     rescue IO::WaitReadable
  #       IO.select([io_like])
  #       retry
  #     rescue IO::WaitWritable
  #       IO.select(nil, [io_like])
  #       retry
  #     end
  #     string = string.byteslice(written..-1)
  #   end
  #
  # Example:
  #
  #     rp, wp = IO.pipe
  #     mesg = "ping "
  #     100.times {
  #       # IO.select follows IO#read.  Not the best way to use IO.select.
  #       rs, ws, = IO.select([rp], [wp])
  #       if r = rs[0]
  #         ret = r.read(5)
  #         print ret
  #         case ret
  #         when /ping/
  #           mesg = "pong\n"
  #         when /pong/
  #           mesg = "ping "
  #         end
  #       end
  #       if w = ws[0]
  #         w.write(mesg)
  #       end
  #     }
  #
  # Output:
  #
  #     ping pong
  #     ping pong
  #     ping pong
  #     (snipped)
  #     ping
  def select(p1, p2 = v2, p3 = v3, p4 = v4) end

  # Establishes _proc_ as the handler for tracing, or disables
  # tracing if the parameter is +nil+.
  #
  # *Note:* this method is obsolete, please use TracePoint instead.
  #
  # _proc_ takes up to six parameters:
  #
  # * an event name string
  # * a filename string
  # * a line number
  # * a method name symbol, or nil
  # * a binding, or nil
  # * the class, module, or nil
  #
  # _proc_ is invoked whenever an event occurs.
  #
  # Events are:
  #
  # <code>"c-call"</code>:: call a C-language routine
  # <code>"c-return"</code>:: return from a C-language routine
  # <code>"call"</code>:: call a Ruby method
  # <code>"class"</code>:: start a class or module definition
  # <code>"end"</code>:: finish a class or module definition
  # <code>"line"</code>:: execute code on a new line
  # <code>"raise"</code>:: raise an exception
  # <code>"return"</code>:: return from a Ruby method
  #
  # Tracing is disabled within the context of _proc_.
  #
  #   class Test
  #     def test
  #       a = 1
  #       b = 2
  #     end
  #   end
  #
  #   set_trace_func proc { |event, file, line, id, binding, class_or_module|
  #     printf "%8s %s:%-2d %16p %14p\n", event, file, line, id, class_or_module
  #   }
  #   t = Test.new
  #   t.test
  #
  # Produces:
  #
  #   c-return prog.rb:8   :set_trace_func         Kernel
  #       line prog.rb:11              nil            nil
  #     c-call prog.rb:11             :new          Class
  #     c-call prog.rb:11      :initialize    BasicObject
  #   c-return prog.rb:11      :initialize    BasicObject
  #   c-return prog.rb:11             :new          Class
  #       line prog.rb:12              nil            nil
  #       call prog.rb:2             :test           Test
  #       line prog.rb:3             :test           Test
  #       line prog.rb:4             :test           Test
  #     return prog.rb:5             :test           Test
  def set_trace_func(...) end

  # Suspends execution of the current thread for the number of seconds
  # specified by numeric argument +secs+, or forever if +secs+ is +nil+;
  # returns the integer number of seconds suspended (rounded).
  #
  #   Time.new  # => 2008-03-08 19:56:19 +0900
  #   sleep 1.2 # => 1
  #   Time.new  # => 2008-03-08 19:56:20 +0900
  #   sleep 1.9 # => 2
  #   Time.new  # => 2008-03-08 19:56:22 +0900
  def sleep(secs = nil) end

  # Creates a new child process by doing one of the following
  # in that process:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Returns the process ID (pid) of the new process,
  # without waiting for it to complete.
  #
  # To avoid zombie processes, the parent process should call either:
  #
  # - Process.wait, to collect the termination statuses of its children.
  # - Process.detach, to register disinterest in their status.
  #
  # The new process is created using the
  # {exec system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/execve.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   spawn('if true; then echo "Foo"; fi') # => 798847 # Shell reserved word.
  #   Process.wait                          # => 798847
  #   spawn('echo')                         # => 798848 # Built-in.
  #   Process.wait                          # => 798848
  #   spawn('date > /tmp/date.tmp')         # => 798879 # Contains meta character.
  #   Process.wait                          # => 798849
  #   spawn('date > /nop/date.tmp')         # => 798882 # Issues error message.
  #   Process.wait                          # => 798882
  #
  # The command line may also contain arguments and options for the command:
  #
  #   spawn('echo "Foo"') # => 799031
  #   Process.wait        # => 799031
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called:
  #
  #     spawn('/usr/bin/date') # Path to date on Unix-style system.
  #     Process.wait
  #
  #   Output:
  #
  #     Thu Aug 31 10:06:48 AM CDT 2023
  #
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process:
  #
  #     pid = spawn(['sleep', 'Hello!'], '1') # 2-element array.
  #     p `ps -p #{pid} -o command=`
  #
  #   Output:
  #
  #     "Hello! 1\n"
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion.
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   spawn('echo', 'C*')             # => 799392
  #   Process.wait                    # => 799392
  #   spawn('echo', 'hello', 'world') # => 799393
  #   Process.wait                    # => 799393
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def spawn(...) end

  # Returns the string resulting from formatting +objects+
  # into +format_string+.
  #
  # For details on +format_string+, see
  # {Format Specifications}[rdoc-ref:format_specifications.rdoc].
  def sprintf(format_string, *args) end
  alias format sprintf

  # Seeds the system pseudo-random number generator, with +number+.
  # The previous seed value is returned.
  #
  # If +number+ is omitted, seeds the generator using a source of entropy
  # provided by the operating system, if available (/dev/urandom on Unix systems
  # or the RSA cryptographic provider on Windows), which is then combined with
  # the time, the process id, and a sequence number.
  #
  # srand may be used to ensure repeatable sequences of pseudo-random numbers
  # between different runs of the program. By setting the seed to a known value,
  # programs can be made deterministic during testing.
  #
  #   srand 1234               # => 268519324636777531569100071560086917274
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  #   [ rand(10), rand(1000) ] # => [4, 664]
  #   srand 1234               # => 1234
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  def srand(number = Random.new_seed) end

  # Equivalent to <code>$_.sub(<i>args</i>)</code>, except that
  # <code>$_</code> will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def sub(...) end

  # Invokes Posix system call {syscall(2)}[https://linux.die.net/man/2/syscall],
  # which calls a specified function.
  #
  # Calls the operating system function identified by +integer_callno+;
  # returns the result of the function or raises SystemCallError if it failed.
  # The effect of the call is platform-dependent.
  # The arguments and returned value are platform-dependent.
  #
  # For each of +arguments+: if it is an integer, it is passed directly;
  # if it is a string, it is interpreted as a binary sequence of bytes.
  # There may be as many as nine such arguments.
  #
  # Arguments +integer_callno+ and +argument+, as well as the returned value,
  # are platform-dependent.
  #
  # Note: Method +syscall+ is essentially unsafe and unportable.
  # The DL (Fiddle) library is preferred for safer and a bit
  # more portable programming.
  #
  # Not implemented on all platforms.
  def syscall(integer_callno, *arguments) end

  # Creates a new child process by doing one of the following
  # in that process:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Returns:
  #
  # - +true+ if the command exits with status zero.
  # - +false+ if the exit status is a non-zero integer.
  # - +nil+ if the command could not execute.
  #
  # Raises an exception (instead of returning +false+ or +nil+)
  # if keyword argument +exception+ is set to +true+.
  #
  # Assigns the command's error status to <tt>$?</tt>.
  #
  # The new process is created using the
  # {system system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/system.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   system('if true; then echo "Foo"; fi')          # => true  # Shell reserved word.
  #   system('echo')                                  # => true  # Built-in.
  #   system('date > /tmp/date.tmp')                  # => true  # Contains meta character.
  #   system('date > /nop/date.tmp')                  # => false
  #   system('date > /nop/date.tmp', exception: true) # Raises RuntimeError.
  #
  # Assigns the command's error status to <tt>$?</tt>:
  #
  #   system('echo')                             # => true  # Built-in.
  #   $?                                         # => #<Process::Status: pid 640610 exit 0>
  #   system('date > /nop/date.tmp')             # => false
  #   $?                                         # => #<Process::Status: pid 640742 exit 2>
  #
  # The command line may also contain arguments and options for the command:
  #
  #   system('echo "Foo"') # => true
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called.
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process.
  #
  # Example:
  #
  #   system('/usr/bin/date') # => true # Path to date on Unix-style system.
  #   system('foo')           # => nil  # Command failed.
  #
  # Output:
  #
  #   Mon Aug 28 11:43:10 AM CDT 2023
  #
  # Assigns the command's error status to <tt>$?</tt>:
  #
  #   system('/usr/bin/date') # => true
  #   $?                      # => #<Process::Status: pid 645605 exit 0>
  #   system('foo')           # => nil
  #   $?                      # => #<Process::Status: pid 645608 exit 127>
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion:
  #
  #   system('doesnt_exist') # => nil
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   system('echo', 'C*')             # => true
  #   system('echo', 'hello', 'world') # => true
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def system(...) end

  # Uses the character +cmd+ to perform various tests on +file1+ (first
  # table below) or on +file1+ and +file2+ (second table).
  #
  # File tests on a single file:
  #
  #   Cmd    Returns   Meaning
  #   "A"  | Time    | Last access time for file1
  #   "b"  | boolean | True if file1 is a block device
  #   "c"  | boolean | True if file1 is a character device
  #   "C"  | Time    | Last change time for file1
  #   "d"  | boolean | True if file1 exists and is a directory
  #   "e"  | boolean | True if file1 exists
  #   "f"  | boolean | True if file1 exists and is a regular file
  #   "g"  | boolean | True if file1 has the setgid bit set
  #   "G"  | boolean | True if file1 exists and has a group
  #        |         | ownership equal to the caller's group
  #   "k"  | boolean | True if file1 exists and has the sticky bit set
  #   "l"  | boolean | True if file1 exists and is a symbolic link
  #   "M"  | Time    | Last modification time for file1
  #   "o"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's effective uid
  #   "O"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's real uid
  #   "p"  | boolean | True if file1 exists and is a fifo
  #   "r"  | boolean | True if file1 is readable by the effective
  #        |         | uid/gid of the caller
  #   "R"  | boolean | True if file is readable by the real
  #        |         | uid/gid of the caller
  #   "s"  | int/nil | If file1 has nonzero size, return the size,
  #        |         | otherwise return nil
  #   "S"  | boolean | True if file1 exists and is a socket
  #   "u"  | boolean | True if file1 has the setuid bit set
  #   "w"  | boolean | True if file1 exists and is writable by
  #        |         | the effective uid/gid
  #   "W"  | boolean | True if file1 exists and is writable by
  #        |         | the real uid/gid
  #   "x"  | boolean | True if file1 exists and is executable by
  #        |         | the effective uid/gid
  #   "X"  | boolean | True if file1 exists and is executable by
  #        |         | the real uid/gid
  #   "z"  | boolean | True if file1 exists and has a zero length
  #
  # Tests that take two files:
  #
  #   "-"  | boolean | True if file1 and file2 are identical
  #   "="  | boolean | True if the modification times of file1
  #        |         | and file2 are equal
  #   "<"  | boolean | True if the modification time of file1
  #        |         | is prior to that of file2
  #   ">"  | boolean | True if the modification time of file1
  #        |         | is after that of file2
  def test(*args) end

  # Transfers control to the end of the active +catch+ block
  # waiting for _tag_. Raises +UncaughtThrowError+ if there
  # is no +catch+ block for the _tag_. The optional second
  # parameter supplies a return value for the +catch+ block,
  # which otherwise defaults to +nil+. For examples, see
  # Kernel::catch.
  def throw(p1, p2 = v2) end

  # Controls tracing of assignments to global variables. The parameter
  # +symbol+ identifies the variable (as either a string name or a
  # symbol identifier). _cmd_ (which may be a string or a
  # +Proc+ object) or block is executed whenever the variable
  # is assigned. The block or +Proc+ object receives the
  # variable's new value as a parameter. Also see
  # Kernel::untrace_var.
  #
  #    trace_var :$_, proc {|v| puts "$_ is now '#{v}'" }
  #    $_ = "hello"
  #    $_ = ' there'
  #
  # <em>produces:</em>
  #
  #    $_ is now 'hello'
  #    $_ is now ' there'
  def trace_var(...) end

  # Specifies the handling of signals. The first parameter is a signal
  # name (a string such as ``SIGALRM'', ``SIGUSR1'', and so on) or a
  # signal number. The characters ``SIG'' may be omitted from the
  # signal name. The command or block specifies code to be run when the
  # signal is raised.
  # If the command is the string ``IGNORE'' or ``SIG_IGN'', the signal
  # will be ignored.
  # If the command is ``DEFAULT'' or ``SIG_DFL'', the Ruby's default handler
  # will be invoked.
  # If the command is ``EXIT'', the script will be terminated by the signal.
  # If the command is ``SYSTEM_DEFAULT'', the operating system's default
  # handler will be invoked.
  # Otherwise, the given command or block will be run.
  # The special signal name ``EXIT'' or signal number zero will be
  # invoked just prior to program termination.
  # trap returns the previous handler for the given signal.
  #
  #     Signal.trap(0, proc { puts "Terminating: #{$$}" })
  #     Signal.trap("CLD")  { puts "Child died" }
  #     fork && Process.wait
  #
  # <em>produces:</em>
  #     Terminating: 27461
  #     Child died
  #     Terminating: 27460
  def trap(...) end

  # Removes tracing for the specified command on the given global
  # variable and returns +nil+. If no command is specified,
  # removes all tracing for that variable and returns an array
  # containing the commands actually removed.
  def untrace_var(symbol, *cmd) end
end
