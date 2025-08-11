# frozen_string_literal: true

# The \GC module provides an interface to Ruby's mark-and-sweep garbage collection mechanism.
#
# Some of the underlying methods are also available via the ObjectSpace module.
#
# You may obtain information about the operation of the \GC through GC::Profiler.
module GC
  # Internal constants in the garbage collector.
  INTERNAL_CONSTANTS = _
  # \GC build options
  OPTS = _

  # Returns whether or not automatic compaction has been enabled.
  def self.auto_compact; end

  # Updates automatic compaction mode.
  #
  # When enabled, the compactor will execute on every major collection.
  #
  # Enabling compaction will degrade performance on major collections.
  def self.auto_compact=(flag) end

  # This function compacts objects together in Ruby's heap. It eliminates
  # unused space (or fragmentation) in the heap by moving objects in to that
  # unused space.
  #
  # The returned +hash+ contains statistics about the objects that were moved;
  # see GC.latest_compact_info.
  #
  # This method is only expected to work on CRuby.
  #
  # To test whether \GC compaction is supported, use the idiom:
  #
  #   GC.respond_to?(:compact)
  def self.compact; end

  # Sets or gets information about the current \GC config.
  #
  # Configuration parameters are \GC implementation-specific and may change
  # without notice.
  #
  # This method can be called without parameters to retrieve the current config
  # as a +Hash+ with +Symbol+ keys.
  #
  # This method can also be called with a +Hash+ argument to assign values to
  # valid config keys. Config keys missing from the passed +Hash+ will be left
  # unmodified.
  #
  # If a key/value pair is passed to this function that does not correspond to
  # a valid config key for the \GC implementation being used, no config will be
  # updated, the key will be present in the returned Hash, and its value will
  # be +nil+. This is to facilitate easy migration between \GC implementations.
  #
  # In both call-seqs, the return value of <code>GC.config</code> will be a +Hash+
  # containing the most recent full configuration, i.e., all keys and values
  # defined by the specific \GC implementation being used. In the case of a
  # config update, the return value will include the new values being updated.
  #
  # This method is only expected to work on CRuby.
  #
  # === \GC Implementation independent values
  #
  # The <code>GC.config</code> hash can also contain keys that are global and
  # read-only. These keys are not specific to any one \GC library implementation
  # and attempting to write to them will raise +ArgumentError+.
  #
  # There is currently only one global, read-only key:
  #
  # [implementation]
  #    Returns a +String+ containing the name of the currently loaded \GC library,
  #    if one has been loaded using +RUBY_GC_LIBRARY+, and "default" in all other
  #    cases
  #
  # === \GC Implementation specific values
  #
  # \GC libraries are expected to document their own configuration. Valid keys
  # for Ruby's default \GC implementation are:
  #
  # [rgengc_allow_full_mark]
  #   Controls whether the \GC is allowed to run a full mark (young & old objects).
  #
  #   When +true+, \GC interleaves major and minor collections. This is the default. \GC
  #   will function as intended.
  #
  #   When +false+, the \GC will never trigger a full marking cycle unless
  #   explicitly requested by user code. Instead, only a minor mark will run—
  #   only young objects will be marked. When the heap space is exhausted, new
  #   pages will be allocated immediately instead of running a full mark.
  #
  #   A flag will be set to notify that a full mark has been
  #   requested. This flag is accessible using
  #   <code>GC.latest_gc_info(:needs_major_by)</code>
  #
  #   The user can trigger a major collection at any time using
  #   <code>GC.start(full_mark: true)</code>
  #
  #   When +false+, Young to Old object promotion is disabled. For performance
  #   reasons, it is recommended to warm up an application using +Process.warmup+
  #   before setting this parameter to +false+.
  def self.config(...) end

  # Returns the number of times \GC has occurred since the process started.
  def self.count; end

  # Disables garbage collection, returning +true+ if garbage
  # collection was already disabled.
  #
  #    GC.disable   #=> false
  #    GC.disable   #=> true
  def self.disable; end

  # Enables garbage collection, returning +true+ if garbage
  # collection was previously disabled.
  #
  #    GC.disable   #=> false
  #    GC.enable    #=> true
  #    GC.enable    #=> false
  def self.enable; end

  # Returns information about object moved in the most recent \GC compaction.
  #
  # The returned +hash+ contains the following keys:
  #
  # [considered]
  #   Hash containing the type of the object as the key and the number of
  #   objects of that type that were considered for movement.
  # [moved]
  #   Hash containing the type of the object as the key and the number of
  #   objects of that type that were actually moved.
  # [moved_up]
  #   Hash containing the type of the object as the key and the number of
  #   objects of that type that were increased in size.
  # [moved_down]
  #   Hash containing the type of the object as the key and the number of
  #   objects of that type that were decreased in size.
  #
  # Some objects can't be moved (due to pinning) so these numbers can be used to
  # calculate compaction efficiency.
  def self.latest_compact_info; end

  # Returns information about the most recent garbage collection.
  #
  # If the argument +hash+ is given and is a Hash object,
  # it is overwritten and returned.
  # This is intended to avoid the probe effect.
  #
  # If the argument +key+ is given and is a Symbol object,
  # it returns the value associated with the key.
  # This is equivalent to <tt>GC.latest_gc_info[key]</tt>.
  def self.latest_gc_info(...) end

  # Returns the size of memory allocated by malloc().
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocated_size; end

  # Returns the number of malloc() allocations.
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocations; end

  # Returns the measure_total_time flag (default: +true+).
  # Note that measurement can affect the application's performance.
  def self.measure_total_time; end

  # Enables measuring \GC time.
  # You can get the result with <tt>GC.stat(:time)</tt>.
  # Note that \GC time measurement can cause some performance overhead.
  def self.measure_total_time=(flag) end

  # Initiates garbage collection, even if manually disabled.
  #
  # The +full_mark+ keyword argument determines whether or not to perform a
  # major garbage collection cycle. When set to +true+, a major garbage
  # collection cycle is run, meaning all objects are marked. When set to
  # +false+, a minor garbage collection cycle is run, meaning only young
  # objects are marked.
  #
  # The +immediate_mark+ keyword argument determines whether or not to perform
  # incremental marking. When set to +true+, marking is completed during the
  # call to this method. When set to +false+, marking is performed in steps
  # that are interleaved with future Ruby code execution, so marking might not
  # be completed during this method call. Note that if +full_mark+ is +false+,
  # then marking will always be immediate, regardless of the value of
  # +immediate_mark+.
  #
  # The +immediate_sweep+ keyword argument determines whether or not to defer
  # sweeping (using lazy sweep). When set to +false+, sweeping is performed in
  # steps that are interleaved with future Ruby code execution, so sweeping might
  # not be completed during this method call. When set to +true+, sweeping is
  # completed during the call to this method.
  #
  # Note: These keyword arguments are implementation and version-dependent. They
  # are not guaranteed to be future-compatible and may be ignored if the
  # underlying implementation does not support them.
  def self.start(full_mark: true, immediate_mark: true, immediate_sweep: true) end

  # Returns a Hash containing information about the \GC.
  #
  # The contents of the hash are implementation-specific and may change in
  # the future without notice.
  #
  # The hash includes internal statistics about \GC such as:
  #
  # [count]
  #   The total number of garbage collections run since application start
  #   (count includes both minor and major garbage collections)
  # [time]
  #   The total time spent in garbage collections (in milliseconds)
  # [heap_allocated_pages]
  #   The total number of +:heap_eden_pages+ + +:heap_tomb_pages+
  # [heap_sorted_length]
  #   The number of pages that can fit into the buffer that holds references to
  #   all pages
  # [heap_allocatable_pages]
  #   The total number of pages the application could allocate without additional \GC
  # [heap_available_slots]
  #   The total number of slots in all +:heap_allocated_pages+
  # [heap_live_slots]
  #   The total number of slots which contain live objects
  # [heap_free_slots]
  #   The total number of slots which do not contain live objects
  # [heap_final_slots]
  #   The total number of slots with pending finalizers to be run
  # [heap_marked_slots]
  #   The total number of objects marked in the last \GC
  # [heap_eden_pages]
  #   The total number of pages which contain at least one live slot
  # [heap_tomb_pages]
  #   The total number of pages which do not contain any live slots
  # [total_allocated_pages]
  #   The cumulative number of pages allocated since application start
  # [total_freed_pages]
  #   The cumulative number of pages freed since application start
  # [total_allocated_objects]
  #   The cumulative number of objects allocated since application start
  # [total_freed_objects]
  #   The cumulative number of objects freed since application start
  # [malloc_increase_bytes]
  #   Amount of memory allocated on the heap for objects. Decreased by any \GC
  # [malloc_increase_bytes_limit]
  #   When +:malloc_increase_bytes+ crosses this limit, \GC is triggered
  # [minor_gc_count]
  #   The total number of minor garbage collections run since process start
  # [major_gc_count]
  #   The total number of major garbage collections run since process start
  # [compact_count]
  #   The total number of compactions run since process start
  # [read_barrier_faults]
  #   The total number of times the read barrier was triggered during
  #   compaction
  # [total_moved_objects]
  #   The total number of objects compaction has moved
  # [remembered_wb_unprotected_objects]
  #   The total number of objects without write barriers
  # [remembered_wb_unprotected_objects_limit]
  #   When +:remembered_wb_unprotected_objects+ crosses this limit,
  #   major \GC is triggered
  # [old_objects]
  #   Number of live, old objects which have survived at least 3 garbage collections
  # [old_objects_limit]
  #   When +:old_objects+ crosses this limit, major \GC is triggered
  # [oldmalloc_increase_bytes]
  #   Amount of memory allocated on the heap for objects. Decreased by major \GC
  # [oldmalloc_increase_bytes_limit]
  #   When +:oldmalloc_increase_bytes+ crosses this limit, major \GC is triggered
  #
  # If the optional argument, hash, is given,
  # it is overwritten and returned.
  # This is intended to avoid the probe effect.
  #
  # This method is only expected to work on CRuby.
  def self.stat(...) end

  # Returns information for heaps in the \GC.
  #
  # If the first optional argument, +heap_name+, is passed in and not +nil+, it
  # returns a +Hash+ containing information about the particular heap.
  # Otherwise, it will return a +Hash+ with heap names as keys and
  # a +Hash+ containing information about the heap as values.
  #
  # If the second optional argument, +hash_or_key+, is given as a +Hash+, it will
  # be overwritten and returned. This is intended to avoid the probe effect.
  #
  # If both optional arguments are passed in and the second optional argument is
  # a symbol, it will return a +Numeric+ value for the particular heap.
  #
  # On CRuby, +heap_name+ is of the type +Integer+ but may be of type +String+
  # on other implementations.
  #
  # The contents of the hash are implementation-specific and may change in
  # the future without notice.
  #
  # If the optional argument, hash, is given, it is overwritten and returned.
  #
  # This method is only expected to work on CRuby.
  #
  # The hash includes the following keys about the internal information in
  # the \GC:
  #
  # [slot_size]
  #   The slot size of the heap in bytes.
  # [heap_allocatable_pages]
  #   The number of pages that can be allocated without triggering a new
  #   garbage collection cycle.
  # [heap_eden_pages]
  #   The number of pages in the eden heap.
  # [heap_eden_slots]
  #   The total number of slots in all of the pages in the eden heap.
  # [heap_tomb_pages]
  #   The number of pages in the tomb heap. The tomb heap only contains pages
  #   that do not have any live objects.
  # [heap_tomb_slots]
  #   The total number of slots in all of the pages in the tomb heap.
  # [total_allocated_pages]
  #   The total number of pages that have been allocated in the heap.
  # [total_freed_pages]
  #   The total number of pages that have been freed and released back to the
  #   system in the heap.
  # [force_major_gc_count]
  #   The number of times this heap has forced major garbage collection cycles
  #   to start due to running out of free slots.
  # [force_incremental_marking_finish_count]
  #   The number of times this heap has forced incremental marking to complete
  #   due to running out of pooled slots.
  def self.stat_heap(...) end

  # Returns the current status of \GC stress mode.
  def self.stress; end

  # Updates the \GC stress mode.
  #
  # When stress mode is enabled, the \GC is invoked at every \GC opportunity:
  # all memory and object allocations.
  #
  # Enabling stress mode will degrade performance; it is only for debugging.
  #
  # The flag can be true, false, or an integer bitwise-ORed with the following flags:
  #   0x01:: no major GC
  #   0x02:: no immediate sweep
  #   0x04:: full mark after malloc/calloc/realloc
  def self.stress=(flag) end

  # Returns the measured \GC total time in nanoseconds.
  def self.total_time; end

  # Verify compaction reference consistency.
  #
  # This method is implementation specific.  During compaction, objects that
  # were moved are replaced with T_MOVED objects.  No object should have a
  # reference to a T_MOVED object after compaction.
  #
  # This function expands the heap to ensure room to move all objects,
  # compacts the heap to make sure everything moves, updates all references,
  # then performs a full \GC.  If any object contains a reference to a T_MOVED
  # object, that object should be pushed on the mark stack, and will
  # make a SEGV.
  def self.verify_compaction_references(toward: nil, double_heap: false) end

  # Verify internal consistency.
  #
  # This method is implementation specific.
  # Now this method checks generational consistency
  # if RGenGC is supported.
  def self.verify_internal_consistency; end

  # Alias of GC.start
  def garbage_collect(full_mark: true, immediate_mark: true, immediate_sweep: true) end

  module Profiler
    # Clears the \GC profiler data.
    def self.clear; end

    # Stops the \GC profiler.
    def self.disable; end

    # Starts the \GC profiler.
    def self.enable; end

    # The current status of \GC profile mode.
    def self.enabled?; end

    # Returns an Array of individual raw profile data Hashes ordered
    # from earliest to latest by +:GC_INVOKE_TIME+.
    #
    # For example:
    #
    #   [
    #     {
    #        :GC_TIME=>1.3000000000000858e-05,
    #        :GC_INVOKE_TIME=>0.010634999999999999,
    #        :HEAP_USE_SIZE=>289640,
    #        :HEAP_TOTAL_SIZE=>588960,
    #        :HEAP_TOTAL_OBJECTS=>14724,
    #        :GC_IS_MARKED=>false
    #     },
    #     # ...
    #   ]
    #
    # The keys mean:
    #
    # +:GC_TIME+::
    #     Time elapsed in seconds for this GC run
    # +:GC_INVOKE_TIME+::
    #     Time elapsed in seconds from startup to when the GC was invoked
    # +:HEAP_USE_SIZE+::
    #     Total bytes of heap used
    # +:HEAP_TOTAL_SIZE+::
    #     Total size of heap in bytes
    # +:HEAP_TOTAL_OBJECTS+::
    #     Total number of objects
    # +:GC_IS_MARKED+::
    #     Returns +true+ if the GC is in mark phase
    #
    # If ruby was built with +GC_PROFILE_MORE_DETAIL+, you will also have access
    # to the following hash keys:
    #
    # +:GC_MARK_TIME+::
    # +:GC_SWEEP_TIME+::
    # +:ALLOCATE_INCREASE+::
    # +:ALLOCATE_LIMIT+::
    # +:HEAP_USE_PAGES+::
    # +:HEAP_LIVE_OBJECTS+::
    # +:HEAP_FREE_OBJECTS+::
    # +:HAVE_FINALIZE+::
    def self.raw_data; end

    # Writes the GC::Profiler.result to <tt>$stdout</tt> or the given IO object.
    def self.report(...) end

    # Returns a profile data report such as:
    #
    #   GC 1 invokes.
    #   Index    Invoke Time(sec)       Use Size(byte)     Total Size(byte)         Total Object                    GC time(ms)
    #       1               0.012               159240               212940                10647         0.00000000000001530000
    def self.result; end

    # The total time used for garbage collection in seconds
    def self.total_time; end
  end
end
