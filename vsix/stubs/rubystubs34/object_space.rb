# frozen_string_literal: true

# The objspace library extends the ObjectSpace module and adds several
# methods to get internal statistic information about
# object/memory management.
#
# You need to <code>require 'objspace'</code> to use this extension module.
#
# Generally, you *SHOULD* *NOT* use this library if you do not know
# about the MRI implementation.  Mainly, this library is for (memory)
# profiler developers and MRI developers who need to know about MRI
# memory usage.
# ---
# The ObjectSpace module contains a number of routines
# that interact with the garbage collection facility and allow you to
# traverse all living objects with an iterator.
#
# ObjectSpace also provides support for object finalizers, procs that will be
# called after a specific object was destroyed by garbage collection.  See
# the documentation for +ObjectSpace.define_finalizer+ for important
# information on how to use this method correctly.
#
#    a = "A"
#    b = "B"
#
#    ObjectSpace.define_finalizer(a, proc {|id| puts "Finalizer one on #{id}" })
#    ObjectSpace.define_finalizer(b, proc {|id| puts "Finalizer two on #{id}" })
#
#    a = nil
#    b = nil
#
# _produces:_
#
#    Finalizer two on 537763470
#    Finalizer one on 537763480
module ObjectSpace
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
  # T_DATA may be wrong.
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
  #
  # The return value is an array <code>[0, aProc]</code>.
  #
  # The two recommended patterns are to either create the finaliser proc
  # in a non-instance method where it can safely capture the needed state,
  # or to use a custom callable object that stores the needed state
  # explicitly as instance variables.
  #
  #     class Foo
  #       def initialize(data_needed_for_finalization)
  #         ObjectSpace.define_finalizer(self, self.class.create_finalizer(data_needed_for_finalization))
  #       end
  #
  #       def self.create_finalizer(data_needed_for_finalization)
  #         proc {
  #           puts "finalizing #{data_needed_for_finalization}"
  #         }
  #       end
  #     end
  #
  #     class Bar
  #      class Remover
  #         def initialize(data_needed_for_finalization)
  #           @data_needed_for_finalization = data_needed_for_finalization
  #         end
  #
  #         def call(id)
  #           puts "finalizing #{@data_needed_for_finalization}"
  #         end
  #       end
  #
  #       def initialize(data_needed_for_finalization)
  #         ObjectSpace.define_finalizer(self, Remover.new(data_needed_for_finalization))
  #       end
  #     end
  #
  # Note that if your finalizer references the object to be
  # finalized it will never be run on GC, although it will still be
  # run at exit. You will get a warning if you capture the object
  # to be finalized as the receiver of the finalizer.
  #
  #     class CapturesSelf
  #       def initialize(name)
  #         ObjectSpace.define_finalizer(self, proc {
  #           # this finalizer will only be run on exit
  #           puts "finalizing #{name}"
  #         })
  #       end
  #     end
  #
  # Also note that finalization can be unpredictable and is never guaranteed
  # to be run except on exit.
  def self.define_finalizer(obj, proc = _) end

  # Calls the block once for each living, nonimmediate object in this
  # Ruby process. If <i>module</i> is specified, calls the block
  # for only those classes or modules that match (or are a subclass of)
  # <i>module</i>. Returns the number of objects found. Immediate
  # objects (<code>Fixnum</code>s, <code>Symbol</code>s
  # <code>true</code>, <code>false</code>, and <code>nil</code>) are
  # never returned. In the example below, #each_object returns both
  # the numbers we defined and several constants defined in the Math
  # module.
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
  def self.each_object(*args) end

  # Alias of GC.start
  def self.garbage_collect(full_mark: true, immediate_mark: true, immediate_sweep: true) end

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

  # Return consuming memory size of obj in bytes.
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

  # Return consuming memory size of all living objects in bytes.
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
  # This method is only expected to work with C Ruby.
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
  # T_DATA may be wrong.
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
  #
  # The return value is an array <code>[0, aProc]</code>.
  #
  # The two recommended patterns are to either create the finaliser proc
  # in a non-instance method where it can safely capture the needed state,
  # or to use a custom callable object that stores the needed state
  # explicitly as instance variables.
  #
  #     class Foo
  #       def initialize(data_needed_for_finalization)
  #         ObjectSpace.define_finalizer(self, self.class.create_finalizer(data_needed_for_finalization))
  #       end
  #
  #       def self.create_finalizer(data_needed_for_finalization)
  #         proc {
  #           puts "finalizing #{data_needed_for_finalization}"
  #         }
  #       end
  #     end
  #
  #     class Bar
  #      class Remover
  #         def initialize(data_needed_for_finalization)
  #           @data_needed_for_finalization = data_needed_for_finalization
  #         end
  #
  #         def call(id)
  #           puts "finalizing #{@data_needed_for_finalization}"
  #         end
  #       end
  #
  #       def initialize(data_needed_for_finalization)
  #         ObjectSpace.define_finalizer(self, Remover.new(data_needed_for_finalization))
  #       end
  #     end
  #
  # Note that if your finalizer references the object to be
  # finalized it will never be run on GC, although it will still be
  # run at exit. You will get a warning if you capture the object
  # to be finalized as the receiver of the finalizer.
  #
  #     class CapturesSelf
  #       def initialize(name)
  #         ObjectSpace.define_finalizer(self, proc {
  #           # this finalizer will only be run on exit
  #           puts "finalizing #{name}"
  #         })
  #       end
  #     end
  #
  # Also note that finalization can be unpredictable and is never guaranteed
  # to be run except on exit.
  def define_finalizer(p1, p2 = v2) end

  # Calls the block once for each living, nonimmediate object in this
  # Ruby process. If <i>module</i> is specified, calls the block
  # for only those classes or modules that match (or are a subclass of)
  # <i>module</i>. Returns the number of objects found. Immediate
  # objects (<code>Fixnum</code>s, <code>Symbol</code>s
  # <code>true</code>, <code>false</code>, and <code>nil</code>) are
  # never returned. In the example below, #each_object returns both
  # the numbers we defined and several constants defined in the Math
  # module.
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
  def each_object(*args) end

  # Alias of GC.start
  def garbage_collect(full_mark: true, immediate_mark: true, immediate_sweep: true) end

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

  # Return consuming memory size of obj in bytes.
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

  # Return consuming memory size of all living objects in bytes.
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
  # This method is only expected to work with C Ruby.
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

  # An ObjectSpace::WeakKeyMap is a key-value map that holds weak references
  # to its keys, so they can be garbage collected when there is no more references.
  #
  # Unlike ObjectSpace::WeakMap:
  #
  # * references to values are _strong_, so they aren't garbage collected while
  #   they are in the map;
  # * keys are compared by value (using Object#eql?), not by identity;
  # * only garbage-collectable objects can be used as keys.
  #
  #      map = ObjectSpace::WeakKeyMap.new
  #      val = Time.new(2023, 12, 7)
  #      key = "name"
  #      map[key] = val
  #
  #      # Value is fetched by equality: the instance of string "name" is
  #      # different here, but it is equal to the key
  #      map["name"] #=> 2023-12-07 00:00:00 +0200
  #
  #      val = nil
  #      GC.start
  #      # There are no more references to `val`, yet the pair isn't
  #      # garbage-collected.
  #      map["name"] #=> 2023-12-07 00:00:00 +0200
  #
  #      key = nil
  #      GC.start
  #      # There are no more references to `key`, key and value are
  #      # garbage-collected.
  #      map["name"] #=> nil
  #
  # (Note that GC.start is used here only for demonstrational purposes and might
  # not always lead to demonstrated results.)
  #
  # The collection is especially useful for implementing caches of lightweight value
  # objects, so that only one copy of each value representation would be stored in
  # memory, but the copies that aren't used would be garbage-collected.
  #
  #   CACHE = ObjectSpace::WeakKeyMap
  #
  #   def make_value(**)
  #      val = ValueObject.new(**)
  #      if (existing = @cache.getkey(val))
  #         # if the object with this value exists, we return it
  #         existing
  #      else
  #         # otherwise, put it in the cache
  #         @cache[val] = true
  #         val
  #      end
  #   end
  #
  # This will result in +make_value+ returning the same object for same set of attributes
  # always, but the values that aren't needed anymore wouldn't be sitting in the cache forever.
  class WeakKeyMap
    # Returns the value associated with the given +key+ if found.
    #
    # If +key+ is not found, returns +nil+.
    def [](key) end

    # Associates the given +value+ with the given +key+
    #
    # The reference to +key+ is weak, so when there is no other reference
    # to +key+ it may be garbage collected.
    #
    # If the given +key+ exists, replaces its value with the given +value+;
    # the ordering is not affected
    def []=(key, value) end

    # Removes all map entries; returns +self+.
    def clear; end

    # Deletes the entry for the given +key+ and returns its associated value.
    #
    # If no block is given and +key+ is found, deletes the entry and returns the associated value:
    #   m = ObjectSpace::WeakKeyMap.new
    #   key = "foo" # to hold reference to the key
    #   m[key] = 1
    #   m.delete("foo") # => 1
    #   m["foo"] # => nil
    #
    # If no block given and +key+ is not found, returns +nil+.
    #
    # If a block is given and +key+ is found, ignores the block,
    # deletes the entry, and returns the associated value:
    #   m = ObjectSpace::WeakKeyMap.new
    #   key = "foo" # to hold reference to the key
    #   m[key] = 2
    #   m.delete("foo") { |key| raise 'Will never happen'} # => 2
    #
    # If a block is given and +key+ is not found,
    # yields the +key+ to the block and returns the block's return value:
    #   m = ObjectSpace::WeakKeyMap.new
    #   m.delete("nosuch") { |key| "Key #{key} not found" } # => "Key nosuch not found"
    def delete(key) end

    # Returns the existing equal key if it exists, otherwise returns +nil+.
    #
    # This might be useful for implementing caches, so that only one copy of
    # some object would be used everywhere in the program:
    #
    #   value = {amount: 1, currency: 'USD'}
    #
    #   # Now if we put this object in a cache:
    #   cache = ObjectSpace::WeakKeyMap.new
    #   cache[value] = true
    #
    #   # ...we can always extract from there and use the same object:
    #   copy = cache.getkey({amount: 1, currency: 'USD'})
    #   copy.object_id == value.object_id #=> true
    def getkey(key) end

    # Returns a new String containing informations about the map:
    #
    #   m = ObjectSpace::WeakKeyMap.new
    #   m[key] = value
    #   m.inspect # => "#<ObjectSpace::WeakKeyMap:0x00000001028dcba8 size=1>"
    def inspect; end

    # Returns +true+ if +key+ is a key in +self+, otherwise +false+.
    def key?(key) end
  end

  # An ObjectSpace::WeakMap is a key-value map that holds weak references
  # to its keys and values, so they can be garbage-collected when there are
  # no more references left.
  #
  # Keys in the map are compared by identity.
  #
  #    m = ObjectSpace::WeakMap.new
  #    key1 = "foo"
  #    val1 = Object.new
  #    m[key1] = val1
  #
  #    key2 = "bar"
  #    val2 = Object.new
  #    m[key2] = val2
  #
  #    m[key1] #=> #<Object:0x0...>
  #    m[key2] #=> #<Object:0x0...>
  #
  #    val1 = nil # remove the other reference to value
  #    GC.start
  #
  #    m[key1] #=> nil
  #    m.keys #=> ["bar"]
  #
  #    key2 = nil # remove the other reference to key
  #    GC.start
  #
  #    m[key2] #=> nil
  #    m.keys #=> []
  #
  # (Note that GC.start is used here only for demonstrational purposes and might
  # not always lead to demonstrated results.)
  #
  # See also ObjectSpace::WeakKeyMap map class, which compares keys by value,
  # and holds weak references only to the keys.
  class WeakMap
    include Enumerable

    # Returns the value associated with the given +key+ if found.
    #
    # If +key+ is not found, returns +nil+.
    def [](key) end

    # Associates the given +value+ with the given +key+.
    #
    # If the given +key+ exists, replaces its value with the given +value+;
    # the ordering is not affected.
    def []=(key, value) end

    # Deletes the entry for the given +key+ and returns its associated value.
    #
    # If no block is given and +key+ is found, deletes the entry and returns the associated value:
    #   m = ObjectSpace::WeakMap.new
    #   key = "foo"
    #   m[key] = 1
    #   m.delete(key) # => 1
    #   m[key] # => nil
    #
    # If no block is given and +key+ is not found, returns +nil+.
    #
    # If a block is given and +key+ is found, ignores the block,
    # deletes the entry, and returns the associated value:
    #   m = ObjectSpace::WeakMap.new
    #   key = "foo"
    #   m[key] = 2
    #   m.delete(key) { |key| raise 'Will never happen'} # => 2
    #
    # If a block is given and +key+ is not found,
    # yields the +key+ to the block and returns the block's return value:
    #   m = ObjectSpace::WeakMap.new
    #   m.delete("nosuch") { |key| "Key #{key} not found" } # => "Key nosuch not found"
    def delete(key) end

    # Iterates over keys and values. Note that unlike other collections,
    # +each+ without block isn't supported.
    def each; end
    alias each_pair each

    # Iterates over keys. Note that unlike other collections,
    # +each_key+ without block isn't supported.
    def each_key; end

    # Iterates over values. Note that unlike other collections,
    # +each_value+ without block isn't supported.
    def each_value; end

    # Returns +true+ if +key+ is a key in +self+, otherwise +false+.
    def include?(p1) end
    alias member? include?
    alias key? include?

    def inspect; end

    # Returns a new Array containing all keys in the map.
    def keys; end

    # Returns the number of referenced objects
    def size; end
    alias length size

    # Returns a new Array containing all values in the map.
    def values; end
  end
end
