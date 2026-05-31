# frozen_string_literal: true

# The GC module provides an interface to Ruby's mark and
# sweep garbage collection mechanism.
#
# Some of the underlying methods are also available via the ObjectSpace
# module.
#
# You may obtain information about the operation of the \GC through
# GC::Profiler.
module GC
  # internal constants
  INTERNAL_CONSTANTS = _
  # \GC build options
  OPTS = _

  # Raises NoMemoryError when allocating an instance of the given classes.
  def self.add_stress_to_class(*args) end

  # Returns whether or not automatic compaction has been enabled.
  def self.auto_compact; end

  # Updates automatic compaction mode.
  #
  # When enabled, the compactor will execute on every major collection.
  #
  # Enabling compaction will degrade performance on major collections.
  def self.auto_compact=(flag) end

  # This function compacts objects together in Ruby's heap.  It eliminates
  # unused space (or fragmentation) in the heap by moving objects in to that
  # unused space.  This function returns a hash which contains statistics about
  # which objects were moved.  See <tt>GC.latest_gc_info</tt> for details about
  # compaction statistics.
  #
  # This method is implementation specific and not expected to be implemented
  # in any implementation besides MRI.
  #
  # To test whether \GC compaction is supported, use the idiom:
  #
  #   GC.respond_to?(:compact)
  def self.compact; end

  # The number of times \GC occurred.
  #
  # It returns the number of times \GC occurred since the process started.
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
  # The returned hash has two keys :considered and :moved.  The hash for
  # :considered lists the number of objects that were considered for movement
  # by the compactor, and the :moved hash lists the number of objects that
  # were actually moved.  Some objects can't be moved (maybe they were pinned)
  # so these numbers can be used to calculate compaction efficiency.
  def self.latest_compact_info; end

  # Returns information about the most recent garbage collection.
  #
  # If the optional argument, hash, is given,
  # it is overwritten and returned.
  # This is intended to avoid probe effect.
  def self.latest_gc_info(...) end

  # Returns the size of memory allocated by malloc().
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocated_size; end

  # Returns the number of malloc() allocations.
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocations; end

  # Return measure_total_time flag (default: +true+).
  # Note that measurement can affect the application performance.
  def self.measure_total_time; end

  # Enable to measure \GC time.
  # You can get the result with <tt>GC.stat(:time)</tt>.
  # Note that \GC time measurement can cause some performance overhead.
  def self.measure_total_time=(flag) end

  # No longer raises NoMemoryError when allocating an instance of the
  # given classes.
  def self.remove_stress_to_class(*args) end

  # Initiates garbage collection, even if manually disabled.
  #
  # This method is defined with keyword arguments that default to true:
  #
  #    def GC.start(full_mark: true, immediate_sweep: true); end
  #
  # Use full_mark: false to perform a minor \GC.
  # Use immediate_sweep: false to defer sweeping (use lazy sweep).
  #
  # Note: These keyword arguments are implementation and version dependent. They
  # are not guaranteed to be future-compatible, and may be ignored if the
  # underlying implementation does not support them.
  def self.start(...) end

  # Returns a Hash containing information about the \GC.
  #
  # The contents of the hash are implementation specific and may change in
  # the future without notice.
  #
  # The hash includes information about internal statistics about \GC such as:
  #
  # [count]
  #   The total number of garbage collections ran since application start
  #   (count includes both minor and major garbage collections)
  # [time]
  #   The total time spent in garbage collections (in milliseconds)
  # [heap_allocated_pages]
  #   The total number of `:heap_eden_pages` + `:heap_tomb_pages`
  # [heap_sorted_length]
  #   The number of pages that can fit into the buffer that holds references to
  #   all pages
  # [heap_allocatable_pages]
  #   The total number of pages the application could allocate without additional \GC
  # [heap_available_slots]
  #   The total number of slots in all `:heap_allocated_pages`
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
  #   When `:malloc_increase_bytes` crosses this limit, \GC is triggered
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
  #   When `:remembered_wb_unprotected_objects` crosses this limit,
  #   major \GC is triggered
  # [old_objects]
  #   Number of live, old objects which have survived at least 3 garbage collections
  # [old_objects_limit]
  #   When `:old_objects` crosses this limit, major \GC is triggered
  # [oldmalloc_increase_bytes]
  #   Amount of memory allocated on the heap for objects. Decreased by major \GC
  # [oldmalloc_increase_bytes_limit]
  #   When `:old_malloc_increase_bytes` crosses this limit, major \GC is triggered
  #
  # If the optional argument, hash, is given,
  # it is overwritten and returned.
  # This is intended to avoid probe effect.
  #
  # This method is only expected to work on CRuby.
  def self.stat(...) end

  # Returns information for memory pools in the \GC.
  #
  # If the first optional argument, +heap_name+, is passed in and not +nil+, it
  # returns a +Hash+ containing information about the particular memory pool.
  # Otherwise, it will return a +Hash+ with memory pool names as keys and
  # a +Hash+ containing information about the memory pool as values.
  #
  # If the second optional argument, +hash_or_key+, is given as +Hash+, it will
  # be overwritten and returned. This is intended to avoid the probe effect.
  #
  # If both optional arguments are passed in and the second optional argument is
  # a symbol, it will return a +Numeric+ of the value for the particular memory
  # pool.
  #
  # On CRuby, +heap_name+ is of the type +Integer+ but may be of type +String+
  # on other implementations.
  #
  # The contents of the hash are implementation specific and may change in
  # the future without notice.
  #
  # If the optional argument, hash, is given, it is overwritten and returned.
  #
  # This method is only expected to work on CRuby.
  def self.stat_heap(...) end

  # Returns current status of \GC stress mode.
  def self.stress; end

  # Updates the \GC stress mode.
  #
  # When stress mode is enabled, the \GC is invoked at every \GC opportunity:
  # all memory and object allocations.
  #
  # Enabling stress mode will degrade performance, it is only for debugging.
  #
  # flag can be true, false, or an integer bit-ORed following flags.
  #   0x01:: no major GC
  #   0x02:: no immediate sweep
  #   0x04:: full mark after malloc/calloc/realloc
  def self.stress=(flag) end

  # Return measured \GC total time in nano seconds.
  def self.total_time; end

  # Returns true if using experimental feature Variable Width Allocation, false
  # otherwise.
  def self.using_rvargc?; end

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

  def self.verify_transient_heap_internal_consistency; end

  def garbage_collect(full_mark: true, immediate_mark: true, immediate_sweep: true) end

  # The GC profiler provides access to information on GC runs including time,
  # length and object space size.
  #
  # Example:
  #
  #   GC::Profiler.enable
  #
  #   require 'rdoc/rdoc'
  #
  #   GC::Profiler.report
  #
  #   GC::Profiler.disable
  #
  # See also GC.count, GC.malloc_allocated_size and GC.malloc_allocations
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
