# frozen_string_literal: true

# The RubyVM module only exists on MRI. +RubyVM+ is not defined in
# other Ruby implementations such as JRuby and TruffleRuby.
#
# The RubyVM module provides some access to MRI internals.
# This module is for very limited purposes, such as debugging,
# prototyping, and research.  Normal users must not use it.
# This module is not portable between Ruby implementations.
class RubyVM
  # ::RubyVM::DEFAULT_PARAMS
  # This constant exposes the VM's default parameters.
  # Note that changing these values does not affect VM execution.
  # Specification is not stable and you should not depend on this value.
  # Of course, this constant is MRI specific.
  DEFAULT_PARAMS = _
  # ::RubyVM::INSTRUCTION_NAMES
  # A list of bytecode instruction names in MRI.
  # This constant is MRI specific.
  INSTRUCTION_NAMES = _
  # ::RubyVM::OPTS
  # An Array of VM build options.
  # This constant is MRI specific.
  OPTS = _

  def self.each_builtin; end

  # Return current +keep_script_lines+ status. Now it only returns
  # +true+ of +false+, but it can return other objects in future.
  #
  # Note that this is an API for ruby internal use, debugging,
  # and research. Do not use this for any other purpose.
  # The compatibility is not guaranteed.
  def self.keep_script_lines; end

  # It set +keep_script_lines+ flag. If the flag is set, all
  # loaded scripts are recorded in a interpreter process.
  #
  # Note that this is an API for ruby internal use, debugging,
  # and research. Do not use this for any other purpose.
  # The compatibility is not guaranteed.
  def self.keep_script_lines=(p1) end

  def self.mtbl(p1, p2) end

  def self.mtbl2(p1, p2) end

  # Returns a Hash containing implementation-dependent counters inside the VM.
  #
  # This hash includes information about method/constant caches:
  #
  #   {
  #     :constant_cache_invalidations=>2,
  #     :constant_cache_misses=>14,
  #     :global_cvar_state=>27
  #   }
  #
  # If <tt>USE_DEBUG_COUNTER</tt> is enabled, debug counters will be included.
  #
  # The contents of the hash are implementation specific and may be changed in
  # the future.
  #
  # This method is only expected to work on C Ruby.
  def self.stat(...) end

  # AbstractSyntaxTree provides methods to parse Ruby code into
  # abstract syntax trees. The nodes in the tree
  # are instances of RubyVM::AbstractSyntaxTree::Node.
  #
  # This module is MRI specific as it exposes implementation details
  # of the MRI abstract syntax tree.
  #
  # This module is experimental and its API is not stable, therefore it might
  # change without notice. As examples, the order of children nodes is not
  # guaranteed, the number of children nodes might change, there is no way to
  # access children nodes by name, etc.
  #
  # If you are looking for a stable API or an API working under multiple Ruby
  # implementations, consider using the _parser_ gem or Ripper. If you would
  # like to make RubyVM::AbstractSyntaxTree stable, please join the discussion
  # at https://bugs.ruby-lang.org/issues/14844.
  module AbstractSyntaxTree
    #  Returns the node id for the given backtrace location.
    #
    #    begin
    #      raise
    #    rescue =>  e
    #      loc = e.backtrace_locations.first
    #      RubyVM::AbstractSyntaxTree.node_id_for_backtrace_location(loc)
    #    end # => 0
    def self.node_id_for_backtrace_location(backtrace_location) end

    #  Returns AST nodes of the given _proc_ or _method_.
    #
    #    RubyVM::AbstractSyntaxTree.of(proc {1 + 2})
    #    # => #<RubyVM::AbstractSyntaxTree::Node:SCOPE@1:35-1:42>
    #
    #    def hello
    #      puts "hello, world"
    #    end
    #
    #    RubyVM::AbstractSyntaxTree.of(method(:hello))
    #    # => #<RubyVM::AbstractSyntaxTree::Node:SCOPE@1:0-3:3>
    #
    #  See ::parse for explanation of keyword argument meaning and usage.
    def self.of(...) end

    # Parses the given _string_ into an abstract syntax tree,
    # returning the root node of that tree.
    #
    #   RubyVM::AbstractSyntaxTree.parse("x = 1 + 2")
    #   # => #<RubyVM::AbstractSyntaxTree::Node:SCOPE@1:0-1:9>
    #
    # If <tt>keep_script_lines: true</tt> option is provided, the text of the parsed
    # source is associated with nodes and is available via Node#script_lines.
    #
    # If <tt>keep_tokens: true</tt> option is provided, Node#tokens are populated.
    #
    # SyntaxError is raised if the given _string_ is invalid syntax. To overwrite this
    # behavior, <tt>error_tolerant: true</tt> can be provided. In this case, the parser
    # will produce a tree where expressions with syntax errors would be represented by
    # Node with <tt>type=:ERROR</tt>.
    #
    #    root = RubyVM::AbstractSyntaxTree.parse("x = 1; p(x; y=2")
    #    # <internal:ast>:33:in `parse': syntax error, unexpected ';', expecting ')' (SyntaxError)
    #    # x = 1; p(x; y=2
    #    #           ^
    #
    #    root = RubyVM::AbstractSyntaxTree.parse("x = 1; p(x; y=2", error_tolerant: true)
    #    # (SCOPE@1:0-1:15
    #    #  tbl: [:x, :y]
    #    #  args: nil
    #    #  body: (BLOCK@1:0-1:15 (LASGN@1:0-1:5 :x (LIT@1:4-1:5 1)) (ERROR@1:7-1:11) (LASGN@1:12-1:15 :y (LIT@1:14-1:15 2))))
    #    root.children.last.children
    #    # [(LASGN@1:0-1:5 :x (LIT@1:4-1:5 1)),
    #    #  (ERROR@1:7-1:11),
    #    #  (LASGN@1:12-1:15 :y (LIT@1:14-1:15 2))]
    #
    # Note that parsing continues even after the errored expresion.
    def self.parse(string, keep_script_lines: false, error_tolerant: false, keep_tokens: false) end

    #  Reads the file from _pathname_, then parses it like ::parse,
    #  returning the root node of the abstract syntax tree.
    #
    #  SyntaxError is raised if _pathname_'s contents are not
    #  valid Ruby syntax.
    #
    #    RubyVM::AbstractSyntaxTree.parse_file("my-app/app.rb")
    #    # => #<RubyVM::AbstractSyntaxTree::Node:SCOPE@1:0-31:3>
    #
    #  See ::parse for explanation of keyword argument meaning and usage.
    def self.parse_file(pathname, keep_script_lines: false, error_tolerant: false, keep_tokens: false) end

    # RubyVM::AbstractSyntaxTree::Node instances are created by parse methods in
    # RubyVM::AbstractSyntaxTree.
    #
    # This class is MRI specific.
    class Node
      # Returns all tokens for the input script regardless the receiver node.
      # Returns +nil+ if +keep_tokens+ is not enabled when #parse method is called.
      #
      #   root = RubyVM::AbstractSyntaxTree.parse("x = 1 + 2", keep_tokens: true)
      #   root.all_tokens # => [[0, :tIDENTIFIER, "x", [1, 0, 1, 1]], [1, :tSP, " ", [1, 1, 1, 2]], ...]
      #   root.children[-1].all_tokens # => [[0, :tIDENTIFIER, "x", [1, 0, 1, 1]], [1, :tSP, " ", [1, 1, 1, 2]], ...]
      def all_tokens; end

      # Returns AST nodes under this one.  Each kind of node
      # has different children, depending on what kind of node it is.
      #
      # The returned array may contain other nodes or <code>nil</code>.
      def children; end

      # The column number in the source code where this AST's text began.
      def first_column; end

      # The line number in the source code where this AST's text began.
      def first_lineno; end

      # Returns debugging information about this node as a string.
      def inspect; end

      # The column number in the source code where this AST's text ended.
      def last_column; end

      # The line number in the source code where this AST's text ended.
      def last_lineno; end

      # Returns an internal node_id number.
      # Note that this is an API for ruby internal use, debugging,
      # and research. Do not use this for any other purpose.
      # The compatibility is not guaranteed.
      def node_id; end

      # Returns the original source code as an array of lines.
      #
      # Note that this is an API for ruby internal use, debugging,
      # and research. Do not use this for any other purpose.
      # The compatibility is not guaranteed.
      def script_lines; end

      # Returns the code fragment that corresponds to this AST.
      #
      # Note that this is an API for ruby internal use, debugging,
      # and research. Do not use this for any other purpose.
      # The compatibility is not guaranteed.
      #
      # Also note that this API may return an incomplete code fragment
      # that does not parse; for example, a here document following
      # an expression may be dropped.
      def source; end

      # Returns tokens corresponding to the location of the node.
      # Returns +nil+ if +keep_tokens+ is not enabled when #parse method is called.
      #
      #   root = RubyVM::AbstractSyntaxTree.parse("x = 1 + 2", keep_tokens: true)
      #   root.tokens # => [[0, :tIDENTIFIER, "x", [1, 0, 1, 1]], [1, :tSP, " ", [1, 1, 1, 2]], ...]
      #   root.tokens.map{_1[2]}.join # => "x = 1 + 2"
      #
      # Token is an array of:
      #
      # - id
      # - token type
      # - source code text
      # - location [ first_lineno, first_column, last_lineno, last_column ]
      def tokens; end

      # Returns the type of this node as a symbol.
      #
      #   root = RubyVM::AbstractSyntaxTree.parse("x = 1 + 2")
      #   root.type # => :SCOPE
      #   lasgn = root.children[2]
      #   lasgn.type # => :LASGN
      #   call = lasgn.children[1]
      #   call.type # => :OPCALL
      def type; end
    end
  end

  # The InstructionSequence class represents a compiled sequence of
  # instructions for the Virtual Machine used in MRI. Not all implementations of Ruby
  # may implement this class, and for the implementations that implement it,
  # the methods defined and behavior of the methods can change in any version.
  #
  # With it, you can get a handle to the instructions that make up a method or
  # a proc, compile strings of Ruby code down to VM instructions, and
  # disassemble instruction sequences to strings for easy inspection. It is
  # mostly useful if you want to learn how YARV works, but it also lets
  # you control various settings for the Ruby iseq compiler.
  #
  # You can find the source for the VM instructions in +insns.def+ in the Ruby
  # source.
  #
  # The instruction sequence results will almost certainly change as Ruby
  # changes, so example output in this documentation may be different from what
  # you see.
  #
  # Of course, this class is MRI specific.
  class InstructionSequence
    # Takes +source+, a String of Ruby code and compiles it to an
    # InstructionSequence.
    #
    # Optionally takes +file+, +path+, and +line+ which describe the file path,
    # real path and first line number of the ruby code in +source+ which are
    # metadata attached to the returned +iseq+.
    #
    # +file+ is used for `__FILE__` and exception backtrace. +path+ is used for
    # +require_relative+ base. It is recommended these should be the same full
    # path.
    #
    # +options+, which can be +true+, +false+ or a +Hash+, is used to
    # modify the default behavior of the Ruby iseq compiler.
    #
    # For details regarding valid compile options see ::compile_option=.
    #
    #    RubyVM::InstructionSequence.compile("a = 1 + 2")
    #    #=> <RubyVM::InstructionSequence:<compiled>@<compiled>>
    #
    #    path = "test.rb"
    #    RubyVM::InstructionSequence.compile(File.read(path), path, File.expand_path(path))
    #    #=> <RubyVM::InstructionSequence:<compiled>@test.rb:1>
    #
    #    path = File.expand_path("test.rb")
    #    RubyVM::InstructionSequence.compile(File.read(path), path, path)
    #    #=> <RubyVM::InstructionSequence:<compiled>@/absolute/path/to/test.rb:1>
    def self.compile(*args) end

    # Takes +file+, a String with the location of a Ruby source file, reads,
    # parses and compiles the file, and returns +iseq+, the compiled
    # InstructionSequence with source location metadata set.
    #
    # Optionally takes +options+, which can be +true+, +false+ or a +Hash+, to
    # modify the default behavior of the Ruby iseq compiler.
    #
    # For details regarding valid compile options see ::compile_option=.
    #
    #     # /tmp/hello.rb
    #     puts "Hello, world!"
    #
    #     # elsewhere
    #     RubyVM::InstructionSequence.compile_file("/tmp/hello.rb")
    #     #=> <RubyVM::InstructionSequence:<main>@/tmp/hello.rb>
    def self.compile_file(*args) end

    # Returns a hash of default options used by the Ruby iseq compiler.
    #
    # For details, see InstructionSequence.compile_option=.
    def self.compile_option; end

    # Sets the default values for various optimizations in the Ruby iseq
    # compiler.
    #
    # Possible values for +options+ include +true+, which enables all options,
    # +false+ which disables all options, and +nil+ which leaves all options
    # unchanged.
    #
    # You can also pass a +Hash+ of +options+ that you want to change, any
    # options not present in the hash will be left unchanged.
    #
    # Possible option names (which are keys in +options+) which can be set to
    # +true+ or +false+ include:
    #
    # * +:inline_const_cache+
    # * +:instructions_unification+
    # * +:operands_unification+
    # * +:peephole_optimization+
    # * +:specialized_instruction+
    # * +:stack_caching+
    # * +:tailcall_optimization+
    #
    # Additionally, +:debug_level+ can be set to an integer.
    #
    # These default options can be overwritten for a single run of the iseq
    # compiler by passing any of the above values as the +options+ parameter to
    # ::new, ::compile and ::compile_file.
    def self.compile_option=(options) end

    # Takes +body+, a Method or Proc object, and returns a String with the
    # human readable instructions for +body+.
    #
    # For a Method object:
    #
    #   # /tmp/method.rb
    #   def hello
    #     puts "hello, world"
    #   end
    #
    #   puts RubyVM::InstructionSequence.disasm(method(:hello))
    #
    # Produces:
    #
    #   == disasm: <RubyVM::InstructionSequence:hello@/tmp/method.rb>============
    #   0000 trace            8                                               (   1)
    #   0002 trace            1                                               (   2)
    #   0004 putself
    #   0005 putstring        "hello, world"
    #   0007 send             :puts, 1, nil, 8, <ic:0>
    #   0013 trace            16                                              (   3)
    #   0015 leave                                                            (   2)
    #
    # For a Proc:
    #
    #   # /tmp/proc.rb
    #   p = proc { num = 1 + 2 }
    #   puts RubyVM::InstructionSequence.disasm(p)
    #
    # Produces:
    #
    #   == disasm: <RubyVM::InstructionSequence:block in <main>@/tmp/proc.rb>===
    #   == catch table
    #   | catch type: redo   st: 0000 ed: 0012 sp: 0000 cont: 0000
    #   | catch type: next   st: 0000 ed: 0012 sp: 0000 cont: 0012
    #   |------------------------------------------------------------------------
    #   local table (size: 2, argc: 0 [opts: 0, rest: -1, post: 0, block: -1] s1)
    #   [ 2] num
    #   0000 trace            1                                               (   1)
    #   0002 putobject        1
    #   0004 putobject        2
    #   0006 opt_plus         <ic:1>
    #   0008 dup
    #   0009 setlocal         num, 0
    #   0012 leave
    def self.disasm(body) end

    # Takes +body+, a Method or Proc object, and returns a String with the
    # human readable instructions for +body+.
    #
    # For a Method object:
    #
    #   # /tmp/method.rb
    #   def hello
    #     puts "hello, world"
    #   end
    #
    #   puts RubyVM::InstructionSequence.disasm(method(:hello))
    #
    # Produces:
    #
    #   == disasm: <RubyVM::InstructionSequence:hello@/tmp/method.rb>============
    #   0000 trace            8                                               (   1)
    #   0002 trace            1                                               (   2)
    #   0004 putself
    #   0005 putstring        "hello, world"
    #   0007 send             :puts, 1, nil, 8, <ic:0>
    #   0013 trace            16                                              (   3)
    #   0015 leave                                                            (   2)
    #
    # For a Proc:
    #
    #   # /tmp/proc.rb
    #   p = proc { num = 1 + 2 }
    #   puts RubyVM::InstructionSequence.disasm(p)
    #
    # Produces:
    #
    #   == disasm: <RubyVM::InstructionSequence:block in <main>@/tmp/proc.rb>===
    #   == catch table
    #   | catch type: redo   st: 0000 ed: 0012 sp: 0000 cont: 0000
    #   | catch type: next   st: 0000 ed: 0012 sp: 0000 cont: 0012
    #   |------------------------------------------------------------------------
    #   local table (size: 2, argc: 0 [opts: 0, rest: -1, post: 0, block: -1] s1)
    #   [ 2] num
    #   0000 trace            1                                               (   1)
    #   0002 putobject        1
    #   0004 putobject        2
    #   0006 opt_plus         <ic:1>
    #   0008 dup
    #   0009 setlocal         num, 0
    #   0012 leave
    def self.disassemble(body) end

    # Load an iseq object from binary format String object
    # created by RubyVM::InstructionSequence.to_binary.
    #
    # This loader does not have a verifier, so that loading broken/modified
    # binary causes critical problem.
    #
    # You should not load binary data provided by others.
    # You should use binary data translated by yourself.
    def self.load_from_binary(binary) end

    # Load extra data embed into binary format String object.
    def self.load_from_binary_extra_data(binary) end

    # Returns the instruction sequence containing the given proc or method.
    #
    # For example, using irb:
    #
    #     # a proc
    #     > p = proc { num = 1 + 2 }
    #     > RubyVM::InstructionSequence.of(p)
    #     > #=> <RubyVM::InstructionSequence:block in irb_binding@(irb)>
    #
    #     # for a method
    #     > def foo(bar); puts bar; end
    #     > RubyVM::InstructionSequence.of(method(:foo))
    #     > #=> <RubyVM::InstructionSequence:foo@(irb)>
    #
    # Using ::compile_file:
    #
    #     # /tmp/iseq_of.rb
    #     def hello
    #       puts "hello, world"
    #     end
    #
    #     $a_global_proc = proc { str = 'a' + 'b' }
    #
    #     # in irb
    #     > require '/tmp/iseq_of.rb'
    #
    #     # first the method hello
    #     > RubyVM::InstructionSequence.of(method(:hello))
    #     > #=> #<RubyVM::InstructionSequence:0x007fb73d7cb1d0>
    #
    #     # then the global proc
    #     > RubyVM::InstructionSequence.of($a_global_proc)
    #     > #=> #<RubyVM::InstructionSequence:0x007fb73d7caf78>
    def self.of(p1) end

    # Takes +source+, a String of Ruby code and compiles it to an
    # InstructionSequence.
    #
    # Optionally takes +file+, +path+, and +line+ which describe the file path,
    # real path and first line number of the ruby code in +source+ which are
    # metadata attached to the returned +iseq+.
    #
    # +file+ is used for `__FILE__` and exception backtrace. +path+ is used for
    # +require_relative+ base. It is recommended these should be the same full
    # path.
    #
    # +options+, which can be +true+, +false+ or a +Hash+, is used to
    # modify the default behavior of the Ruby iseq compiler.
    #
    # For details regarding valid compile options see ::compile_option=.
    #
    #    RubyVM::InstructionSequence.compile("a = 1 + 2")
    #    #=> <RubyVM::InstructionSequence:<compiled>@<compiled>>
    #
    #    path = "test.rb"
    #    RubyVM::InstructionSequence.compile(File.read(path), path, File.expand_path(path))
    #    #=> <RubyVM::InstructionSequence:<compiled>@test.rb:1>
    #
    #    path = File.expand_path("test.rb")
    #    RubyVM::InstructionSequence.compile(File.read(path), path, path)
    #    #=> <RubyVM::InstructionSequence:<compiled>@/absolute/path/to/test.rb:1>
    def initialize(*args) end

    # Returns the absolute path of this instruction sequence.
    #
    # +nil+ if the iseq was evaluated from a string.
    #
    # For example, using ::compile_file:
    #
    #     # /tmp/method.rb
    #     def hello
    #       puts "hello, world"
    #     end
    #
    #     # in irb
    #     > iseq = RubyVM::InstructionSequence.compile_file('/tmp/method.rb')
    #     > iseq.absolute_path #=> /tmp/method.rb
    def absolute_path; end

    # Returns the base label of this instruction sequence.
    #
    # For example, using irb:
    #
    #     iseq = RubyVM::InstructionSequence.compile('num = 1 + 2')
    #     #=> <RubyVM::InstructionSequence:<compiled>@<compiled>>
    #     iseq.base_label
    #     #=> "<compiled>"
    #
    # Using ::compile_file:
    #
    #     # /tmp/method.rb
    #     def hello
    #       puts "hello, world"
    #     end
    #
    #     # in irb
    #     > iseq = RubyVM::InstructionSequence.compile_file('/tmp/method.rb')
    #     > iseq.base_label #=> <main>
    def base_label; end

    # Returns the instruction sequence as a +String+ in human readable form.
    #
    #   puts RubyVM::InstructionSequence.compile('1 + 2').disasm
    #
    # Produces:
    #
    #   == disasm: <RubyVM::InstructionSequence:<compiled>@<compiled>>==========
    #   0000 trace            1                                               (   1)
    #   0002 putobject        1
    #   0004 putobject        2
    #   0006 opt_plus         <ic:1>
    #   0008 leave
    def disasm; end
    alias disassemble disasm

    # Iterate all direct child instruction sequences.
    # Iteration order is implementation/version defined
    # so that people should not rely on the order.
    def each_child; end

    # Evaluates the instruction sequence and returns the result.
    #
    #     RubyVM::InstructionSequence.compile("1 + 2").eval #=> 3
    def eval; end

    # Returns the number of the first source line where the instruction sequence
    # was loaded from.
    #
    # For example, using irb:
    #
    #     iseq = RubyVM::InstructionSequence.compile('num = 1 + 2')
    #     #=> <RubyVM::InstructionSequence:<compiled>@<compiled>>
    #     iseq.first_lineno
    #     #=> 1
    def first_lineno; end

    # Returns a human-readable string representation of this instruction
    # sequence, including the #label and #path.
    def inspect; end

    # Returns the label of this instruction sequence.
    #
    # <code><main></code> if it's at the top level, <code><compiled></code> if it
    # was evaluated from a string.
    #
    # For example, using irb:
    #
    #     iseq = RubyVM::InstructionSequence.compile('num = 1 + 2')
    #     #=> <RubyVM::InstructionSequence:<compiled>@<compiled>>
    #     iseq.label
    #     #=> "<compiled>"
    #
    # Using ::compile_file:
    #
    #     # /tmp/method.rb
    #     def hello
    #       puts "hello, world"
    #     end
    #
    #     # in irb
    #     > iseq = RubyVM::InstructionSequence.compile_file('/tmp/method.rb')
    #     > iseq.label #=> <main>
    def label; end

    # Returns the path of this instruction sequence.
    #
    # <code><compiled></code> if the iseq was evaluated from a string.
    #
    # For example, using irb:
    #
    #     iseq = RubyVM::InstructionSequence.compile('num = 1 + 2')
    #     #=> <RubyVM::InstructionSequence:<compiled>@<compiled>>
    #     iseq.path
    #     #=> "<compiled>"
    #
    # Using ::compile_file:
    #
    #     # /tmp/method.rb
    #     def hello
    #       puts "hello, world"
    #     end
    #
    #     # in irb
    #     > iseq = RubyVM::InstructionSequence.compile_file('/tmp/method.rb')
    #     > iseq.path #=> /tmp/method.rb
    def path; end

    # It returns recorded script lines if it is availalble.
    # The script lines are not limited to the iseq range, but
    # are entire lines of the source file.
    #
    # Note that this is an API for ruby internal use, debugging,
    # and research. Do not use this for any other purpose.
    # The compatibility is not guaranteed.
    def script_lines; end

    # Returns an Array with 14 elements representing the instruction sequence
    # with the following data:
    #
    # [magic]
    #   A string identifying the data format. <b>Always
    #   +YARVInstructionSequence/SimpleDataFormat+.</b>
    #
    # [major_version]
    #   The major version of the instruction sequence.
    #
    # [minor_version]
    #   The minor version of the instruction sequence.
    #
    # [format_type]
    #   A number identifying the data format. <b>Always 1</b>.
    #
    # [misc]
    #   A hash containing:
    #
    #   [+:arg_size+]
    #     the total number of arguments taken by the method or the block (0 if
    #     _iseq_ doesn't represent a method or block)
    #   [+:local_size+]
    #     the number of local variables + 1
    #   [+:stack_max+]
    #     used in calculating the stack depth at which a SystemStackError is
    #     thrown.
    #
    # [#label]
    #   The name of the context (block, method, class, module, etc.) that this
    #   instruction sequence belongs to.
    #
    #   <code><main></code> if it's at the top level, <code><compiled></code> if
    #   it was evaluated from a string.
    #
    # [#path]
    #   The relative path to the Ruby file where the instruction sequence was
    #   loaded from.
    #
    #   <code><compiled></code> if the iseq was evaluated from a string.
    #
    # [#absolute_path]
    #   The absolute path to the Ruby file where the instruction sequence was
    #   loaded from.
    #
    #   +nil+ if the iseq was evaluated from a string.
    #
    # [#first_lineno]
    #   The number of the first source line where the instruction sequence was
    #   loaded from.
    #
    # [type]
    #   The type of the instruction sequence.
    #
    #   Valid values are +:top+, +:method+, +:block+, +:class+, +:rescue+,
    #   +:ensure+, +:eval+, +:main+, and +plain+.
    #
    # [locals]
    #   An array containing the names of all arguments and local variables as
    #   symbols.
    #
    # [params]
    #   An Hash object containing parameter information.
    #
    #   More info about these values can be found in +vm_core.h+.
    #
    # [catch_table]
    #   A list of exceptions and control flow operators (rescue, next, redo,
    #   break, etc.).
    #
    # [bytecode]
    #   An array of arrays containing the instruction names and operands that
    #   make up the body of the instruction sequence.
    #
    # Note that this format is MRI specific and version dependent.
    def to_a; end

    # Returns serialized iseq binary format data as a String object.
    # A corresponding iseq object is created by
    # RubyVM::InstructionSequence.load_from_binary() method.
    #
    # String extra_data will be saved with binary data.
    # You can access this data with
    # RubyVM::InstructionSequence.load_from_binary_extra_data(binary).
    #
    # Note that the translated binary data is not portable.
    # You can not move this binary data to another machine.
    # You can not use the binary data which is created by another
    # version/another architecture of Ruby.
    def to_binary(extra_data = nil) end

    # Return trace points in the instruction sequence.
    # Return an array of [line, event_symbol] pair.
    def trace_points; end
  end

  module MJIT
    # forward declaration for ruby_vm/mjit/compiler
    C = _

    # Return true if MJIT is enabled.
    def self.enabled?; end

    # Stop generating JITed code.
    def self.pause(wait: true) end

    # Start generating JITed code again after pause.
    def self.resume; end
  end

  class Shape
    OBJ_TOO_COMPLEX_SHAPE_ID = _
    SHAPE_FLAG_SHIFT = _
    SHAPE_FROZEN = _
    SHAPE_ID_NUM_BITS = _
    SHAPE_IVAR = _
    SHAPE_MAX_VARIATIONS = _
    SHAPE_ROOT = _
    SHAPE_T_OBJECT = _
    SPECIAL_CONST_SHAPE_ID = _
  end

  # This module allows for introspection of YJIT, CRuby's in-process
  # just-in-time compiler. This module exists only to help develop YJIT, as such,
  # everything in the module is highly implementation specific and comes with no
  # API stability guarantee whatsoever.
  #
  # This module may not exist if YJIT does not support the particular platform
  # for which CRuby is built. There is also no API stability guarantee as to in
  # what situations this module is defined.
  module YJIT
    # Free and recompile all existing JIT code
    def self.code_gc; end

    # Produce disassembly for an iseq
    def self.disasm(iseq) end

    # Marshal dumps exit locations to the given filename.
    #
    # Usage:
    #
    # If `--yjit-exit-locations` is passed, a file named
    # "yjit_exit_locations.dump" will automatically be generated.
    #
    # If you want to collect traces manually, call `dump_exit_locations`
    # directly.
    #
    # Note that calling this in a script will generate stats after the
    # dump is created, so the stats data may include exits from the
    # dump itself.
    #
    # In a script call:
    #
    #   at_exit do
    #     RubyVM::YJIT.dump_exit_locations("my_file.dump")
    #   end
    #
    # Then run the file with the following options:
    #
    #   ruby --yjit --yjit-trace-exits test.rb
    #
    # Once the code is done running, use Stackprof to read the dump file.
    # See Stackprof documentation for options.
    def self.dump_exit_locations(filename) end

    # Check if YJIT is enabled
    def self.enabled?; end

    # If --yjit-trace-exits is enabled parse the hashes from
    # Primitive.rb_yjit_get_exit_locations into a format readable
    # by Stackprof. This will allow us to find the exact location of a
    # side exit in YJIT based on the instruction that is exiting.
    def self.exit_locations; end

    # Produce a list of instructions compiled by YJIT for an iseq
    def self.insns_compiled(iseq) end

    # Discard statistics collected for --yjit-stats.
    def self.reset_stats!; end

    # Return a hash for statistics generated for the --yjit-stats command line option.
    # Return nil when option is not passed or unavailable.
    def self.runtime_stats; end

    # Check if --yjit-stats is used.
    def self.stats_enabled?; end

    # Check if rb_yjit_trace_exit_locations_enabled_p is enabled.
    def self.trace_exit_locations_enabled?; end
  end
end
