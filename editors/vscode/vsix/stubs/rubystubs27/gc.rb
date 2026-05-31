# frozen_string_literal: true

# The GC module provides an interface to Ruby's mark and
# sweep garbage collection mechanism.
#
# Some of the underlying methods are also available via the ObjectSpace
# module.
#
# You may obtain information about the operation of the GC through
# GC::Profiler.
module GC
  # internal constants
  INTERNAL_CONSTANTS = _
  # GC build options
  OPTS = _

  # Raises NoMemoryError when allocating an instance of the given classes.
  def self.add_stress_to_class(*args) end

  def self.compact; end

  # The number of times GC occurred.
  #
  # It returns the number of times GC occurred since the process started.
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

  # Returns information about the most recent garbage collection.
  def self.latest_gc_info(...) end

  # Returns the size of memory allocated by malloc().
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocated_size; end

  # Returns the number of malloc() allocations.
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocations; end

  # No longer raises NoMemoryError when allocating an instance of the
  # given classes.
  def self.remove_stress_to_class(*args) end

  # Initiates garbage collection, even if manually disabled.
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
  def self.start(...) end

  # Returns a Hash containing information about the GC.
  #
  # The hash includes information about internal statistics about GC such as:
  #
  #     {
  #         :count=>0,
  #         :heap_allocated_pages=>24,
  #         :heap_sorted_length=>24,
  #         :heap_allocatable_pages=>0,
  #         :heap_available_slots=>9783,
  #         :heap_live_slots=>7713,
  #         :heap_free_slots=>2070,
  #         :heap_final_slots=>0,
  #         :heap_marked_slots=>0,
  #         :heap_eden_pages=>24,
  #         :heap_tomb_pages=>0,
  #         :total_allocated_pages=>24,
  #         :total_freed_pages=>0,
  #         :total_allocated_objects=>7796,
  #         :total_freed_objects=>83,
  #         :malloc_increase_bytes=>2389312,
  #         :malloc_increase_bytes_limit=>16777216,
  #         :minor_gc_count=>0,
  #         :major_gc_count=>0,
  #         :remembered_wb_unprotected_objects=>0,
  #         :remembered_wb_unprotected_objects_limit=>0,
  #         :old_objects=>0,
  #         :old_objects_limit=>0,
  #         :oldmalloc_increase_bytes=>2389760,
  #         :oldmalloc_increase_bytes_limit=>16777216
  #     }
  #
  # The contents of the hash are implementation specific and may be changed in
  # the future.
  #
  # This method is only expected to work on C Ruby.
  def self.stat(...) end

  # Returns current status of GC stress mode.
  def self.stress; end

  # Updates the GC stress mode.
  #
  # When stress mode is enabled, the GC is invoked at every GC opportunity:
  # all memory and object allocations.
  #
  # Enabling stress mode will degrade performance, it is only for debugging.
  #
  # flag can be true, false, or an integer bit-ORed following flags.
  #   0x01:: no major GC
  #   0x02:: no immediate sweep
  #   0x04:: full mark after malloc/calloc/realloc
  def self.stress=(flag) end

  # Verify compaction reference consistency.
  #
  # This method is implementation specific.  During compaction, objects that
  # were moved are replaced with T_MOVED objects.  No object should have a
  # reference to a T_MOVED object after compaction.
  #
  # This function doubles the heap to ensure room to move all objects,
  # compacts the heap to make sure everything moves, updates all references,
  # then performs a full GC.  If any object contains a reference to a T_MOVED
  # object, that object should be pushed on the mark stack, and will
  # make a SEGV.
  def self.verify_compaction_references(toward: nil, double_heap: nil) end

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
    # Clears the GC profiler data.
    def self.clear; end

    # Stops the GC profiler.
    def self.disable; end

    # Starts the GC profiler.
    def self.enable; end

    # The current status of GC profile mode.
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
