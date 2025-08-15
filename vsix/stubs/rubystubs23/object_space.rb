# frozen_string_literal: true

# The objspace library extends the ObjectSpace module and adds several
# methods to get internal statistic information about
# object/memory management.
#
# You need to <code>require 'objspace'</code> to use this extension module.
#
# Generally, you *SHOULD NOT* use this library if you do not know
# about the MRI implementation.  Mainly, this library is for (memory)
# profiler developers and MRI developers who need to know about MRI
# memory usage.
# ---
# The ObjectSpace module contains a number of routines
# that interact with the garbage collection facility and allow you to
# traverse all living objects with an iterator.
#
# ObjectSpace also provides support for object finalizers, procs that will be
# called when a specific object is about to be destroyed by garbage
# collection.
#
#    require 'objspace'
#
#    a = "A"
#    b = "B"
#
#    ObjectSpace.define_finalizer(a, proc {|id| puts "Finalizer one on #{id}" })
#    ObjectSpace.define_finalizer(b, proc {|id| puts "Finalizer two on #{id}" })
#
# _produces:_
#
#    Finalizer two on 537763470
#    Finalizer one on 537763480
module ObjectSpace
  # Converts an object id to a reference to the object. May not be
  # called on an object id passed as a parameter to a finalizer.
  #
  #    s = "I am a string"                    #=> "I am a string"
  #    r = ObjectSpace._id2ref(s.object_id)   #=> "I am a string"
  #    r == s                                 #=> true
  def self._id2ref(object_id) end

  # Returns the class for the given +object+.
  #
  #      class A
  #        def foo
  #          ObjectSpace::trace_object_allocations do
  #            obj = Object.new
  #            p "#{ObjectSpace::allocation_class_path(obj)}"
  #          end
  #        end
  #      end
  #
  #      A.new.foo #=> "Class"
  #
  # See ::trace_object_allocations for more information and examples.
  def self.allocation_class_path(object) end

  # Returns garbage collector generation for the given +object+.
  #
  #      class B
  #        include ObjectSpace
  #
  #        def foo
  #          trace_object_allocations do
  #            obj = Object.new
  #            p "Generation is #{allocation_generation(obj)}"
  #          end
  #        end
  #      end
  #
  #      B.new.foo #=> "Generation is 3"
  #
  # See ::trace_object_allocations for more information and examples.
  def self.allocation_generation(object) end

  # Returns the method identifier for the given +object+.
  #
  #      class A
  #        include ObjectSpace
  #
  #        def foo
  #          trace_object_allocations do
  #            obj = Object.new
  #            p "#{allocation_class_path(obj)}##{allocation_method_id(obj)}"
  #          end
  #        end
  #      end
  #
  #      A.new.foo #=> "Class#new"
  #
  # See ::trace_object_allocations for more information and examples.
  def self.allocation_method_id(object) end

  # Returns the source file origin from the given +object+.
  #
  # See ::trace_object_allocations for more information and examples.
  def self.allocation_sourcefile(object) end

  # Returns the original line from source for from the given +object+.
  #
  # See ::trace_object_allocations for more information and examples.
  def self.allocation_sourceline(object) end

  # Counts objects for each +T_IMEMO+ type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # It returns a hash as:
  #
  #      {:imemo_ifunc=>8,
  #       :imemo_svar=>7,
  #       :imemo_cref=>509,
  #       :imemo_memo=>1,
  #       :imemo_throw_data=>1}
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # The contents of the returned hash is implementation specific and may change
  # in the future.
  #
  # In this version, keys are symbol objects.
  #
  # This method is only expected to work with C Ruby.
  def self.count_imemo_objects(*result_hash) end

  # Counts nodes for each node type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # It returns a hash as:
  #
  #     {:NODE_METHOD=>2027, :NODE_FBODY=>1927, :NODE_CFUNC=>1798, ...}
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # Note:
  # The contents of the returned hash is implementation defined.
  # It may be changed in future.
  #
  # This method is only expected to work with C Ruby.
  def self.count_nodes(*result_hash) end

  # Counts all objects grouped by type.
  #
  # It returns a hash, such as:
  #     {
  #       :TOTAL=>10000,
  #       :FREE=>3011,
  #       :T_OBJECT=>6,
  #       :T_CLASS=>404,
  #       # ...
  #     }
  #
  # The contents of the returned hash are implementation specific.
  # It may be changed in future.
  #
  # The keys starting with +:T_+ means live objects.
  # For example, +:T_ARRAY+ is the number of arrays.
  # +:FREE+ means object slots which is not used now.
  # +:TOTAL+ means sum of above.
  #
  # If the optional argument +result_hash+ is given,
  # it is overwritten and returned. This is intended to avoid probe effect.
  #
  #   h = {}
  #   ObjectSpace.count_objects(h)
  #   puts h
  #   # => { :TOTAL=>10000, :T_CLASS=>158280, :T_MODULE=>20672, :T_STRING=>527249 }
  #
  # This method is only expected to work on C Ruby.
  def self.count_objects(*result_hash) end

  # Counts objects size (in bytes) for each type.
  #
  # Note that this information is incomplete.  You need to deal with
  # this information as only a *HINT*.  Especially, total size of
  # T_DATA may not right size.
  #
  # It returns a hash as:
  #   {:TOTAL=>1461154, :T_CLASS=>158280, :T_MODULE=>20672, :T_STRING=>527249, ...}
  #
  # If the optional argument, result_hash, is given,
  # it is overwritten and returned.
  # This is intended to avoid probe effect.
  #
  # The contents of the returned hash is implementation defined.
  # It may be changed in future.
  #
  # This method is only expected to work with C Ruby.
  def self.count_objects_size(*result_hash) end

  # Counts symbols for each Symbol type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # Note:
  # The contents of the returned hash is implementation defined.
  # It may be changed in future.
  #
  # This method is only expected to work with C Ruby.
  #
  # On this version of MRI, they have 3 types of Symbols (and 1 total counts).
  #
  #  * mortal_dynamic_symbol: GC target symbols (collected by GC)
  #  * immortal_dynamic_symbol: Immortal symbols promoted from dynamic symbols (do not collected by GC)
  #  * immortal_static_symbol: Immortal symbols (do not collected by GC)
  #  * immortal_symbol: total immortal symbols (immortal_dynamic_symbol+immortal_static_symbol)
  def self.count_symbols(*result_hash) end

  # Counts objects for each +T_DATA+ type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # It returns a hash as:
  #
  #     {RubyVM::InstructionSequence=>504, :parser=>5, :barrier=>6,
  #      :mutex=>6, Proc=>60, RubyVM::Env=>57, Mutex=>1, Encoding=>99,
  #      ThreadGroup=>1, Binding=>1, Thread=>1, RubyVM=>1, :iseq=>1,
  #      Random=>1, ARGF.class=>1, Data=>1, :autoload=>3, Time=>2}
  #     # T_DATA objects existing at startup on r32276.
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # The contents of the returned hash is implementation specific and may change
  # in the future.
  #
  # In this version, keys are Class object or Symbol object.
  #
  # If object is kind of normal (accessible) object, the key is Class object.
  # If object is not a kind of normal (internal) object, the key is symbol
  # name, registered by rb_data_type_struct.
  #
  # This method is only expected to work with C Ruby.
  def self.count_tdata_objects(*result_hash) end

  # Adds <i>aProc</i> as a finalizer, to be called after <i>obj</i>
  # was destroyed. The object ID of the <i>obj</i> will be passed
  # as an argument to <i>aProc</i>. If <i>aProc</i> is a lambda or
  # method, make sure it can be called with a single argument.
  def self.define_finalizer(obj, proc = _) end

  # Dump the contents of a ruby object as JSON.
  #
  # This method is only expected to work with C Ruby.
  # This is an experimental method and is subject to change.
  # In particular, the function signature and output format are
  # not guaranteed to be compatible in future versions of ruby.
  def self.dump(p1, p2 = {}) end

  # Dump the contents of the ruby heap as JSON.
  #
  # This method is only expected to work with C Ruby.
  # This is an experimental method and is subject to change.
  # In particular, the function signature and output format are
  # not guaranteed to be compatible in future versions of ruby.
  def self.dump_all(p1 = {}) end

  # Calls the block once for each living, nonimmediate object in this
  # Ruby process. If <i>module</i> is specified, calls the block
  # for only those classes or modules that match (or are a subclass of)
  # <i>module</i>. Returns the number of objects found. Immediate
  # objects (<code>Fixnum</code>s, <code>Symbol</code>s
  # <code>true</code>, <code>false</code>, and <code>nil</code>) are
  # never returned. In the example below, <code>each_object</code>
  # returns both the numbers we defined and several constants defined in
  # the <code>Math</code> module.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    a = 102.7
  #    b = 95       # Won't be returned
  #    c = 12345678987654321
  #    count = ObjectSpace.each_object(Numeric) {|x| p x }
  #    puts "Total count: #{count}"
  #
  # <em>produces:</em>
  #
  #    12345678987654321
  #    102.7
  #    2.71828182845905
  #    3.14159265358979
  #    2.22044604925031e-16
  #    1.7976931348623157e+308
  #    2.2250738585072e-308
  #    Total count: 7
  def self.each_object(*module1) end

  # Initiates garbage collection, unless manually disabled.
  #
  # This method is defined with keyword arguments that default to true:
  #
  #    def GC.start(full_mark: true, immediate_sweep: true); end
  #
  # Use full_mark: false to perform a minor GC.
  # Use immediate_sweep: false to defer sweeping (use lazy sweep).
  #
  # Note: These keyword arguments are implementation and version dependent. They
  # are not guaranteed to be future-compatible, and may be ignored if the
  # underlying implementation does not support them.
  def self.garbage_collect(*several_variants) end

  # [MRI specific feature] Return internal class of obj.
  # obj can be an instance of InternalObjectWrapper.
  #
  # Note that you should not use this method in your application.
  def self.internal_class_of(obj) end

  # [MRI specific feature] Return internal super class of cls (Class or Module).
  # obj can be an instance of InternalObjectWrapper.
  #
  # Note that you should not use this method in your application.
  def self.internal_super_of(cls) end

  # Return consuming memory size of obj.
  #
  # Note that the return size is incomplete.  You need to deal with this
  # information as only a *HINT*. Especially, the size of +T_DATA+ may not be
  # correct.
  #
  # This method is only expected to work with C Ruby.
  #
  # From Ruby 2.2, memsize_of(obj) returns a memory size includes
  # sizeof(RVALUE).
  def self.memsize_of(obj) end

  # Return consuming memory size of all living objects.
  #
  # If +klass+ (should be Class object) is given, return the total memory size
  # of instances of the given class.
  #
  # Note that the returned size is incomplete. You need to deal with this
  # information as only a *HINT*. Especially, the size of +T_DATA+ may not be
  # correct.
  #
  # Note that this method does *NOT* return total malloc'ed memory size.
  #
  # This method can be defined by the following Ruby code:
  #
  #     def memsize_of_all klass = false
  #       total = 0
  #       ObjectSpace.each_object{|e|
  #         total += ObjectSpace.memsize_of(e) if klass == false || e.kind_of?(klass)
  #       }
  #       total
  #     end
  #
  # This method is only expected to work with C Ruby.
  def self.memsize_of_all(*klass) end

  # [MRI specific feature] Return all reachable objects from `obj'.
  #
  # This method returns all reachable objects from `obj'.
  #
  # If `obj' has two or more references to the same object `x', then returned
  # array only includes one `x' object.
  #
  # If `obj' is a non-markable (non-heap management) object such as true,
  # false, nil, symbols and Fixnums (and Flonum) then it simply returns nil.
  #
  # If `obj' has references to an internal object, then it returns instances of
  # ObjectSpace::InternalObjectWrapper class. This object contains a reference
  # to an internal object and you can check the type of internal object with
  # `type' method.
  #
  # If `obj' is instance of ObjectSpace::InternalObjectWrapper class, then this
  # method returns all reachable object from an internal object, which is
  # pointed by `obj'.
  #
  # With this method, you can find memory leaks.
  #
  # This method is only expected to work except with C Ruby.
  #
  # Example:
  #   ObjectSpace.reachable_objects_from(['a', 'b', 'c'])
  #   #=> [Array, 'a', 'b', 'c']
  #
  #   ObjectSpace.reachable_objects_from(['a', 'a', 'a'])
  #   #=> [Array, 'a', 'a', 'a'] # all 'a' strings have different object id
  #
  #   ObjectSpace.reachable_objects_from([v = 'a', v, v])
  #   #=> [Array, 'a']
  #
  #   ObjectSpace.reachable_objects_from(1)
  #   #=> nil # 1 is not markable (heap managed) object
  def self.reachable_objects_from(obj) end

  # [MRI specific feature] Return all reachable objects from root.
  def self.reachable_objects_from_root; end

  # Starts tracing object allocations from the ObjectSpace extension module.
  #
  # For example:
  #
  #      require 'objspace'
  #
  #      class C
  #        include ObjectSpace
  #
  #        def foo
  #          trace_object_allocations do
  #            obj = Object.new
  #            p "#{allocation_sourcefile(obj)}:#{allocation_sourceline(obj)}"
  #          end
  #        end
  #      end
  #
  #      C.new.foo #=> "objtrace.rb:8"
  #
  # This example has included the ObjectSpace module to make it easier to read,
  # but you can also use the ::trace_object_allocations notation (recommended).
  #
  # Note that this feature introduces a huge performance decrease and huge
  # memory consumption.
  def self.trace_object_allocations; end

  # Clear recorded tracing information.
  def self.trace_object_allocations_clear; end

  def self.trace_object_allocations_debug_start; end

  # Starts tracing object allocations.
  def self.trace_object_allocations_start; end

  # Stop tracing object allocations.
  #
  # Note that if ::trace_object_allocations_start is called n-times, then
  # tracing will stop after calling ::trace_object_allocations_stop n-times.
  def self.trace_object_allocations_stop; end

  # Removes all finalizers for <i>obj</i>.
  def self.undefine_finalizer(obj) end

  private

  # Converts an object id to a reference to the object. May not be
  # called on an object id passed as a parameter to a finalizer.
  #
  #    s = "I am a string"                    #=> "I am a string"
  #    r = ObjectSpace._id2ref(s.object_id)   #=> "I am a string"
  #    r == s                                 #=> true
  def _id2ref(object_id) end

  # Returns the class for the given +object+.
  #
  #      class A
  #        def foo
  #          ObjectSpace::trace_object_allocations do
  #            obj = Object.new
  #            p "#{ObjectSpace::allocation_class_path(obj)}"
  #          end
  #        end
  #      end
  #
  #      A.new.foo #=> "Class"
  #
  # See ::trace_object_allocations for more information and examples.
  def allocation_class_path(object) end

  # Returns garbage collector generation for the given +object+.
  #
  #      class B
  #        include ObjectSpace
  #
  #        def foo
  #          trace_object_allocations do
  #            obj = Object.new
  #            p "Generation is #{allocation_generation(obj)}"
  #          end
  #        end
  #      end
  #
  #      B.new.foo #=> "Generation is 3"
  #
  # See ::trace_object_allocations for more information and examples.
  def allocation_generation(object) end

  # Returns the method identifier for the given +object+.
  #
  #      class A
  #        include ObjectSpace
  #
  #        def foo
  #          trace_object_allocations do
  #            obj = Object.new
  #            p "#{allocation_class_path(obj)}##{allocation_method_id(obj)}"
  #          end
  #        end
  #      end
  #
  #      A.new.foo #=> "Class#new"
  #
  # See ::trace_object_allocations for more information and examples.
  def allocation_method_id(object) end

  # Returns the source file origin from the given +object+.
  #
  # See ::trace_object_allocations for more information and examples.
  def allocation_sourcefile(object) end

  # Returns the original line from source for from the given +object+.
  #
  # See ::trace_object_allocations for more information and examples.
  def allocation_sourceline(object) end

  # Counts objects for each +T_IMEMO+ type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # It returns a hash as:
  #
  #      {:imemo_ifunc=>8,
  #       :imemo_svar=>7,
  #       :imemo_cref=>509,
  #       :imemo_memo=>1,
  #       :imemo_throw_data=>1}
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # The contents of the returned hash is implementation specific and may change
  # in the future.
  #
  # In this version, keys are symbol objects.
  #
  # This method is only expected to work with C Ruby.
  def count_imemo_objects(*result_hash) end

  # Counts nodes for each node type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # It returns a hash as:
  #
  #     {:NODE_METHOD=>2027, :NODE_FBODY=>1927, :NODE_CFUNC=>1798, ...}
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # Note:
  # The contents of the returned hash is implementation defined.
  # It may be changed in future.
  #
  # This method is only expected to work with C Ruby.
  def count_nodes(*result_hash) end

  # Counts all objects grouped by type.
  #
  # It returns a hash, such as:
  #     {
  #       :TOTAL=>10000,
  #       :FREE=>3011,
  #       :T_OBJECT=>6,
  #       :T_CLASS=>404,
  #       # ...
  #     }
  #
  # The contents of the returned hash are implementation specific.
  # It may be changed in future.
  #
  # The keys starting with +:T_+ means live objects.
  # For example, +:T_ARRAY+ is the number of arrays.
  # +:FREE+ means object slots which is not used now.
  # +:TOTAL+ means sum of above.
  #
  # If the optional argument +result_hash+ is given,
  # it is overwritten and returned. This is intended to avoid probe effect.
  #
  #   h = {}
  #   ObjectSpace.count_objects(h)
  #   puts h
  #   # => { :TOTAL=>10000, :T_CLASS=>158280, :T_MODULE=>20672, :T_STRING=>527249 }
  #
  # This method is only expected to work on C Ruby.
  def count_objects(*result_hash) end

  # Counts objects size (in bytes) for each type.
  #
  # Note that this information is incomplete.  You need to deal with
  # this information as only a *HINT*.  Especially, total size of
  # T_DATA may not right size.
  #
  # It returns a hash as:
  #   {:TOTAL=>1461154, :T_CLASS=>158280, :T_MODULE=>20672, :T_STRING=>527249, ...}
  #
  # If the optional argument, result_hash, is given,
  # it is overwritten and returned.
  # This is intended to avoid probe effect.
  #
  # The contents of the returned hash is implementation defined.
  # It may be changed in future.
  #
  # This method is only expected to work with C Ruby.
  def count_objects_size(*result_hash) end

  # Counts symbols for each Symbol type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # Note:
  # The contents of the returned hash is implementation defined.
  # It may be changed in future.
  #
  # This method is only expected to work with C Ruby.
  #
  # On this version of MRI, they have 3 types of Symbols (and 1 total counts).
  #
  #  * mortal_dynamic_symbol: GC target symbols (collected by GC)
  #  * immortal_dynamic_symbol: Immortal symbols promoted from dynamic symbols (do not collected by GC)
  #  * immortal_static_symbol: Immortal symbols (do not collected by GC)
  #  * immortal_symbol: total immortal symbols (immortal_dynamic_symbol+immortal_static_symbol)
  def count_symbols(*result_hash) end

  # Counts objects for each +T_DATA+ type.
  #
  # This method is only for MRI developers interested in performance and memory
  # usage of Ruby programs.
  #
  # It returns a hash as:
  #
  #     {RubyVM::InstructionSequence=>504, :parser=>5, :barrier=>6,
  #      :mutex=>6, Proc=>60, RubyVM::Env=>57, Mutex=>1, Encoding=>99,
  #      ThreadGroup=>1, Binding=>1, Thread=>1, RubyVM=>1, :iseq=>1,
  #      Random=>1, ARGF.class=>1, Data=>1, :autoload=>3, Time=>2}
  #     # T_DATA objects existing at startup on r32276.
  #
  # If the optional argument, result_hash, is given, it is overwritten and
  # returned. This is intended to avoid probe effect.
  #
  # The contents of the returned hash is implementation specific and may change
  # in the future.
  #
  # In this version, keys are Class object or Symbol object.
  #
  # If object is kind of normal (accessible) object, the key is Class object.
  # If object is not a kind of normal (internal) object, the key is symbol
  # name, registered by rb_data_type_struct.
  #
  # This method is only expected to work with C Ruby.
  def count_tdata_objects(*result_hash) end

  # Adds <i>aProc</i> as a finalizer, to be called after <i>obj</i>
  # was destroyed. The object ID of the <i>obj</i> will be passed
  # as an argument to <i>aProc</i>. If <i>aProc</i> is a lambda or
  # method, make sure it can be called with a single argument.
  def define_finalizer(p1, p2 = v2) end

  # Dump the contents of a ruby object as JSON.
  #
  # This method is only expected to work with C Ruby.
  # This is an experimental method and is subject to change.
  # In particular, the function signature and output format are
  # not guaranteed to be compatible in future versions of ruby.
  def dump(p1, p2 = {}) end

  # Dump the contents of the ruby heap as JSON.
  #
  # This method is only expected to work with C Ruby.
  # This is an experimental method and is subject to change.
  # In particular, the function signature and output format are
  # not guaranteed to be compatible in future versions of ruby.
  def dump_all(p1 = {}) end

  # Calls the block once for each living, nonimmediate object in this
  # Ruby process. If <i>module</i> is specified, calls the block
  # for only those classes or modules that match (or are a subclass of)
  # <i>module</i>. Returns the number of objects found. Immediate
  # objects (<code>Fixnum</code>s, <code>Symbol</code>s
  # <code>true</code>, <code>false</code>, and <code>nil</code>) are
  # never returned. In the example below, <code>each_object</code>
  # returns both the numbers we defined and several constants defined in
  # the <code>Math</code> module.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    a = 102.7
  #    b = 95       # Won't be returned
  #    c = 12345678987654321
  #    count = ObjectSpace.each_object(Numeric) {|x| p x }
  #    puts "Total count: #{count}"
  #
  # <em>produces:</em>
  #
  #    12345678987654321
  #    102.7
  #    2.71828182845905
  #    3.14159265358979
  #    2.22044604925031e-16
  #    1.7976931348623157e+308
  #    2.2250738585072e-308
  #    Total count: 7
  def each_object(p1 = v1) end

  # Initiates garbage collection, unless manually disabled.
  #
  # This method is defined with keyword arguments that default to true:
  #
  #    def GC.start(full_mark: true, immediate_sweep: true); end
  #
  # Use full_mark: false to perform a minor GC.
  # Use immediate_sweep: false to defer sweeping (use lazy sweep).
  #
  # Note: These keyword arguments are implementation and version dependent. They
  # are not guaranteed to be future-compatible, and may be ignored if the
  # underlying implementation does not support them.
  def garbage_collect(*several_variants) end

  # [MRI specific feature] Return internal class of obj.
  # obj can be an instance of InternalObjectWrapper.
  #
  # Note that you should not use this method in your application.
  def internal_class_of(obj) end

  # [MRI specific feature] Return internal super class of cls (Class or Module).
  # obj can be an instance of InternalObjectWrapper.
  #
  # Note that you should not use this method in your application.
  def internal_super_of(cls) end

  # Return consuming memory size of obj.
  #
  # Note that the return size is incomplete.  You need to deal with this
  # information as only a *HINT*. Especially, the size of +T_DATA+ may not be
  # correct.
  #
  # This method is only expected to work with C Ruby.
  #
  # From Ruby 2.2, memsize_of(obj) returns a memory size includes
  # sizeof(RVALUE).
  def memsize_of(obj) end

  # Return consuming memory size of all living objects.
  #
  # If +klass+ (should be Class object) is given, return the total memory size
  # of instances of the given class.
  #
  # Note that the returned size is incomplete. You need to deal with this
  # information as only a *HINT*. Especially, the size of +T_DATA+ may not be
  # correct.
  #
  # Note that this method does *NOT* return total malloc'ed memory size.
  #
  # This method can be defined by the following Ruby code:
  #
  #     def memsize_of_all klass = false
  #       total = 0
  #       ObjectSpace.each_object{|e|
  #         total += ObjectSpace.memsize_of(e) if klass == false || e.kind_of?(klass)
  #       }
  #       total
  #     end
  #
  # This method is only expected to work with C Ruby.
  def memsize_of_all(*klass) end

  # [MRI specific feature] Return all reachable objects from `obj'.
  #
  # This method returns all reachable objects from `obj'.
  #
  # If `obj' has two or more references to the same object `x', then returned
  # array only includes one `x' object.
  #
  # If `obj' is a non-markable (non-heap management) object such as true,
  # false, nil, symbols and Fixnums (and Flonum) then it simply returns nil.
  #
  # If `obj' has references to an internal object, then it returns instances of
  # ObjectSpace::InternalObjectWrapper class. This object contains a reference
  # to an internal object and you can check the type of internal object with
  # `type' method.
  #
  # If `obj' is instance of ObjectSpace::InternalObjectWrapper class, then this
  # method returns all reachable object from an internal object, which is
  # pointed by `obj'.
  #
  # With this method, you can find memory leaks.
  #
  # This method is only expected to work except with C Ruby.
  #
  # Example:
  #   ObjectSpace.reachable_objects_from(['a', 'b', 'c'])
  #   #=> [Array, 'a', 'b', 'c']
  #
  #   ObjectSpace.reachable_objects_from(['a', 'a', 'a'])
  #   #=> [Array, 'a', 'a', 'a'] # all 'a' strings have different object id
  #
  #   ObjectSpace.reachable_objects_from([v = 'a', v, v])
  #   #=> [Array, 'a']
  #
  #   ObjectSpace.reachable_objects_from(1)
  #   #=> nil # 1 is not markable (heap managed) object
  def reachable_objects_from(obj) end

  # [MRI specific feature] Return all reachable objects from root.
  def reachable_objects_from_root; end

  # Starts tracing object allocations from the ObjectSpace extension module.
  #
  # For example:
  #
  #      require 'objspace'
  #
  #      class C
  #        include ObjectSpace
  #
  #        def foo
  #          trace_object_allocations do
  #            obj = Object.new
  #            p "#{allocation_sourcefile(obj)}:#{allocation_sourceline(obj)}"
  #          end
  #        end
  #      end
  #
  #      C.new.foo #=> "objtrace.rb:8"
  #
  # This example has included the ObjectSpace module to make it easier to read,
  # but you can also use the ::trace_object_allocations notation (recommended).
  #
  # Note that this feature introduces a huge performance decrease and huge
  # memory consumption.
  def trace_object_allocations; end

  # Clear recorded tracing information.
  def trace_object_allocations_clear; end

  def trace_object_allocations_debug_start; end

  # Starts tracing object allocations.
  def trace_object_allocations_start; end

  # Stop tracing object allocations.
  #
  # Note that if ::trace_object_allocations_start is called n-times, then
  # tracing will stop after calling ::trace_object_allocations_stop n-times.
  def trace_object_allocations_stop; end

  # Removes all finalizers for <i>obj</i>.
  def undefine_finalizer(obj) end

  # This class is used as a return value from
  # ObjectSpace::reachable_objects_from.
  #
  # When ObjectSpace::reachable_objects_from returns an object with
  # references to an internal object, an instance of this class is returned.
  #
  # You can use the #type method to check the type of the internal object.
  class InternalObjectWrapper
    # See Object#inspect.
    def inspect; end

    # Returns the Object#object_id of the internal object.
    def internal_object_id; end

    # Returns the type of the internal object.
    def type; end
  end

  # An ObjectSpace::WeakMap object holds references to
  # any objects, but those objects can get garbage collected.
  #
  # This class is mostly used internally by WeakRef, please use
  # +lib/weakref.rb+ for the public interface.
  class WeakMap
    include Enumerable

    # Retrieves a weakly referenced object with the given key
    def [](p1) end

    # Creates a weak reference from the given key to the given value
    def []=(p1, p2) end

    # Iterates over keys and objects in a weakly referenced object
    def each; end
    alias each_pair each

    # Iterates over keys and objects in a weakly referenced object
    def each_key; end

    # Iterates over keys and objects in a weakly referenced object
    def each_value; end

    # Returns +true+ if +key+ is registered
    def include?(p1) end
    alias member? include?
    alias key? include?

    def inspect; end

    # Iterates over keys and objects in a weakly referenced object
    def keys; end

    def size; end
    alias length size

    # Iterates over values and objects in a weakly referenced object
    def values; end

    private

    def finalize(p1) end
  end
end
